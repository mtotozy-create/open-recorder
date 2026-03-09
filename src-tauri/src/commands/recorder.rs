use std::{
    collections::HashMap,
    fs,
    io::BufWriter,
    io::ErrorKind,
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
use hound::{SampleFormat as WavSampleFormat, WavSpec, WavWriter};
use tauri::State;
use uuid::Uuid;

use crate::{
    models::{
        merge_session_tags_into_catalog, AudioSegmentMeta, RecorderExportResponse, RecorderPhase,
        RecorderProcessingStatus, RecorderStatus, RecordingQualityPreset, Session, SessionStatus,
        StartSessionResponse, DEFAULT_RECORDING_SESSION_TAG,
    },
    state::AppState,
    storage::Storage,
};

const COMMAND_TIMEOUT: Duration = Duration::from_secs(5);

struct QualityConfig {
    capture_sample_rate: u32,
    capture_channels: u16,
    output_sample_rate: u32,
    output_channels: u16,
    output_bitrate: &'static str,
}

struct OpenSegment {
    path: String,
    sequence: u32,
    started_at: String,
    start_elapsed_ms: u64,
}

struct RecorderShared {
    session_id: String,
    segment_dir: PathBuf,
    quality_preset: RecordingQualityPreset,
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

struct PendingSegmentTask {
    session_id: String,
    path: String,
    sequence: u32,
    started_at: String,
    start_elapsed_ms: u64,
    finished_elapsed_ms: u64,
    sample_rate: u32,
    channels: u16,
    quality_preset: RecordingQualityPreset,
}

struct ProcessingState {
    pending_jobs: usize,
    finalizing: bool,
    last_error: Option<String>,
}

enum SegmentProcessorCommand {
    Process(PendingSegmentTask),
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

fn processing_registry() -> &'static Mutex<HashMap<String, ProcessingState>> {
    static REGISTRY: OnceLock<Mutex<HashMap<String, ProcessingState>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

fn segment_processor_tx(
    storage: Arc<Mutex<Storage>>,
) -> &'static mpsc::Sender<SegmentProcessorCommand> {
    static TX: OnceLock<mpsc::Sender<SegmentProcessorCommand>> = OnceLock::new();
    TX.get_or_init(|| {
        let (tx, rx) = mpsc::channel::<SegmentProcessorCommand>();
        thread::spawn(move || {
            while let Ok(command) = rx.recv() {
                match command {
                    SegmentProcessorCommand::Process(task) => {
                        process_segment_task(task, &storage);
                    }
                }
            }
        });
        tx
    })
}

fn now_iso() -> String {
    Utc::now().to_rfc3339()
}

fn quality_config(preset: &RecordingQualityPreset) -> QualityConfig {
    match preset {
        RecordingQualityPreset::VoiceLowStorage => QualityConfig {
            capture_sample_rate: 16_000,
            capture_channels: 1,
            output_sample_rate: 16_000,
            output_channels: 1,
            output_bitrate: "24k",
        },
        RecordingQualityPreset::LegacyCompatible => QualityConfig {
            capture_sample_rate: 48_000,
            capture_channels: 1,
            output_sample_rate: 48_000,
            output_channels: 1,
            output_bitrate: "64k",
        },
        RecordingQualityPreset::Standard => QualityConfig {
            capture_sample_rate: 16_000,
            capture_channels: 1,
            output_sample_rate: 16_000,
            output_channels: 1,
            output_bitrate: "40k",
        },
        RecordingQualityPreset::Hd => QualityConfig {
            capture_sample_rate: 24_000,
            capture_channels: 1,
            output_sample_rate: 24_000,
            output_channels: 1,
            output_bitrate: "64k",
        },
        RecordingQualityPreset::Hifi => QualityConfig {
            capture_sample_rate: 48_000,
            capture_channels: 2,
            output_sample_rate: 48_000,
            output_channels: 2,
            output_bitrate: "128k",
        },
    }
}

fn ffmpeg_candidates() -> [&'static str; 3] {
    [
        "ffmpeg",
        "/opt/homebrew/bin/ffmpeg",
        "/usr/local/bin/ffmpeg",
    ]
}

fn run_ffmpeg(args: &[&str]) -> Result<std::process::ExitStatus, String> {
    let mut not_found_bins: Vec<String> = vec![];
    for bin in ffmpeg_candidates() {
        match Command::new(bin)
            .args(args)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
        {
            Ok(status) => return Ok(status),
            Err(error) if error.kind() == ErrorKind::NotFound => {
                not_found_bins.push(bin.to_string());
            }
            Err(error) => return Err(format!("failed to run {bin}: {error}")),
        }
    }

    Err(format!(
        "ffmpeg executable not found; tried {}",
        not_found_bins.join(", ")
    ))
}

/// 使用 macOS 原生 afconvert 或 FFmpeg 将 WAV 分段转码为 M4A (AAC)
/// 优先使用 afconvert（macOS 自带），不可用时回退到 ffmpeg
fn encode_wav_to_m4a(
    wav_path: &Path,
    output_sample_rate: u32,
    output_channels: u16,
    bitrate: &str,
) -> Result<PathBuf, String> {
    let m4a_path = wav_path.with_extension("m4a");
    let wav_str = wav_path.to_string_lossy();
    let m4a_str = m4a_path.to_string_lossy();
    let output_channels = output_channels.max(1);
    let output_sample_rate = output_sample_rate.max(8_000);
    let output_format = format!("aac@{output_sample_rate}");

    // 将 bitrate 从 "64k" 格式转为 afconvert 需要的纯数字格式（单位 bps）
    let bitrate_bps = bitrate.trim_end_matches('k').parse::<u32>().unwrap_or(64) * 1000;
    let output_channels_str = output_channels.to_string();

    // 优先尝试 afconvert（macOS 自带）
    let afconvert_result = Command::new("afconvert")
        .args([
            wav_str.as_ref(),
            m4a_str.as_ref(),
            "-d",
            output_format.as_ref(),
            "-f",
            "m4af",
            "-c",
            output_channels_str.as_ref(),
            "-b",
            &bitrate_bps.to_string(),
            "-s",
            "0",
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();

    let success = match afconvert_result {
        Ok(status) if status.success() => true,
        _ => {
            // afconvert 不可用或失败，回退到 ffmpeg
            let output_sample_rate_str = output_sample_rate.to_string();
            let ffmpeg_result = run_ffmpeg(&[
                "-y",
                "-i",
                wav_str.as_ref(),
                "-ar",
                output_sample_rate_str.as_str(),
                "-ac",
                output_channels_str.as_ref(),
                "-codec:a",
                "aac",
                "-b:a",
                bitrate,
                m4a_str.as_ref(),
            ]);

            match ffmpeg_result {
                Ok(status) if status.success() => true,
                Ok(status) => {
                    return Err(format!("ffmpeg m4a encode exited with status: {status}"));
                }
                Err(error) => {
                    return Err(format!(
                        "both afconvert and ffmpeg unavailable for m4a encode: {error}"
                    ));
                }
            }
        }
    };

    if success {
        // 转码成功，删除原始 WAV
        let _ = fs::remove_file(wav_path);
        Ok(m4a_path)
    } else {
        Err("m4a encode failed".to_string())
    }
}

fn processing_snapshot(session_id: &str) -> (usize, bool, Option<String>) {
    let registry = match processing_registry().lock() {
        Ok(value) => value,
        Err(_) => {
            return (
                0,
                false,
                Some("failed to acquire processing lock".to_string()),
            )
        }
    };

    if let Some(state) = registry.get(session_id) {
        return (
            state.pending_jobs,
            state.finalizing,
            state.last_error.clone(),
        );
    }
    (0, false, None)
}

fn increase_pending_jobs(session_id: &str) -> Result<(), String> {
    let mut registry = processing_registry()
        .lock()
        .map_err(|_| "failed to acquire processing lock".to_string())?;
    let state = registry
        .entry(session_id.to_string())
        .or_insert(ProcessingState {
            pending_jobs: 0,
            finalizing: false,
            last_error: None,
        });
    state.pending_jobs = state.pending_jobs.saturating_add(1);
    Ok(())
}

fn mark_finalizing(session_id: &str) -> Result<(usize, Option<String>), String> {
    let mut registry = processing_registry()
        .lock()
        .map_err(|_| "failed to acquire processing lock".to_string())?;
    let state = registry
        .entry(session_id.to_string())
        .or_insert(ProcessingState {
            pending_jobs: 0,
            finalizing: false,
            last_error: None,
        });
    state.finalizing = true;
    Ok((state.pending_jobs, state.last_error.clone()))
}

fn complete_pending_job(
    session_id: &str,
    task_error: Option<String>,
) -> Result<(usize, bool, Option<String>), String> {
    let mut registry = processing_registry()
        .lock()
        .map_err(|_| "failed to acquire processing lock".to_string())?;
    let state = registry
        .entry(session_id.to_string())
        .or_insert(ProcessingState {
            pending_jobs: 0,
            finalizing: false,
            last_error: None,
        });

    if state.pending_jobs > 0 {
        state.pending_jobs -= 1;
    }
    if task_error.is_some() {
        state.last_error = task_error.clone();
    }

    let should_remove = state.pending_jobs == 0 && !state.finalizing;
    let snapshot = (
        state.pending_jobs,
        state.finalizing,
        state.last_error.clone(),
    );
    if should_remove {
        registry.remove(session_id);
    }
    Ok(snapshot)
}

fn clear_processing_state(session_id: &str) {
    if let Ok(mut registry) = processing_registry().lock() {
        registry.remove(session_id);
    }
}

fn has_active_finalizing_session() -> bool {
    let registry = match processing_registry().lock() {
        Ok(value) => value,
        Err(_) => return false,
    };
    registry
        .values()
        .any(|state| state.finalizing && state.pending_jobs > 0)
}

fn persist_segment_meta(
    task: &PendingSegmentTask,
    storage: &Arc<Mutex<Storage>>,
) -> Result<(), String> {
    let quality = quality_config(&task.quality_preset);
    let wav_path = Path::new(&task.path);
    let (final_path, final_format, final_sample_rate, final_channels) = match encode_wav_to_m4a(
        wav_path,
        quality.output_sample_rate,
        quality.output_channels,
        quality.output_bitrate,
    ) {
        Ok(m4a_path) => (
            m4a_path.to_string_lossy().to_string(),
            "m4a".to_string(),
            quality.output_sample_rate,
            quality.output_channels,
        ),
        Err(_) => (
            task.path.clone(),
            "wav".to_string(),
            task.sample_rate,
            task.channels,
        ),
    };
    let duration_ms = task
        .finished_elapsed_ms
        .saturating_sub(task.start_elapsed_ms);
    let file_size_bytes = fs::metadata(&final_path).map(|m| m.len()).unwrap_or(0);

    let meta = AudioSegmentMeta {
        path: final_path.clone(),
        sequence: task.sequence,
        started_at: task.started_at.clone(),
        ended_at: now_iso(),
        duration_ms,
        sample_rate: final_sample_rate,
        channels: final_channels,
        format: final_format,
        file_size_bytes,
    };

    let mut state = storage
        .lock()
        .map_err(|_| "failed to acquire storage lock".to_string())?;
    let session = state
        .data
        .sessions
        .get_mut(&task.session_id)
        .ok_or_else(|| "session not found".to_string())?;
    session.audio_segments.push(meta.path.clone());
    session.audio_segment_meta.push(meta);
    session.elapsed_ms = task.finished_elapsed_ms;
    session.sample_rate = final_sample_rate;
    session.channels = final_channels;
    session.updated_at = now_iso();
    state.save()?;
    Ok(())
}

fn update_finalizing_session_status(
    session_id: &str,
    status: SessionStatus,
    error: Option<String>,
    storage: &Arc<Mutex<Storage>>,
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
    session.updated_at = now_iso();
    if let Some(reason) = error {
        eprintln!(
            "[recorder] processing failed for session {}: {}",
            session_id, reason
        );
    }
    state.save()?;
    Ok(())
}

fn process_segment_task(task: PendingSegmentTask, storage: &Arc<Mutex<Storage>>) {
    let task_result = persist_segment_meta(&task, storage);
    let task_error = task_result.err();

    let (pending_jobs, finalizing, last_error) =
        match complete_pending_job(&task.session_id, task_error.clone()) {
            Ok(snapshot) => snapshot,
            Err(error) => {
                eprintln!(
                    "[recorder] failed to update processing state for {}: {}",
                    task.session_id, error
                );
                return;
            }
        };

    if finalizing && pending_jobs == 0 {
        let status = if last_error.is_some() {
            SessionStatus::Failed
        } else {
            SessionStatus::Stopped
        };
        if let Err(error) =
            update_finalizing_session_status(&task.session_id, status, last_error.clone(), storage)
        {
            eprintln!(
                "[recorder] failed to update session status for {}: {}",
                task.session_id, error
            );
        }
        clear_processing_state(&task.session_id);
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

fn segment_file_path(segment_dir: &Path, sequence: u32) -> PathBuf {
    let timestamp = Utc::now().format("%Y%m%d-%H%M%S");
    segment_dir.join(format!("segment-{timestamp}-{sequence:06}.wav"))
}

fn open_next_segment(shared: &mut RecorderShared) -> Result<(), String> {
    if shared.writer.is_some() {
        return Ok(());
    }

    let sequence = shared.next_sequence;
    let segment_path = segment_file_path(&shared.segment_dir, sequence);
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
    processor_tx: &mpsc::Sender<SegmentProcessorCommand>,
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
    let task = PendingSegmentTask {
        session_id: shared.session_id.clone(),
        path: open_segment.path,
        sequence: open_segment.sequence,
        started_at: open_segment.started_at,
        start_elapsed_ms: open_segment.start_elapsed_ms,
        finished_elapsed_ms,
        sample_rate: shared.sample_rate,
        channels: shared.channels,
        quality_preset: shared.quality_preset.clone(),
    };
    increase_pending_jobs(&task.session_id)?;
    if processor_tx
        .send(SegmentProcessorCommand::Process(task))
        .is_err()
    {
        let _ = complete_pending_job(
            &shared.session_id,
            Some("failed to enqueue segment processing task".to_string()),
        );
        return Err("failed to enqueue segment processing task".to_string());
    }

    Ok(())
}

fn rotate_segment_if_needed(
    shared: &mut RecorderShared,
    storage: &Arc<Mutex<Storage>>,
    processor_tx: &mpsc::Sender<SegmentProcessorCommand>,
) {
    if shared.current_segment_frames < shared.chunk_frames {
        return;
    }

    if let Err(error) = close_open_segment(shared, processor_tx) {
        shared.last_error = Some(error);
        return;
    }

    if let Err(error) = open_next_segment(shared) {
        shared.last_error = Some(error);
        return;
    }

    if let Ok(mut storage_lock) = storage.lock() {
        if let Some(session) = storage_lock.data.sessions.get_mut(&shared.session_id) {
            session.updated_at = now_iso();
            let _ = storage_lock.save();
        }
    }
}

fn process_i16_samples(
    data: &[i16],
    shared: &Arc<Mutex<RecorderShared>>,
    storage: &Arc<Mutex<Storage>>,
    processor_tx: &mpsc::Sender<SegmentProcessorCommand>,
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

    rotate_segment_if_needed(&mut state, storage, processor_tx);
}

fn process_f32_samples(
    data: &[f32],
    shared: &Arc<Mutex<RecorderShared>>,
    storage: &Arc<Mutex<Storage>>,
    processor_tx: &mpsc::Sender<SegmentProcessorCommand>,
) {
    let converted: Vec<i16> = data
        .iter()
        .map(|sample| {
            let clamped = sample.clamp(-1.0, 1.0);
            (clamped * i16::MAX as f32) as i16
        })
        .collect();
    process_i16_samples(&converted, shared, storage, processor_tx);
}

fn process_u16_samples(
    data: &[u16],
    shared: &Arc<Mutex<RecorderShared>>,
    storage: &Arc<Mutex<Storage>>,
    processor_tx: &mpsc::Sender<SegmentProcessorCommand>,
) {
    let converted: Vec<i16> = data
        .iter()
        .map(|sample| ((*sample as i32) - 32768) as i16)
        .collect();
    process_i16_samples(&converted, shared, storage, processor_tx);
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
    processor_tx: mpsc::Sender<SegmentProcessorCommand>,
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
        let processor_tx_for_data = processor_tx.clone();
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
                    process_f32_samples(
                        data,
                        &shared_for_data,
                        &storage_for_data,
                        &processor_tx_for_data,
                    )
                },
                error_callback,
                None,
            ),
            SampleFormat::I16 => {
                let shared_for_data = Arc::clone(&shared);
                let storage_for_data = Arc::clone(&storage);
                let processor_tx_for_data = processor_tx.clone();
                let shared_for_error = Arc::clone(&shared);
                let error_callback = move |error: cpal::StreamError| {
                    if let Ok(mut shared_state) = shared_for_error.lock() {
                        shared_state.last_error = Some(format!("audio stream error: {error}"));
                    }
                };

                device.build_input_stream(
                    &stream_config,
                    move |data: &[i16], _| {
                        process_i16_samples(
                            data,
                            &shared_for_data,
                            &storage_for_data,
                            &processor_tx_for_data,
                        )
                    },
                    error_callback,
                    None,
                )
            }
            SampleFormat::U16 => {
                let shared_for_data = Arc::clone(&shared);
                let storage_for_data = Arc::clone(&storage);
                let processor_tx_for_data = processor_tx.clone();
                let shared_for_error = Arc::clone(&shared);
                let error_callback = move |error: cpal::StreamError| {
                    if let Ok(mut shared_state) = shared_for_error.lock() {
                        shared_state.last_error = Some(format!("audio stream error: {error}"));
                    }
                };

                device.build_input_stream(
                    &stream_config,
                    move |data: &[u16], _| {
                        process_u16_samples(
                            data,
                            &shared_for_data,
                            &storage_for_data,
                            &processor_tx_for_data,
                        )
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
    if has_active_finalizing_session() {
        return Err("previous session is still processing; try again shortly".to_string());
    }

    let target_quality = quality_preset.unwrap_or_default();
    let target_config = quality_config(&target_quality);

    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or_else(|| "no input device available".to_string())?;

    let (stream_config, sample_format) = select_stream_config(
        &device,
        target_config.capture_sample_rate,
        target_config.capture_channels,
    )?;

    let session_id = Uuid::new_v4().to_string();
    let now = now_iso();
    let actual_sample_rate = stream_config.sample_rate.0;
    let actual_channels = stream_config.channels;
    let (segment_dir, segment_duration_secs) = {
        let mut storage = state
            .storage
            .lock()
            .map_err(|_| "failed to acquire storage lock".to_string())?;
        storage.data.settings.normalize();
        (
            storage.session_audio_dir(&session_id)?,
            storage.data.settings.recording_segment_seconds,
        )
    };

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
                tags: vec![DEFAULT_RECORDING_SESSION_TAG.to_string()],
                exported_wav_path: None,
                exported_wav_size: None,
                exported_wav_created_at: None,
                exported_mp3_path: None,
                exported_mp3_size: None,
                exported_mp3_created_at: None,
                exported_m4a_path: None,
                exported_m4a_size: None,
                exported_m4a_created_at: None,
                transcript: vec![],
                summary: None,
            },
        );
        merge_session_tags_into_catalog(
            &mut storage.data.settings.session_tag_catalog,
            &[DEFAULT_RECORDING_SESSION_TAG.to_string()],
        );
        storage.data.settings.normalize();
        storage.save()?;
    }
    clear_processing_state(&session_id);

    let chunk_frames = segment_duration_secs.saturating_mul(u64::from(actual_sample_rate));
    let shared = Arc::new(Mutex::new(RecorderShared {
        session_id: session_id.clone(),
        segment_dir,
        quality_preset: target_quality.clone(),
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
        open_next_segment(&mut shared_state)?;
    }

    let processor_tx = segment_processor_tx(Arc::clone(&state.storage)).clone();
    let control_tx = start_audio_thread(
        Arc::clone(&shared),
        Arc::clone(&state.storage),
        processor_tx,
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

    let processor_tx = segment_processor_tx(Arc::clone(&state.storage)).clone();
    let elapsed_ms = {
        let mut shared = shared_ref
            .lock()
            .map_err(|_| "failed to acquire recorder state lock".to_string())?;
        close_open_segment(&mut shared, &processor_tx)?;
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
        open_next_segment(&mut shared)?;
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

    let processor_tx = segment_processor_tx(Arc::clone(&state.storage)).clone();
    let elapsed_ms = {
        let mut shared = active
            .shared
            .lock()
            .map_err(|_| "failed to acquire recorder state lock".to_string())?;
        close_open_segment(&mut shared, &processor_tx)?;
        elapsed_ms(&shared)
    };

    let (pending_jobs, last_error) = mark_finalizing(&session_id)?;
    let status = if pending_jobs == 0 {
        if last_error.is_some() {
            SessionStatus::Failed
        } else {
            SessionStatus::Stopped
        }
    } else {
        SessionStatus::Processing
    };

    update_session_status(&state.storage, &session_id, status, elapsed_ms)?;
    if pending_jobs == 0 {
        clear_processing_state(&session_id);
    }
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

    let (elapsed_ms_runtime, rms, peak, pending_segment, runtime_phase, runtime_error) =
        match &*runtime {
            RecorderRuntime::Active(active) if active.session_id == session_id => {
                let shared = active
                    .shared
                    .lock()
                    .map_err(|_| "failed to acquire recorder state lock".to_string())?;
                let runtime_phase = if matches!(shared.last_error, Some(_)) {
                    RecorderPhase::Error
                } else {
                    RecorderPhase::Recording
                };
                (
                    elapsed_ms(&shared),
                    shared.last_rms,
                    shared.last_peak,
                    usize::from(shared.open_segment.is_some()),
                    runtime_phase,
                    shared.last_error.clone(),
                )
            }
            _ => (0, 0.0, 0.0, 0, RecorderPhase::Idle, None),
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
    let (pending_jobs, finalizing, last_processing_error) = processing_snapshot(&session_id);
    let phase = if matches!(runtime_phase, RecorderPhase::Error) {
        RecorderPhase::Error
    } else if matches!(runtime_phase, RecorderPhase::Recording) {
        match session.status {
            SessionStatus::Paused => RecorderPhase::Paused,
            _ => RecorderPhase::Recording,
        }
    } else if finalizing || matches!(session.status, SessionStatus::Processing) {
        RecorderPhase::Processing
    } else if matches!(session.status, SessionStatus::Failed) {
        RecorderPhase::Error
    } else {
        RecorderPhase::Idle
    };
    let pending_jobs_total = pending_jobs + pending_segment;
    let last_processing_error = runtime_error.or(last_processing_error);

    Ok(RecorderStatus {
        session_id,
        elapsed_ms: elapsed_ms_runtime.max(session.elapsed_ms),
        segment_count: session.audio_segments.len() + pending_jobs_total,
        persisted_segment_count: session.audio_segments.len(),
        quality_preset: session.quality_preset.clone(),
        rms,
        peak,
        phase,
        pending_jobs: pending_jobs_total,
        last_processing_error,
    })
}

#[tauri::command]
pub fn recorder_processing_status(
    session_id: String,
    state: State<'_, AppState>,
) -> Result<RecorderProcessingStatus, String> {
    let status = recorder_status(session_id.clone(), state)?;
    Ok(RecorderProcessingStatus {
        session_id,
        phase: status.phase,
        pending_jobs: status.pending_jobs,
        last_processing_error: status.last_processing_error,
    })
}

#[tauri::command]
pub fn recorder_export(
    session_id: String,
    format: String,
    state: State<'_, AppState>,
) -> Result<RecorderExportResponse, String> {
    let format = format.trim().to_lowercase();
    if format != "wav" && format != "mp3" && format != "m4a" {
        return Err("unsupported export format; expected wav|mp3|m4a".to_string());
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
        if matches!(session.status, SessionStatus::Processing) {
            return Err(
                "session is still processing segments; export is temporarily unavailable"
                    .to_string(),
            );
        }
        if session.audio_segments.is_empty() {
            return Err("no audio segments available to export".to_string());
        }
        (
            session.audio_segments.clone(),
            storage.session_export_dir(&session_id)?,
        )
    };

    let base_name = format!("recording-{session_id}");

    let output_path = if format == "wav" {
        let wav_path = export_dir.join(format!("{base_name}.wav"));
        merge_segments_with_ffmpeg(&segments, &wav_path, "wav")?;
        wav_path
    } else if format == "m4a" {
        let m4a_path = export_dir.join(format!("{base_name}.m4a"));
        merge_segments_with_ffmpeg(&segments, &m4a_path, "m4a")?;
        m4a_path
    } else {
        // mp3
        let mp3_path = export_dir.join(format!("{base_name}.mp3"));
        merge_segments_with_ffmpeg(&segments, &mp3_path, "mp3")?;
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
        let file_size_bytes = std::fs::metadata(&output_path)
            .map(|m| m.len())
            .unwrap_or(0);
        match format.as_str() {
            "wav" => {
                session.exported_wav_path = Some(output_path.to_string_lossy().to_string());
                session.exported_wav_size = Some(file_size_bytes);
                session.exported_wav_created_at = Some(now_iso());
            }
            "m4a" => {
                session.exported_m4a_path = Some(output_path.to_string_lossy().to_string());
                session.exported_m4a_size = Some(file_size_bytes);
                session.exported_m4a_created_at = Some(now_iso());
            }
            _ => {
                session.exported_mp3_path = Some(output_path.to_string_lossy().to_string());
                session.exported_mp3_size = Some(file_size_bytes);
                session.exported_mp3_created_at = Some(now_iso());
            }
        }
        session.updated_at = now_iso();
        storage.save()?;
    }

    Ok(RecorderExportResponse {
        path: output_path.to_string_lossy().to_string(),
    })
}

/// 合并多个分段文件并输出为指定格式
/// 单分段：优先 afconvert，回退 ffmpeg
/// 多分段：先将分段统一为 WAV，再合并并转码为目标格式
pub(crate) fn merge_segments_with_ffmpeg(
    segment_paths: &[String],
    output_path: &Path,
    output_format: &str,
) -> Result<(), String> {
    if segment_paths.is_empty() {
        return Err("segment list is empty".to_string());
    }

    // 单分段：直接转码
    if segment_paths.len() == 1 {
        return convert_single_file(&segment_paths[0], output_path, output_format);
    }

    // 多分段：先在系统临时目录进行解码与合并，避免目标目录权限影响临时文件写入。
    let output_dir = output_path
        .parent()
        .ok_or_else(|| "failed to resolve export directory".to_string())?;
    fs::create_dir_all(output_dir)
        .map_err(|error| format!("failed to create export directory: {error}"))?;

    let merge_work_dir = std::env::temp_dir()
        .join("open-recorder-merge")
        .join(Uuid::new_v4().to_string());
    fs::create_dir_all(&merge_work_dir).map_err(|error| {
        format!(
            "failed to create merge temp directory {}: {error}",
            merge_work_dir.display()
        )
    })?;

    let intermediate_wav = merge_work_dir.join("_intermediate_merge.wav");

    let merge_result: Result<(), String> = (|| {
        // 统一为 WAV（按分段逐个判断格式，避免混合格式误判）
        let mut wav_paths: Vec<String> = Vec::with_capacity(segment_paths.len());
        for (index, segment) in segment_paths.iter().enumerate() {
            let ext = Path::new(segment)
                .extension()
                .and_then(|value| value.to_str())
                .map(str::to_ascii_lowercase)
                .unwrap_or_else(|| "wav".to_string());
            if ext == "wav" {
                wav_paths.push(segment.clone());
            } else {
                let temp_wav = merge_work_dir.join(format!("temp-decode-{index}.wav"));
                convert_single_file(segment, &temp_wav, "wav")?;
                wav_paths.push(temp_wav.to_string_lossy().to_string());
            }
        }

        merge_wav_segments_with_hound(&wav_paths, &intermediate_wav)?;

        // 如果目标就是 WAV，直接复制中间文件到目标输出
        if output_format == "wav" {
            fs::copy(&intermediate_wav, output_path)
                .map_err(|error| format!("failed to write merged wav: {error}"))?;
            return Ok(());
        }

        // 转码中间 WAV 为目标格式
        convert_single_file(
            &intermediate_wav.to_string_lossy(),
            output_path,
            output_format,
        )
    })();

    // 无论成功失败都清理临时文件目录
    let _ = fs::remove_file(&intermediate_wav);
    let _ = fs::remove_dir_all(&merge_work_dir);

    merge_result
}

/// 使用 hound 合并多个 WAV 文件
pub(crate) fn merge_wav_segments_with_hound(
    segment_paths: &[String],
    output_path: &Path,
) -> Result<(), String> {
    use hound::{WavReader, WavWriter};

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

/// 单文件格式转换：优先 afconvert，回退 ffmpeg
pub(crate) fn convert_single_file(
    input: &str,
    output_path: &Path,
    output_format: &str,
) -> Result<(), String> {
    let output_str = output_path.to_string_lossy();

    // 优先尝试 afconvert
    let afconvert_args = match output_format {
        "wav" => vec![
            input.to_string(),
            output_str.to_string(),
            "-d".to_string(),
            "LEI16".to_string(),
            "-f".to_string(),
            "WAVE".to_string(),
        ],
        "m4a" => vec![
            input.to_string(),
            output_str.to_string(),
            "-d".to_string(),
            "aac".to_string(),
            "-f".to_string(),
            "m4af".to_string(),
            "-b".to_string(),
            "128000".to_string(),
        ],
        _ => vec![], // afconvert 不支持 MP3
    };

    if !afconvert_args.is_empty() {
        let result = Command::new("afconvert")
            .args(&afconvert_args)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();

        if let Ok(status) = result {
            if status.success() {
                return Ok(());
            }
        }
    }

    // afconvert 不可用或不支持该格式，回退到 ffmpeg
    let mut args = vec!["-y".to_string(), "-i".to_string(), input.to_string()];

    match output_format {
        "wav" => {
            args.extend_from_slice(&["-codec:a".to_string(), "pcm_s16le".to_string()]);
        }
        "m4a" => {
            args.extend_from_slice(&[
                "-codec:a".to_string(),
                "aac".to_string(),
                "-b:a".to_string(),
                "128k".to_string(),
            ]);
        }
        _ => {
            // mp3
            args.extend_from_slice(&[
                "-codec:a".to_string(),
                "libmp3lame".to_string(),
                "-q:a".to_string(),
                "3".to_string(),
            ]);
        }
    }

    args.push(output_str.to_string());

    let ffmpeg_args: Vec<&str> = args.iter().map(String::as_str).collect();
    let status = run_ffmpeg(&ffmpeg_args);

    match status {
        Ok(result) if result.success() => Ok(()),
        Ok(result) => Err(format!("ffmpeg exited with status: {result}")),
        Err(error) => Err(format!(
            "failed to run ffmpeg for export; ensure ffmpeg is installed: {error}"
        )),
    }
}
