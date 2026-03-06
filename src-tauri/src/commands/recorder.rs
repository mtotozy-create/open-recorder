use std::{
    fs,
    io::BufWriter,
    path::{Path, PathBuf},
    process::Command,
    sync::{mpsc, Arc, Mutex, OnceLock},
    thread,
    time::Duration,
};

use chrono::Utc;
use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    SampleFormat, SampleRate,
};
use hound::{SampleFormat as WavSampleFormat, WavReader, WavSpec, WavWriter};
use tauri::State;
use uuid::Uuid;

use crate::{
    models::{
        AudioSegmentMeta, RecorderExportResponse, RecorderStatus, RecordingQualityPreset, Session,
        SessionStatus, StartSessionResponse,
    },
    state::AppState,
    storage::Storage,
};

const SEGMENT_DURATION_SECS: u64 = 600;
const COMMAND_TIMEOUT: Duration = Duration::from_secs(5);

struct QualityConfig {
    sample_rate: u32,
    channels: u16,
}

struct OpenSegment {
    path: String,
    sequence: u32,
    started_at: String,
    start_elapsed_ms: u64,
}

struct RecorderShared {
    session_id: String,
    sample_rate: u32,
    channels: u16,
    chunk_frames: u64,
    total_frames: u64,
    current_segment_frames: u64,
    next_sequence: u32,
    writer: Option<WavWriter<BufWriter<fs::File>>>,
    open_segment: Option<OpenSegment>,
    last_rms: f32,
    last_peak: f32,
    last_error: Option<String>,
}

type CommandReply = mpsc::Sender<Result<(), String>>;

enum AudioThreadCommand {
    Pause(CommandReply),
    Resume(CommandReply),
    Stop(CommandReply),
}

struct ActiveRecorder {
    session_id: String,
    control_tx: mpsc::Sender<AudioThreadCommand>,
    shared: Arc<Mutex<RecorderShared>>,
}

enum RecorderRuntime {
    Idle,
    Active(ActiveRecorder),
}

fn recorder_runtime() -> &'static Mutex<RecorderRuntime> {
    static RUNTIME: OnceLock<Mutex<RecorderRuntime>> = OnceLock::new();
    RUNTIME.get_or_init(|| Mutex::new(RecorderRuntime::Idle))
}

fn now_iso() -> String {
    Utc::now().to_rfc3339()
}

fn quality_config(preset: &RecordingQualityPreset) -> QualityConfig {
    match preset {
        RecordingQualityPreset::Standard => QualityConfig {
            sample_rate: 16_000,
            channels: 1,
        },
        RecordingQualityPreset::Hd => QualityConfig {
            sample_rate: 24_000,
            channels: 1,
        },
        RecordingQualityPreset::Hifi => QualityConfig {
            sample_rate: 48_000,
            channels: 2,
        },
    }
}

fn frames_to_ms(frames: u64, sample_rate: u32) -> u64 {
    if sample_rate == 0 {
        return 0;
    }

    ((frames as u128) * 1000 / (sample_rate as u128)) as u64
}

fn elapsed_ms(shared: &RecorderShared) -> u64 {
    frames_to_ms(shared.total_frames, shared.sample_rate)
}

fn segment_file_path(
    storage: &Storage,
    session_id: &str,
    sequence: u32,
) -> Result<PathBuf, String> {
    let segment_dir = storage.session_audio_dir(session_id)?;
    let timestamp = Utc::now().format("%Y%m%d-%H%M%S");
    Ok(segment_dir.join(format!("segment-{timestamp}-{sequence:06}.wav")))
}

fn open_next_segment(shared: &mut RecorderShared, storage: &Storage) -> Result<(), String> {
    if shared.writer.is_some() {
        return Ok(());
    }

    let sequence = shared.next_sequence;
    let segment_path = segment_file_path(storage, &shared.session_id, sequence)?;
    let wav_spec = WavSpec {
        channels: shared.channels,
        sample_rate: shared.sample_rate,
        bits_per_sample: 16,
        sample_format: WavSampleFormat::Int,
    };

    let writer = WavWriter::create(&segment_path, wav_spec).map_err(|error| {
        format!(
            "failed to create segment {}: {error}",
            segment_path.display()
        )
    })?;

    shared.writer = Some(writer);
    shared.open_segment = Some(OpenSegment {
        path: segment_path.to_string_lossy().to_string(),
        sequence,
        started_at: now_iso(),
        start_elapsed_ms: elapsed_ms(shared),
    });
    shared.current_segment_frames = 0;
    shared.next_sequence += 1;

    Ok(())
}

fn close_open_segment(
    shared: &mut RecorderShared,
    storage: &Arc<Mutex<Storage>>,
) -> Result<(), String> {
    let writer = match shared.writer.take() {
        Some(writer) => writer,
        None => return Ok(()),
    };

    writer
        .finalize()
        .map_err(|error| format!("failed to finalize wav writer: {error}"))?;

    let open_segment = match shared.open_segment.take() {
        Some(segment) => segment,
        None => return Ok(()),
    };

    let finished_elapsed_ms = elapsed_ms(shared);
    let duration_ms = finished_elapsed_ms.saturating_sub(open_segment.start_elapsed_ms);

    let meta = AudioSegmentMeta {
        path: open_segment.path.clone(),
        sequence: open_segment.sequence,
        started_at: open_segment.started_at,
        ended_at: now_iso(),
        duration_ms,
        sample_rate: shared.sample_rate,
        channels: shared.channels,
    };

    let mut state = storage
        .lock()
        .map_err(|_| "failed to acquire storage lock".to_string())?;

    let session = state
        .data
        .sessions
        .get_mut(&shared.session_id)
        .ok_or_else(|| "session not found".to_string())?;

    session.audio_segments.push(meta.path.clone());
    session.audio_segment_meta.push(meta);
    session.elapsed_ms = finished_elapsed_ms;
    session.sample_rate = shared.sample_rate;
    session.channels = shared.channels;
    session.updated_at = now_iso();

    state.save()?;

    Ok(())
}

fn rotate_segment_if_needed(shared: &mut RecorderShared, storage: &Arc<Mutex<Storage>>) {
    if shared.current_segment_frames < shared.chunk_frames {
        return;
    }

    if let Err(error) = close_open_segment(shared, storage) {
        shared.last_error = Some(error);
        return;
    }

    let mut storage_lock = match storage.lock() {
        Ok(value) => value,
        Err(_) => {
            shared.last_error = Some("failed to acquire storage lock".to_string());
            return;
        }
    };

    if let Err(error) = open_next_segment(shared, &storage_lock) {
        shared.last_error = Some(error);
        return;
    }

    if let Some(session) = storage_lock.data.sessions.get_mut(&shared.session_id) {
        session.updated_at = now_iso();
    }

    if let Err(error) = storage_lock.save() {
        shared.last_error = Some(error);
    }
}

fn process_i16_samples(
    data: &[i16],
    shared: &Arc<Mutex<RecorderShared>>,
    storage: &Arc<Mutex<Storage>>,
) {
    if data.is_empty() {
        return;
    }

    let mut state = match shared.lock() {
        Ok(value) => value,
        Err(_) => return,
    };

    let channels = usize::from(state.channels.max(1));
    let frames = (data.len() / channels) as u64;

    let mut square_sum: f64 = 0.0;
    let mut peak: f32 = 0.0;

    if let Some(writer) = state.writer.as_mut() {
        for sample in data {
            if let Err(error) = writer.write_sample(*sample) {
                state.last_error = Some(format!("failed to write wav sample: {error}"));
                return;
            }

            let normalized = (*sample as f32 / i16::MAX as f32).abs().min(1.0);
            square_sum += f64::from(normalized * normalized);
            if normalized > peak {
                peak = normalized;
            }
        }
    } else {
        return;
    }

    state.total_frames = state.total_frames.saturating_add(frames);
    state.current_segment_frames = state.current_segment_frames.saturating_add(frames);

    let sample_count = data.len() as f64;
    state.last_rms = if sample_count > 0.0 {
        (square_sum / sample_count).sqrt() as f32
    } else {
        0.0
    };
    state.last_peak = peak;

    rotate_segment_if_needed(&mut state, storage);
}

fn process_f32_samples(
    data: &[f32],
    shared: &Arc<Mutex<RecorderShared>>,
    storage: &Arc<Mutex<Storage>>,
) {
    let converted: Vec<i16> = data
        .iter()
        .map(|sample| {
            let clamped = sample.clamp(-1.0, 1.0);
            (clamped * i16::MAX as f32) as i16
        })
        .collect();
    process_i16_samples(&converted, shared, storage);
}

fn process_u16_samples(
    data: &[u16],
    shared: &Arc<Mutex<RecorderShared>>,
    storage: &Arc<Mutex<Storage>>,
) {
    let converted: Vec<i16> = data
        .iter()
        .map(|sample| ((*sample as i32) - 32768) as i16)
        .collect();
    process_i16_samples(&converted, shared, storage);
}

fn select_stream_config(
    device: &cpal::Device,
    target_sample_rate: u32,
    target_channels: u16,
) -> Result<(cpal::StreamConfig, SampleFormat), String> {
    if let Ok(ranges) = device.supported_input_configs() {
        for range in ranges {
            if range.channels() == target_channels
                && range.min_sample_rate().0 <= target_sample_rate
                && range.max_sample_rate().0 >= target_sample_rate
            {
                let config = range.with_sample_rate(SampleRate(target_sample_rate));
                return Ok((config.config(), config.sample_format()));
            }
        }
    }

    let default = device
        .default_input_config()
        .map_err(|error| format!("failed to get default input config: {error}"))?;
    Ok((default.config(), default.sample_format()))
}

fn update_session_status(
    storage: &Arc<Mutex<Storage>>,
    session_id: &str,
    status: SessionStatus,
    elapsed_ms: u64,
) -> Result<(), String> {
    let mut state = storage
        .lock()
        .map_err(|_| "failed to acquire storage lock".to_string())?;
    let session = state
        .data
        .sessions
        .get_mut(session_id)
        .ok_or_else(|| "session not found".to_string())?;
    session.status = status;
    session.elapsed_ms = elapsed_ms;
    session.updated_at = now_iso();
    state.save()?;
    Ok(())
}

fn send_control_command(
    control_tx: &mpsc::Sender<AudioThreadCommand>,
    command: AudioThreadCommand,
) -> Result<(), String> {
    control_tx
        .send(command)
        .map_err(|_| "failed to send audio control command".to_string())
}

fn start_audio_thread(
    shared: Arc<Mutex<RecorderShared>>,
    storage: Arc<Mutex<Storage>>,
    stream_config: cpal::StreamConfig,
    sample_format: SampleFormat,
) -> Result<mpsc::Sender<AudioThreadCommand>, String> {
    let (control_tx, control_rx) = mpsc::channel::<AudioThreadCommand>();
    let (init_tx, init_rx) = mpsc::channel::<Result<(), String>>();

    thread::spawn(move || {
        let host = cpal::default_host();
        let device = match host.default_input_device() {
            Some(device) => device,
            None => {
                let _ = init_tx.send(Err("no input device available".to_string()));
                return;
            }
        };

        let shared_for_data = Arc::clone(&shared);
        let storage_for_data = Arc::clone(&storage);
        let shared_for_error = Arc::clone(&shared);

        let error_callback = move |error: cpal::StreamError| {
            if let Ok(mut shared_state) = shared_for_error.lock() {
                shared_state.last_error = Some(format!("audio stream error: {error}"));
            }
        };

        let stream_result = match sample_format {
            SampleFormat::F32 => device.build_input_stream(
                &stream_config,
                move |data: &[f32], _| {
                    process_f32_samples(data, &shared_for_data, &storage_for_data)
                },
                error_callback,
                None,
            ),
            SampleFormat::I16 => {
                let shared_for_data = Arc::clone(&shared);
                let storage_for_data = Arc::clone(&storage);
                let shared_for_error = Arc::clone(&shared);
                let error_callback = move |error: cpal::StreamError| {
                    if let Ok(mut shared_state) = shared_for_error.lock() {
                        shared_state.last_error = Some(format!("audio stream error: {error}"));
                    }
                };

                device.build_input_stream(
                    &stream_config,
                    move |data: &[i16], _| {
                        process_i16_samples(data, &shared_for_data, &storage_for_data)
                    },
                    error_callback,
                    None,
                )
            }
            SampleFormat::U16 => {
                let shared_for_data = Arc::clone(&shared);
                let storage_for_data = Arc::clone(&storage);
                let shared_for_error = Arc::clone(&shared);
                let error_callback = move |error: cpal::StreamError| {
                    if let Ok(mut shared_state) = shared_for_error.lock() {
                        shared_state.last_error = Some(format!("audio stream error: {error}"));
                    }
                };

                device.build_input_stream(
                    &stream_config,
                    move |data: &[u16], _| {
                        process_u16_samples(data, &shared_for_data, &storage_for_data)
                    },
                    error_callback,
                    None,
                )
            }
            other => {
                let _ = init_tx.send(Err(format!("unsupported sample format: {other:?}")));
                return;
            }
        };

        let stream = match stream_result {
            Ok(stream) => stream,
            Err(error) => {
                let _ = init_tx.send(Err(format!("failed to build input stream: {error}")));
                return;
            }
        };

        if let Err(error) = stream.play() {
            let _ = init_tx.send(Err(format!("failed to start audio stream: {error}")));
            return;
        }

        let _ = init_tx.send(Ok(()));

        loop {
            match control_rx.recv() {
                Ok(AudioThreadCommand::Pause(reply)) => {
                    let result = stream
                        .pause()
                        .map_err(|error| format!("failed to pause stream: {error}"));
                    let _ = reply.send(result);
                }
                Ok(AudioThreadCommand::Resume(reply)) => {
                    let result = stream
                        .play()
                        .map_err(|error| format!("failed to resume stream: {error}"));
                    let _ = reply.send(result);
                }
                Ok(AudioThreadCommand::Stop(reply)) => {
                    let result = stream
                        .pause()
                        .map_err(|error| format!("failed to stop stream: {error}"));
                    let _ = reply.send(result);
                    break;
                }
                Err(_) => break,
            }
        }
    });

    match init_rx.recv_timeout(COMMAND_TIMEOUT) {
        Ok(result) => result.map(|_| control_tx),
        Err(_) => Err("timed out while starting audio stream".to_string()),
    }
}

#[tauri::command]
pub fn recorder_start(
    input_device_id: Option<String>,
    quality_preset: Option<RecordingQualityPreset>,
    state: State<'_, AppState>,
) -> Result<StartSessionResponse, String> {
    let mut runtime = recorder_runtime()
        .lock()
        .map_err(|_| "failed to acquire recorder runtime lock".to_string())?;

    if matches!(*runtime, RecorderRuntime::Active(_)) {
        return Err("another recording session is already active".to_string());
    }

    let target_quality = quality_preset.unwrap_or_default();
    let target_config = quality_config(&target_quality);

    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or_else(|| "no input device available".to_string())?;

    let (stream_config, sample_format) =
        select_stream_config(&device, target_config.sample_rate, target_config.channels)?;

    let session_id = Uuid::new_v4().to_string();
    let now = now_iso();
    let actual_sample_rate = stream_config.sample_rate.0;
    let actual_channels = stream_config.channels;

    {
        let mut storage = state
            .storage
            .lock()
            .map_err(|_| "failed to acquire storage lock".to_string())?;

        storage.data.sessions.insert(
            session_id.clone(),
            Session {
                id: session_id.clone(),
                name: None,
                status: SessionStatus::Recording,
                created_at: now.clone(),
                updated_at: now,
                input_device_id,
                audio_segments: vec![],
                audio_segment_meta: vec![],
                quality_preset: target_quality.clone(),
                sample_rate: actual_sample_rate,
                channels: actual_channels,
                elapsed_ms: 0,
                exported_wav_path: None,
                exported_mp3_path: None,
                transcript: vec![],
                summary: None,
            },
        );
        storage.save()?;
    }

    let chunk_frames = SEGMENT_DURATION_SECS.saturating_mul(u64::from(actual_sample_rate));
    let shared = Arc::new(Mutex::new(RecorderShared {
        session_id: session_id.clone(),
        sample_rate: actual_sample_rate,
        channels: actual_channels,
        chunk_frames,
        total_frames: 0,
        current_segment_frames: 0,
        next_sequence: 0,
        writer: None,
        open_segment: None,
        last_rms: 0.0,
        last_peak: 0.0,
        last_error: None,
    }));

    {
        let mut shared_state = shared
            .lock()
            .map_err(|_| "failed to acquire recorder state lock".to_string())?;
        let storage = state
            .storage
            .lock()
            .map_err(|_| "failed to acquire storage lock".to_string())?;
        open_next_segment(&mut shared_state, &storage)?;
    }

    let control_tx = start_audio_thread(
        Arc::clone(&shared),
        Arc::clone(&state.storage),
        stream_config,
        sample_format,
    )?;

    *runtime = RecorderRuntime::Active(ActiveRecorder {
        session_id: session_id.clone(),
        control_tx,
        shared,
    });

    Ok(StartSessionResponse { session_id })
}

#[tauri::command]
pub fn recorder_pause(session_id: String, state: State<'_, AppState>) -> Result<(), String> {
    let (control_tx, shared_ref) = {
        let runtime = recorder_runtime()
            .lock()
            .map_err(|_| "failed to acquire recorder runtime lock".to_string())?;

        match &*runtime {
            RecorderRuntime::Active(active) if active.session_id == session_id => {
                (active.control_tx.clone(), Arc::clone(&active.shared))
            }
            RecorderRuntime::Active(_) => {
                return Err("another session is currently recording".to_string());
            }
            RecorderRuntime::Idle => return Err("no active recorder".to_string()),
        }
    };

    let (reply_tx, reply_rx) = mpsc::channel();
    send_control_command(&control_tx, AudioThreadCommand::Pause(reply_tx))?;
    reply_rx
        .recv_timeout(COMMAND_TIMEOUT)
        .map_err(|_| "pause command timed out".to_string())??;

    let elapsed_ms = {
        let mut shared = shared_ref
            .lock()
            .map_err(|_| "failed to acquire recorder state lock".to_string())?;
        close_open_segment(&mut shared, &state.storage)?;
        elapsed_ms(&shared)
    };

    update_session_status(
        &state.storage,
        &session_id,
        SessionStatus::Paused,
        elapsed_ms,
    )?;
    Ok(())
}

#[tauri::command]
pub fn recorder_resume(session_id: String, state: State<'_, AppState>) -> Result<(), String> {
    let (control_tx, shared_ref) = {
        let runtime = recorder_runtime()
            .lock()
            .map_err(|_| "failed to acquire recorder runtime lock".to_string())?;

        match &*runtime {
            RecorderRuntime::Active(active) if active.session_id == session_id => {
                (active.control_tx.clone(), Arc::clone(&active.shared))
            }
            RecorderRuntime::Active(_) => {
                return Err("another session is currently recording".to_string());
            }
            RecorderRuntime::Idle => return Err("no active recorder".to_string()),
        }
    };

    let elapsed_ms = {
        let mut shared = shared_ref
            .lock()
            .map_err(|_| "failed to acquire recorder state lock".to_string())?;
        let storage = state
            .storage
            .lock()
            .map_err(|_| "failed to acquire storage lock".to_string())?;
        open_next_segment(&mut shared, &storage)?;
        elapsed_ms(&shared)
    };

    let (reply_tx, reply_rx) = mpsc::channel();
    send_control_command(&control_tx, AudioThreadCommand::Resume(reply_tx))?;
    reply_rx
        .recv_timeout(COMMAND_TIMEOUT)
        .map_err(|_| "resume command timed out".to_string())??;

    update_session_status(
        &state.storage,
        &session_id,
        SessionStatus::Recording,
        elapsed_ms,
    )?;
    Ok(())
}

#[tauri::command]
pub fn recorder_stop(session_id: String, state: State<'_, AppState>) -> Result<(), String> {
    let active = {
        let mut runtime = recorder_runtime()
            .lock()
            .map_err(|_| "failed to acquire recorder runtime lock".to_string())?;
        match std::mem::replace(&mut *runtime, RecorderRuntime::Idle) {
            RecorderRuntime::Active(active) if active.session_id == session_id => active,
            RecorderRuntime::Active(active) => {
                *runtime = RecorderRuntime::Active(active);
                return Err("another session is currently recording".to_string());
            }
            RecorderRuntime::Idle => return Err("no active recorder".to_string()),
        }
    };

    let (reply_tx, reply_rx) = mpsc::channel();
    send_control_command(&active.control_tx, AudioThreadCommand::Stop(reply_tx))?;
    reply_rx
        .recv_timeout(COMMAND_TIMEOUT)
        .map_err(|_| "stop command timed out".to_string())??;

    let elapsed_ms = {
        let mut shared = active
            .shared
            .lock()
            .map_err(|_| "failed to acquire recorder state lock".to_string())?;
        close_open_segment(&mut shared, &state.storage)?;
        elapsed_ms(&shared)
    };

    update_session_status(
        &state.storage,
        &session_id,
        SessionStatus::Stopped,
        elapsed_ms,
    )?;
    Ok(())
}

#[tauri::command]
pub fn recorder_status(
    session_id: String,
    state: State<'_, AppState>,
) -> Result<RecorderStatus, String> {
    let runtime = recorder_runtime()
        .lock()
        .map_err(|_| "failed to acquire recorder runtime lock".to_string())?;

    let (elapsed_ms_runtime, rms, peak, pending_segment) = match &*runtime {
        RecorderRuntime::Active(active) if active.session_id == session_id => {
            let shared = active
                .shared
                .lock()
                .map_err(|_| "failed to acquire recorder state lock".to_string())?;
            if let Some(error) = &shared.last_error {
                return Err(error.clone());
            }
            (
                elapsed_ms(&shared),
                shared.last_rms,
                shared.last_peak,
                usize::from(shared.open_segment.is_some()),
            )
        }
        _ => (0, 0.0, 0.0, 0),
    };

    let storage = state
        .storage
        .lock()
        .map_err(|_| "failed to acquire storage lock".to_string())?;
    let session = storage
        .data
        .sessions
        .get(&session_id)
        .ok_or_else(|| "session not found".to_string())?;

    Ok(RecorderStatus {
        session_id,
        elapsed_ms: elapsed_ms_runtime.max(session.elapsed_ms),
        segment_count: session.audio_segments.len() + pending_segment,
        quality_preset: session.quality_preset.clone(),
        rms,
        peak,
    })
}

#[tauri::command]
pub fn recorder_export(
    session_id: String,
    format: String,
    state: State<'_, AppState>,
) -> Result<RecorderExportResponse, String> {
    let format = format.trim().to_lowercase();
    if format != "wav" && format != "mp3" {
        return Err("unsupported export format; expected wav|mp3".to_string());
    }

    let (segments, export_dir) = {
        let storage = state
            .storage
            .lock()
            .map_err(|_| "failed to acquire storage lock".to_string())?;
        let session = storage
            .data
            .sessions
            .get(&session_id)
            .ok_or_else(|| "session not found".to_string())?;
        if session.audio_segments.is_empty() {
            return Err("no audio segments available to export".to_string());
        }
        (
            session.audio_segments.clone(),
            storage.session_export_dir(&session_id)?,
        )
    };

    let base_name = format!("recording-{session_id}");
    let wav_path = export_dir.join(format!("{base_name}.wav"));

    let output_path = if format == "wav" {
        merge_segments_to_wav(&segments, &wav_path)?;
        wav_path.clone()
    } else {
        merge_segments_to_wav(&segments, &wav_path)?;
        let mp3_path = export_dir.join(format!("{base_name}.mp3"));
        encode_mp3_with_ffmpeg(&wav_path, &mp3_path)?;
        mp3_path
    };

    {
        let mut storage = state
            .storage
            .lock()
            .map_err(|_| "failed to acquire storage lock".to_string())?;
        let session = storage
            .data
            .sessions
            .get_mut(&session_id)
            .ok_or_else(|| "session not found".to_string())?;
        if format == "wav" {
            session.exported_wav_path = Some(output_path.to_string_lossy().to_string());
        } else {
            session.exported_wav_path = Some(wav_path.to_string_lossy().to_string());
            session.exported_mp3_path = Some(output_path.to_string_lossy().to_string());
        }
        session.updated_at = now_iso();
        storage.save()?;
    }

    Ok(RecorderExportResponse {
        path: output_path.to_string_lossy().to_string(),
    })
}

fn merge_segments_to_wav(segment_paths: &[String], output_path: &Path) -> Result<(), String> {
    if segment_paths.is_empty() {
        return Err("segment list is empty".to_string());
    }

    let first_reader = WavReader::open(&segment_paths[0])
        .map_err(|error| format!("failed to read wav segment {}: {error}", segment_paths[0]))?;
    let spec = first_reader.spec();
    drop(first_reader);

    let mut writer = WavWriter::create(output_path, spec).map_err(|error| {
        format!(
            "failed to create export wav {}: {error}",
            output_path.display()
        )
    })?;

    for segment in segment_paths {
        let mut reader = WavReader::open(segment)
            .map_err(|error| format!("failed to read segment {segment}: {error}"))?;

        if reader.spec() != spec {
            return Err(format!(
                "segment format mismatch in {segment}; cannot merge wav files"
            ));
        }

        for sample in reader.samples::<i16>() {
            let value = sample.map_err(|error| {
                format!("failed to decode pcm sample for segment {segment}: {error}")
            })?;
            writer
                .write_sample(value)
                .map_err(|error| format!("failed to write export sample: {error}"))?;
        }
    }

    writer
        .finalize()
        .map_err(|error| format!("failed to finalize export wav: {error}"))?;

    Ok(())
}

fn encode_mp3_with_ffmpeg(wav_path: &Path, mp3_path: &Path) -> Result<(), String> {
    let status = Command::new("ffmpeg")
        .args([
            "-y",
            "-i",
            wav_path.to_string_lossy().as_ref(),
            "-codec:a",
            "libmp3lame",
            "-q:a",
            "3",
            mp3_path.to_string_lossy().as_ref(),
        ])
        .status();

    match status {
        Ok(result) if result.success() => Ok(()),
        Ok(result) => Err(format!("ffmpeg exited with status: {result}")),
        Err(error) => Err(format!(
            "failed to run ffmpeg for mp3 export; ensure ffmpeg is installed: {error}"
        )),
    }
}
