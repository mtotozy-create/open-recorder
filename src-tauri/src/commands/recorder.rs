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
        merge_session_tags_into_catalog, AudioSegmentMeta, ProviderKind, RecorderExportResponse,
        RecorderInputDevice, RecorderPhase, RecorderProcessingStatus, RecorderRealtimeState,
        RecorderRealtimeStatus, RecorderStatus, RecordingQualityPreset, Session, SessionStatus,
        StartSessionResponse, DEFAULT_RECORDING_SESSION_TAG,
    },
    providers::aliyun_tingwu_realtime::{
        start_realtime_worker, AliyunTingwuRealtimeConfig, RealtimeWorkerEvent,
        RealtimeWorkerHandle, RealtimeWorkerState,
    },
    state::AppState,
    storage::Storage,
};

const COMMAND_TIMEOUT: Duration = Duration::from_secs(5);
const UNKNOWN_INPUT_DEVICE_NAME: &str = "Unknown Input Device";
const DEFAULT_REALTIME_SOURCE_LANGUAGE: &str = "cn";
const DEFAULT_REALTIME_TRANSLATION_TARGET_LANGUAGE: &str = "en";
const SUPPORTED_REALTIME_SOURCE_LANGUAGES: [&str; 6] =
    ["cn", "en", "yue", "ja", "ko", "multilingual"];
const SUPPORTED_REALTIME_TRANSLATION_TARGET_LANGUAGES: [&str; 7] =
    ["en", "cn", "ja", "ko", "fr", "de", "ru"];

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

#[derive(Default)]
struct RealtimeResampleState {
    phase: u32,
}

#[derive(Clone)]
struct RealtimeSegmentMeta {
    sentence_id: Option<String>,
    sentence_index: Option<u64>,
}

#[derive(Clone)]
struct PendingRealtimeTranslation {
    text: String,
    event_time_ms: Option<u64>,
    source_sentence_id: Option<String>,
    sentence_index: Option<u64>,
    target_language: Option<String>,
}

struct RecorderRealtimeRuntime {
    enabled: bool,
    format: String,
    sample_rate: u32,
    source_language: String,
    translation_enabled: bool,
    translation_target_language: String,
    state: RecorderRealtimeState,
    preview_text: String,
    segments: Vec<crate::models::TranscriptSegment>,
    segment_meta: Vec<RealtimeSegmentMeta>,
    pending_translations: Vec<PendingRealtimeTranslation>,
    next_segment_start_ms: u64,
    last_error: Option<String>,
    event_rx: Option<mpsc::Receiver<RealtimeWorkerEvent>>,
    worker: Option<RealtimeWorkerHandle>,
    resample_state: RealtimeResampleState,
}

impl Default for RecorderRealtimeRuntime {
    fn default() -> Self {
        Self {
            enabled: false,
            format: "pcm".to_string(),
            sample_rate: 16000,
            source_language: DEFAULT_REALTIME_SOURCE_LANGUAGE.to_string(),
            translation_enabled: false,
            translation_target_language: DEFAULT_REALTIME_TRANSLATION_TARGET_LANGUAGE.to_string(),
            state: RecorderRealtimeState::Idle,
            preview_text: String::new(),
            segments: vec![],
            segment_meta: vec![],
            pending_translations: vec![],
            next_segment_start_ms: 0,
            last_error: None,
            event_rx: None,
            worker: None,
            resample_state: RealtimeResampleState::default(),
        }
    }
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
    realtime: RecorderRealtimeRuntime,
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

struct InputDeviceEntry {
    device: cpal::Device,
    id: String,
    name: String,
}

struct ResolvedInputDevice {
    device: cpal::Device,
    id: String,
    name: String,
    fallback_from: Option<String>,
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

fn parse_csv_values(raw: Option<&str>) -> Vec<String> {
    raw.unwrap_or_default()
        .replace('，', ",")
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string())
        .collect()
}

fn parse_language_hints(raw: Option<&str>) -> Vec<String> {
    parse_csv_values(raw)
}

fn normalize_realtime_translation_target_language(raw: &str) -> Option<String> {
    let normalized = raw.trim().to_ascii_lowercase().replace('_', "-");
    if normalized.is_empty() {
        return None;
    }
    let canonical = match normalized.as_str() {
        "zh" | "zh-cn" => "cn".to_string(),
        _ => normalized,
    };
    SUPPORTED_REALTIME_TRANSLATION_TARGET_LANGUAGES
        .contains(&canonical.as_str())
        .then_some(canonical)
}

fn normalize_realtime_source_language(raw: &str) -> Option<String> {
    let normalized = raw.trim().to_ascii_lowercase().replace('_', "-");
    if normalized.is_empty() {
        return None;
    }
    let canonical = match normalized.as_str() {
        "zh" | "zh-cn" => "cn".to_string(),
        _ => normalized,
    };
    SUPPORTED_REALTIME_SOURCE_LANGUAGES
        .contains(&canonical.as_str())
        .then_some(canonical)
}

fn parse_realtime_translation_target_languages(raw: Option<&str>) -> Vec<String> {
    let mut values = Vec::new();
    for item in parse_csv_values(raw) {
        if let Some(normalized) = normalize_realtime_translation_target_language(&item) {
            if !values.contains(&normalized) {
                values.push(normalized);
            }
        }
    }
    values
}

fn parse_realtime_summarization_types(raw: Option<&str>) -> Vec<String> {
    let mut values = Vec::new();
    for item in parse_csv_values(raw) {
        let normalized = item.trim().to_ascii_lowercase().replace(' ', "");
        let canonical = match normalized.as_str() {
            "paragraph" => Some("Paragraph".to_string()),
            "conversational" => Some("Conversational".to_string()),
            "questionsanswering" => Some("QuestionsAnswering".to_string()),
            "mindmap" => Some("MindMap".to_string()),
            _ => None,
        };
        if let Some(value) = canonical {
            if !values.contains(&value) {
                values.push(value);
            }
        }
    }
    values
}

fn map_realtime_state(state: RealtimeWorkerState) -> RecorderRealtimeState {
    match state {
        RealtimeWorkerState::Idle => RecorderRealtimeState::Idle,
        RealtimeWorkerState::Connecting => RecorderRealtimeState::Connecting,
        RealtimeWorkerState::Running => RecorderRealtimeState::Running,
        RealtimeWorkerState::Paused => RecorderRealtimeState::Paused,
        RealtimeWorkerState::Stopping => RecorderRealtimeState::Stopping,
        RealtimeWorkerState::Error => RecorderRealtimeState::Error,
    }
}

fn find_translation_target_segment_index(
    shared: &RecorderShared,
    translation: &PendingRealtimeTranslation,
) -> Option<usize> {
    if let Some(source_sentence_id) = translation.source_sentence_id.as_deref() {
        if let Some(index) = shared
            .realtime
            .segment_meta
            .iter()
            .rposition(|meta| meta.sentence_id.as_deref() == Some(source_sentence_id))
        {
            return Some(index);
        }
    }
    if let Some(sentence_index) = translation.sentence_index {
        if let Some(index) = shared
            .realtime
            .segment_meta
            .iter()
            .rposition(|meta| meta.sentence_index == Some(sentence_index))
        {
            return Some(index);
        }
    }
    if let Some(event_time_ms) = translation.event_time_ms {
        if let Some(index) = shared
            .realtime
            .segments
            .iter()
            .rposition(|segment| segment.end_ms == event_time_ms)
        {
            return Some(index);
        }
    }
    shared.realtime.segments.len().checked_sub(1)
}

fn apply_translation_to_segment(
    shared: &mut RecorderShared,
    translation: &PendingRealtimeTranslation,
) -> bool {
    let Some(index) = find_translation_target_segment_index(shared, translation) else {
        return false;
    };
    let Some(segment) = shared.realtime.segments.get_mut(index) else {
        return false;
    };
    segment.translation_text = Some(translation.text.clone());
    segment.translation_target_language = translation
        .target_language
        .clone()
        .or_else(|| Some(shared.realtime.translation_target_language.clone()));
    true
}

fn flush_pending_translations(shared: &mut RecorderShared) {
    if shared.realtime.pending_translations.is_empty() {
        return;
    }
    let pending = std::mem::take(&mut shared.realtime.pending_translations);
    let mut remaining = Vec::with_capacity(pending.len());
    for item in pending {
        if !apply_translation_to_segment(shared, &item) {
            remaining.push(item);
        }
    }
    shared.realtime.pending_translations = remaining;
}

fn apply_realtime_event(shared: &mut RecorderShared, event: RealtimeWorkerEvent) {
    match event {
        RealtimeWorkerEvent::StateChanged { state, error } => {
            shared.realtime.state = map_realtime_state(state);
            if let Some(reason) = error {
                shared.realtime.last_error = Some(reason);
                shared.realtime.state = RecorderRealtimeState::Error;
            }
            if matches!(
                state,
                RealtimeWorkerState::Idle | RealtimeWorkerState::Error
            ) {
                shared.realtime.worker = None;
                shared.realtime.event_rx = None;
                shared.realtime.enabled = false;
                shared.realtime.translation_enabled = false;
            }
        }
        RealtimeWorkerEvent::FinalSentence {
            text,
            event_time_ms,
            sentence_id,
            sentence_index,
        } => {
            let sentence = text.trim();
            if sentence.is_empty() {
                return;
            }
            if !shared.realtime.preview_text.is_empty() {
                shared.realtime.preview_text.push('\n');
            }
            shared.realtime.preview_text.push_str(sentence);

            let mut end_ms = event_time_ms.unwrap_or_else(|| elapsed_ms(shared));
            let start_ms = shared.realtime.next_segment_start_ms;
            if end_ms <= start_ms {
                end_ms = start_ms.saturating_add(1000);
            }
            shared
                .realtime
                .segments
                .push(crate::models::TranscriptSegment {
                    start_ms,
                    end_ms,
                    text: sentence.to_string(),
                    translation_text: None,
                    translation_target_language: None,
                    confidence: None,
                    speaker_id: None,
                    speaker_label: None,
                });
            shared.realtime.segment_meta.push(RealtimeSegmentMeta {
                sentence_id,
                sentence_index,
            });
            shared.realtime.next_segment_start_ms = end_ms;
            flush_pending_translations(shared);
        }
        RealtimeWorkerEvent::TranslatedSentence {
            text,
            event_time_ms,
            source_sentence_id,
            sentence_index,
            target_language,
        } => {
            if !shared.realtime.translation_enabled {
                return;
            }
            let translation = PendingRealtimeTranslation {
                text,
                event_time_ms,
                source_sentence_id,
                sentence_index,
                target_language,
            };
            if !apply_translation_to_segment(shared, &translation) {
                shared.realtime.pending_translations.push(translation);
                if shared.realtime.pending_translations.len() > 128 {
                    let drop_count = shared.realtime.pending_translations.len() - 128;
                    shared.realtime.pending_translations.drain(0..drop_count);
                }
            }
        }
    }
}

fn drain_realtime_events(shared: &mut RecorderShared) {
    let mut drained = Vec::new();
    if let Some(event_rx) = shared.realtime.event_rx.as_ref() {
        while let Ok(event) = event_rx.try_recv() {
            drained.push(event);
        }
    }
    for event in drained {
        apply_realtime_event(shared, event);
    }
}

fn resample_to_mono_target(
    data: &[i16],
    input_sample_rate: u32,
    input_channels: u16,
    target_sample_rate: u32,
    state: &mut RealtimeResampleState,
) -> Vec<i16> {
    if data.is_empty() || input_sample_rate == 0 || target_sample_rate == 0 {
        return vec![];
    }

    let channels = usize::from(input_channels.max(1));
    let mut output = Vec::with_capacity(data.len() / channels);
    let mut phase = state.phase;
    let target_rate = target_sample_rate;

    for frame in data.chunks_exact(channels) {
        let sample_sum = frame.iter().map(|value| i32::from(*value)).sum::<i32>();
        let avg = sample_sum / (channels as i32);
        let mono = avg.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16;

        phase = phase.saturating_add(target_rate);
        while phase >= input_sample_rate {
            output.push(mono);
            phase -= input_sample_rate;
        }
    }

    state.phase = phase;
    output
}

fn realtime_status_snapshot(shared: &mut RecorderShared) -> RecorderRealtimeStatus {
    drain_realtime_events(shared);
    RecorderRealtimeStatus {
        enabled: shared.realtime.enabled,
        source_language: shared.realtime.source_language.clone(),
        translation_enabled: shared.realtime.translation_enabled,
        translation_target_language: shared.realtime.translation_target_language.clone(),
        state: shared.realtime.state.clone(),
        preview_text: shared.realtime.preview_text.clone(),
        segment_count: shared.realtime.segments.len(),
        segments: shared.realtime.segments.clone(),
        last_error: shared.realtime.last_error.clone(),
    }
}

fn resolve_realtime_provider_config(
    storage: &Storage,
    source_language: String,
    translation_enabled: bool,
    translation_target_language: String,
) -> Result<AliyunTingwuRealtimeConfig, String> {
    let provider = storage
        .data
        .settings
        .providers
        .iter()
        .find(|item| item.kind == ProviderKind::AliyunTingwu)
        .ok_or_else(|| "aliyun tingwu provider not configured".to_string())?;
    let aliyun = provider
        .aliyun_tingwu
        .as_ref()
        .ok_or_else(|| "aliyun tingwu provider settings missing".to_string())?;

    let access_key_id = aliyun.access_key_id.clone().unwrap_or_default();
    let access_key_secret = aliyun.access_key_secret.clone().unwrap_or_default();
    let app_key = aliyun.app_key.clone().unwrap_or_default();
    if access_key_id.trim().is_empty()
        || access_key_secret.trim().is_empty()
        || app_key.trim().is_empty()
    {
        return Err(
            "aliyun realtime requires access key id, access key secret and app key".to_string(),
        );
    }

    let provider_source_language =
        normalize_realtime_source_language(&aliyun.realtime_source_language)
            .unwrap_or_else(|| DEFAULT_REALTIME_SOURCE_LANGUAGE.to_string());
    let source_language =
        normalize_realtime_source_language(&source_language).unwrap_or(provider_source_language);

    let mut translation_target_languages = if let Some(runtime_target) =
        normalize_realtime_translation_target_language(&translation_target_language)
    {
        vec![runtime_target]
    } else {
        parse_realtime_translation_target_languages(
            aliyun.realtime_translation_target_languages.as_deref(),
        )
    };
    if translation_target_languages.is_empty() {
        translation_target_languages.push(DEFAULT_REALTIME_TRANSLATION_TARGET_LANGUAGE.to_string());
    }
    let translation_enabled = translation_enabled
        && translation_target_languages
            .iter()
            .any(|item| item.as_str() != source_language.as_str());

    let realtime_format = match aliyun.realtime_format.trim().to_ascii_lowercase().as_str() {
        "pcm" | "opus" | "aac" | "speex" | "mp3" => {
            aliyun.realtime_format.trim().to_ascii_lowercase()
        }
        _ => "pcm".to_string(),
    };
    if realtime_format != "pcm" {
        return Err("open-recorder realtime currently supports Input.Format=pcm only".to_string());
    }
    let realtime_sample_rate = if aliyun.realtime_sample_rate == 8000 {
        8000
    } else {
        16000
    };
    let language_hints = if source_language == "multilingual" {
        parse_language_hints(aliyun.realtime_language_hints.as_deref())
    } else {
        Vec::new()
    };
    let task_key = aliyun
        .realtime_task_key
        .as_deref()
        .map(str::trim)
        .and_then(|value| {
            if value.is_empty() {
                None
            } else {
                Some(value.to_string())
            }
        });
    let transcription_phrase_id = aliyun
        .realtime_transcription_phrase_id
        .as_deref()
        .map(str::trim)
        .and_then(|value| {
            if value.is_empty() {
                None
            } else {
                Some(value.to_string())
            }
        });
    let transcoding_target_audio_format = aliyun
        .realtime_transcoding_target_audio_format
        .as_deref()
        .map(str::trim)
        .map(str::to_ascii_lowercase)
        .filter(|value| value == "mp3");

    Ok(AliyunTingwuRealtimeConfig {
        access_key_id,
        access_key_secret,
        app_key,
        endpoint: aliyun.endpoint.clone(),
        format: realtime_format,
        sample_rate: realtime_sample_rate,
        source_language,
        language_hints,
        task_key,
        progressive_callbacks_enabled: aliyun.realtime_progressive_callbacks_enabled,
        transcoding_target_audio_format,
        transcription_output_level: aliyun.realtime_transcription_output_level.clamp(1, 2),
        transcription_diarization_enabled: aliyun.realtime_transcription_diarization_enabled,
        transcription_diarization_speaker_count: aliyun
            .realtime_transcription_diarization_speaker_count
            .map(|value| value.clamp(0, 64)),
        transcription_phrase_id,
        translation_enabled,
        translation_output_level: aliyun.realtime_translation_output_level.clamp(1, 2),
        translation_target_languages,
        auto_chapters_enabled: aliyun.realtime_auto_chapters_enabled,
        meeting_assistance_enabled: aliyun.realtime_meeting_assistance_enabled,
        summarization_enabled: aliyun.realtime_summarization_enabled,
        summarization_types: parse_realtime_summarization_types(
            aliyun.realtime_summarization_types.as_deref(),
        ),
        text_polish_enabled: aliyun.realtime_text_polish_enabled,
        service_inspection_enabled: aliyun.realtime_service_inspection_enabled,
        service_inspection: aliyun.realtime_service_inspection.clone(),
        custom_prompt_enabled: aliyun.realtime_custom_prompt_enabled,
        custom_prompt: aliyun.realtime_custom_prompt.clone(),
    })
}

fn start_realtime_runtime(
    shared_ref: &Arc<Mutex<RecorderShared>>,
    storage: &Arc<Mutex<Storage>>,
) -> Result<(), String> {
    let (source_language, translation_enabled, translation_target_language) = {
        let shared = shared_ref
            .lock()
            .map_err(|_| "failed to acquire recorder state lock".to_string())?;
        (
            shared.realtime.source_language.clone(),
            shared.realtime.translation_enabled,
            shared.realtime.translation_target_language.clone(),
        )
    };
    let config = {
        let storage_lock = storage
            .lock()
            .map_err(|_| "failed to acquire storage lock".to_string())?;
        resolve_realtime_provider_config(
            &storage_lock,
            source_language,
            translation_enabled,
            translation_target_language,
        )?
    };
    let realtime_format = config.format.clone();
    let realtime_sample_rate = config.sample_rate;

    let mut shared = shared_ref
        .lock()
        .map_err(|_| "failed to acquire recorder state lock".to_string())?;
    drain_realtime_events(&mut shared);
    shared.realtime.format = realtime_format;
    shared.realtime.sample_rate = realtime_sample_rate;
    shared.realtime.resample_state = RealtimeResampleState::default();
    if shared.realtime.worker.is_some() {
        shared.realtime.enabled = true;
        return Ok(());
    }

    let (event_tx, event_rx) = mpsc::channel::<RealtimeWorkerEvent>();
    let worker = start_realtime_worker(config, event_tx);
    shared.realtime.enabled = true;
    shared.realtime.state = RecorderRealtimeState::Connecting;
    shared.realtime.last_error = None;
    shared.realtime.pending_translations.clear();
    shared.realtime.event_rx = Some(event_rx);
    shared.realtime.worker = Some(worker);
    Ok(())
}

fn stop_realtime_runtime(
    shared_ref: &Arc<Mutex<RecorderShared>>,
    disable_after_stop: bool,
) -> Result<(), String> {
    let worker = {
        let mut shared = shared_ref
            .lock()
            .map_err(|_| "failed to acquire recorder state lock".to_string())?;
        drain_realtime_events(&mut shared);
        shared.realtime.state = RecorderRealtimeState::Stopping;
        shared.realtime.worker.take()
    };

    let mut stop_error = None;
    if let Some(worker) = worker {
        if let Err(error) = worker.stop() {
            stop_error = Some(error);
        }
    }

    let mut shared = shared_ref
        .lock()
        .map_err(|_| "failed to acquire recorder state lock".to_string())?;
    drain_realtime_events(&mut shared);
    if disable_after_stop {
        shared.realtime.enabled = false;
        shared.realtime.translation_enabled = false;
    }
    shared.realtime.pending_translations.clear();
    shared.realtime.event_rx = None;
    shared.realtime.state = if stop_error.is_some() {
        RecorderRealtimeState::Error
    } else {
        RecorderRealtimeState::Idle
    };
    if let Some(error) = stop_error {
        shared.realtime.last_error = Some(error);
    }
    Ok(())
}

fn pause_realtime_if_session_paused(
    session_id: &str,
    shared_ref: &Arc<Mutex<RecorderShared>>,
    storage: &Arc<Mutex<Storage>>,
) -> Result<(), String> {
    let session_is_paused = {
        let storage = storage
            .lock()
            .map_err(|_| "failed to acquire storage lock".to_string())?;
        storage
            .data
            .sessions
            .get(session_id)
            .map(|session| matches!(session.status, SessionStatus::Paused))
            .unwrap_or(false)
    };
    if !session_is_paused {
        return Ok(());
    }
    if let Ok(mut shared) = shared_ref.lock() {
        if let Some(worker) = shared.realtime.worker.as_ref() {
            let _ = worker.pause();
            shared.realtime.state = RecorderRealtimeState::Paused;
        }
    }
    Ok(())
}

fn persist_realtime_segments(
    session_id: &str,
    shared_ref: &Arc<Mutex<RecorderShared>>,
    storage: &Arc<Mutex<Storage>>,
) -> Result<(), String> {
    let segments = {
        let mut shared = shared_ref
            .lock()
            .map_err(|_| "failed to acquire recorder state lock".to_string())?;
        drain_realtime_events(&mut shared);
        shared.realtime.segments.clone()
    };

    if segments.is_empty() {
        return Ok(());
    }

    let mut storage_lock = storage
        .lock()
        .map_err(|_| "failed to acquire storage lock".to_string())?;
    let session = storage_lock
        .data
        .sessions
        .get_mut(session_id)
        .ok_or_else(|| "session not found".to_string())?;
    session.transcript = segments
        .into_iter()
        .map(|mut segment| {
            segment.translation_text = None;
            segment.translation_target_language = None;
            segment
        })
        .collect();
    session.updated_at = now_iso();
    storage_lock.save()?;
    Ok(())
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

fn make_input_device_id(name: &str, ordinal: usize) -> String {
    format!("{ordinal}:{name}")
}

fn parse_input_device_id(raw: &str) -> Option<(usize, &str)> {
    let (ordinal_raw, name) = raw.split_once(':')?;
    let ordinal = ordinal_raw.parse::<usize>().ok()?;
    let trimmed_name = name.trim();
    if ordinal == 0 || trimmed_name.is_empty() {
        return None;
    }
    Some((ordinal, trimmed_name))
}

fn list_input_device_entries(
    host: &cpal::Host,
) -> Result<(Vec<InputDeviceEntry>, Option<usize>), String> {
    let default_name = host
        .default_input_device()
        .and_then(|device| device.name().ok());
    let mut ordinal_by_name: HashMap<String, usize> = HashMap::new();
    let mut entries = Vec::new();
    let mut default_index = None;

    let devices = host
        .input_devices()
        .map_err(|error| format!("failed to enumerate input devices: {error}"))?;
    for device in devices {
        let name = device
            .name()
            .unwrap_or_else(|_| UNKNOWN_INPUT_DEVICE_NAME.to_string());
        let ordinal = ordinal_by_name
            .entry(name.clone())
            .and_modify(|value| *value += 1)
            .or_insert(1);
        if default_index.is_none() && default_name.as_deref() == Some(name.as_str()) {
            default_index = Some(entries.len());
        }
        entries.push(InputDeviceEntry {
            device,
            id: make_input_device_id(&name, *ordinal),
            name,
        });
    }

    Ok((entries, default_index))
}

fn normalize_requested_input_device_id(value: Option<String>) -> Option<String> {
    value.and_then(|raw| {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn resolve_input_device(
    host: &cpal::Host,
    requested_input_device_id: Option<String>,
) -> Result<ResolvedInputDevice, String> {
    let (mut entries, default_index) = list_input_device_entries(host)?;
    if entries.is_empty() {
        return Err("no input device available".to_string());
    }

    let resolved_default_index = default_index.unwrap_or(0);
    let mut selected_index = resolved_default_index;
    let mut fallback_from = None;

    if let Some(requested_id) = requested_input_device_id {
        if let Some(index) = entries.iter().position(|entry| entry.id == requested_id) {
            selected_index = index;
        } else {
            let requested_name = parse_input_device_id(&requested_id)
                .map(|(_, name)| name.to_string())
                .or_else(|| {
                    let trimmed = requested_id.trim();
                    if trimmed.is_empty() {
                        None
                    } else {
                        Some(trimmed.to_string())
                    }
                });

            if let Some(name) = requested_name {
                if let Some(index) = entries.iter().position(|entry| entry.name == name) {
                    selected_index = index;
                } else {
                    fallback_from = Some(requested_id);
                }
            } else {
                fallback_from = Some(requested_id);
            }
        }
    }

    let selected = entries.swap_remove(selected_index);
    Ok(ResolvedInputDevice {
        device: selected.device,
        id: selected.id,
        name: selected.name,
        fallback_from,
    })
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

    if state.realtime.enabled {
        let realtime_frame = resample_to_mono_target(
            data,
            state.sample_rate,
            state.channels,
            state.realtime.sample_rate,
            &mut state.realtime.resample_state,
        );
        if !realtime_frame.is_empty() {
            if let Some(worker) = state.realtime.worker.as_ref() {
                worker.push_audio_frame(realtime_frame);
            }
        }
    }

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
    device: cpal::Device,
    stream_config: cpal::StreamConfig,
    sample_format: SampleFormat,
) -> Result<mpsc::Sender<AudioThreadCommand>, String> {
    let (control_tx, control_rx) = mpsc::channel::<AudioThreadCommand>();
    let (init_tx, init_rx) = mpsc::channel::<Result<(), String>>();

    thread::spawn(move || {
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
pub fn recorder_list_input_devices() -> Result<Vec<RecorderInputDevice>, String> {
    let host = cpal::default_host();
    let (entries, default_index) = list_input_device_entries(&host)?;
    Ok(entries
        .into_iter()
        .enumerate()
        .map(|(index, entry)| RecorderInputDevice {
            id: entry.id,
            name: entry.name,
            is_default: default_index == Some(index),
        })
        .collect())
}

#[tauri::command]
pub fn recorder_start(
    input_device_id: Option<String>,
    quality_preset: Option<RecordingQualityPreset>,
    realtime_enabled: Option<bool>,
    realtime_source_language: Option<String>,
    realtime_translate_enabled: Option<bool>,
    realtime_translate_target_language: Option<String>,
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
    let requested_input_device_id = normalize_requested_input_device_id(input_device_id);

    let session_id = Uuid::new_v4().to_string();
    let now = now_iso();
    let (
        segment_dir,
        segment_duration_secs,
        settings_input_device_id,
        realtime_enabled_by_default,
        realtime_format_by_default,
        realtime_sample_rate_by_default,
        realtime_source_language_by_default,
        realtime_translation_enabled_by_default,
        realtime_translation_target_language_by_default,
    ) = {
        let mut storage = state
            .storage
            .lock()
            .map_err(|_| "failed to acquire storage lock".to_string())?;
        storage.data.settings.normalize();
        let realtime_provider = storage
            .data
            .settings
            .providers
            .iter()
            .find(|provider| provider.kind == ProviderKind::AliyunTingwu)
            .and_then(|provider| provider.aliyun_tingwu.as_ref());
        let realtime_default = realtime_provider
            .map(|config| config.realtime_enabled_by_default)
            .unwrap_or(false);
        let realtime_source_language_default = realtime_provider
            .and_then(|config| normalize_realtime_source_language(&config.realtime_source_language))
            .unwrap_or_else(|| DEFAULT_REALTIME_SOURCE_LANGUAGE.to_string());
        let realtime_translation_enabled_default = realtime_provider
            .map(|config| config.realtime_translation_enabled)
            .unwrap_or(false);
        let realtime_translation_target_default = realtime_provider
            .map(|config| {
                parse_realtime_translation_target_languages(
                    config.realtime_translation_target_languages.as_deref(),
                )
            })
            .and_then(|items| items.first().cloned())
            .unwrap_or_else(|| DEFAULT_REALTIME_TRANSLATION_TARGET_LANGUAGE.to_string());
        let realtime_format_default = realtime_provider
            .map(|config| config.realtime_format.trim().to_ascii_lowercase())
            .filter(|value| value == "pcm")
            .unwrap_or_else(|| "pcm".to_string());
        let realtime_sample_rate_default = realtime_provider
            .map(|config| {
                if config.realtime_sample_rate == 8000 {
                    8000
                } else {
                    16000
                }
            })
            .unwrap_or(16000);
        (
            storage.session_audio_dir(&session_id)?,
            storage.data.settings.recording_segment_seconds,
            normalize_requested_input_device_id(
                storage.data.settings.recording_input_device_id.clone(),
            ),
            realtime_default,
            realtime_format_default,
            realtime_sample_rate_default,
            realtime_source_language_default,
            realtime_translation_enabled_default,
            realtime_translation_target_default,
        )
    };
    let preferred_input_device_id = requested_input_device_id.or(settings_input_device_id);

    let host = cpal::default_host();
    let resolved_device = resolve_input_device(&host, preferred_input_device_id)?;
    let (stream_config, sample_format) = select_stream_config(
        &resolved_device.device,
        target_config.capture_sample_rate,
        target_config.capture_channels,
    )?;

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
                discoverable: true,
                status: SessionStatus::Recording,
                created_at: now.clone(),
                updated_at: now,
                input_device_id: Some(resolved_device.id.clone()),
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
    let realtime_requested = realtime_enabled.unwrap_or(realtime_enabled_by_default);
    let realtime_source_language = realtime_source_language
        .as_deref()
        .and_then(normalize_realtime_source_language)
        .unwrap_or(realtime_source_language_by_default);
    let realtime_translation_target_language = realtime_translate_target_language
        .as_deref()
        .and_then(normalize_realtime_translation_target_language)
        .unwrap_or(realtime_translation_target_language_by_default);
    let realtime_translation_enabled = realtime_requested
        && realtime_translate_enabled.unwrap_or(realtime_translation_enabled_by_default);
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
        realtime: RecorderRealtimeRuntime {
            enabled: false,
            format: realtime_format_by_default,
            sample_rate: realtime_sample_rate_by_default,
            source_language: realtime_source_language,
            translation_enabled: realtime_translation_enabled,
            translation_target_language: realtime_translation_target_language,
            ..RecorderRealtimeRuntime::default()
        },
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
        resolved_device.device,
        stream_config,
        sample_format,
    )?;

    *runtime = RecorderRuntime::Active(ActiveRecorder {
        session_id: session_id.clone(),
        control_tx,
        shared,
    });

    if realtime_requested {
        let active_shared = match &*runtime {
            RecorderRuntime::Active(active) => Arc::clone(&active.shared),
            RecorderRuntime::Idle => unreachable!(),
        };
        if let Err(error) = start_realtime_runtime(&active_shared, &state.storage) {
            if let Ok(mut shared_state) = active_shared.lock() {
                shared_state.realtime.enabled = false;
                shared_state.realtime.state = RecorderRealtimeState::Error;
                shared_state.realtime.last_error = Some(error);
            }
        }
    }

    Ok(StartSessionResponse {
        session_id,
        input_device_id: Some(resolved_device.id),
        input_device_name: Some(resolved_device.name),
        fallback_from_input_device_id: resolved_device.fallback_from,
    })
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

    if let Ok(mut shared) = shared_ref.lock() {
        drain_realtime_events(&mut shared);
        if let Some(worker) = shared.realtime.worker.as_ref() {
            if let Err(error) = worker.pause() {
                shared.realtime.last_error = Some(error);
                shared.realtime.state = RecorderRealtimeState::Error;
            }
        }
    }
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

    if let Ok(mut shared) = shared_ref.lock() {
        drain_realtime_events(&mut shared);
        if let Some(worker) = shared.realtime.worker.as_ref() {
            if let Err(error) = worker.resume() {
                shared.realtime.last_error = Some(error);
                shared.realtime.state = RecorderRealtimeState::Error;
            }
        }
    }
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

    let _ = stop_realtime_runtime(&active.shared, true);
    let _ = persist_realtime_segments(&session_id, &active.shared, &state.storage);

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
pub fn recorder_toggle_realtime(
    session_id: String,
    enabled: bool,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let shared_ref = {
        let runtime = recorder_runtime()
            .lock()
            .map_err(|_| "failed to acquire recorder runtime lock".to_string())?;
        match &*runtime {
            RecorderRuntime::Active(active) if active.session_id == session_id => {
                Arc::clone(&active.shared)
            }
            RecorderRuntime::Active(_) => {
                return Err("another session is currently recording".to_string());
            }
            RecorderRuntime::Idle => return Err("no active recorder".to_string()),
        }
    };

    if enabled {
        start_realtime_runtime(&shared_ref, &state.storage)?;
        pause_realtime_if_session_paused(&session_id, &shared_ref, &state.storage)?;
        return Ok(());
    }

    if let Ok(mut shared) = shared_ref.lock() {
        shared.realtime.translation_enabled = false;
    }
    stop_realtime_runtime(&shared_ref, true)
}

#[tauri::command]
pub fn recorder_toggle_realtime_translation(
    session_id: String,
    enabled: bool,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let shared_ref = {
        let runtime = recorder_runtime()
            .lock()
            .map_err(|_| "failed to acquire recorder runtime lock".to_string())?;
        match &*runtime {
            RecorderRuntime::Active(active) if active.session_id == session_id => {
                Arc::clone(&active.shared)
            }
            RecorderRuntime::Active(_) => {
                return Err("another session is currently recording".to_string());
            }
            RecorderRuntime::Idle => return Err("no active recorder".to_string()),
        }
    };

    let should_restart = {
        let mut shared = shared_ref
            .lock()
            .map_err(|_| "failed to acquire recorder state lock".to_string())?;
        if !shared.realtime.enabled {
            shared.realtime.translation_enabled = false;
            return Err("realtime transcription is disabled".to_string());
        }
        shared.realtime.translation_enabled = enabled;
        shared.realtime.worker.is_some()
    };

    if !should_restart {
        return Ok(());
    }

    stop_realtime_runtime(&shared_ref, false)?;
    start_realtime_runtime(&shared_ref, &state.storage)?;
    pause_realtime_if_session_paused(&session_id, &shared_ref, &state.storage)?;
    Ok(())
}

#[tauri::command]
pub fn recorder_set_realtime_translation_target(
    session_id: String,
    target_language: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let normalized_target = normalize_realtime_translation_target_language(&target_language)
        .ok_or_else(|| {
            format!(
                "unsupported realtime translation target language: {}",
                target_language
            )
        })?;
    let shared_ref = {
        let runtime = recorder_runtime()
            .lock()
            .map_err(|_| "failed to acquire recorder runtime lock".to_string())?;
        match &*runtime {
            RecorderRuntime::Active(active) if active.session_id == session_id => {
                Arc::clone(&active.shared)
            }
            RecorderRuntime::Active(_) => {
                return Err("another session is currently recording".to_string());
            }
            RecorderRuntime::Idle => return Err("no active recorder".to_string()),
        }
    };

    let should_restart = {
        let mut shared = shared_ref
            .lock()
            .map_err(|_| "failed to acquire recorder state lock".to_string())?;
        shared.realtime.translation_target_language = normalized_target;
        shared.realtime.enabled
            && shared.realtime.translation_enabled
            && shared.realtime.worker.is_some()
    };

    if !should_restart {
        return Ok(());
    }

    stop_realtime_runtime(&shared_ref, false)?;
    start_realtime_runtime(&shared_ref, &state.storage)?;
    pause_realtime_if_session_paused(&session_id, &shared_ref, &state.storage)?;
    Ok(())
}

#[tauri::command]
pub fn recorder_set_realtime_source_language(
    session_id: String,
    source_language: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let normalized_source = normalize_realtime_source_language(&source_language)
        .ok_or_else(|| format!("unsupported realtime source language: {}", source_language))?;
    let shared_ref = {
        let runtime = recorder_runtime()
            .lock()
            .map_err(|_| "failed to acquire recorder runtime lock".to_string())?;
        match &*runtime {
            RecorderRuntime::Active(active) if active.session_id == session_id => {
                Arc::clone(&active.shared)
            }
            RecorderRuntime::Active(_) => {
                return Err("another session is currently recording".to_string());
            }
            RecorderRuntime::Idle => return Err("no active recorder".to_string()),
        }
    };

    let should_restart = {
        let mut shared = shared_ref
            .lock()
            .map_err(|_| "failed to acquire recorder state lock".to_string())?;
        shared.realtime.source_language = normalized_source;
        shared.realtime.enabled && shared.realtime.worker.is_some()
    };

    if !should_restart {
        return Ok(());
    }

    stop_realtime_runtime(&shared_ref, false)?;
    start_realtime_runtime(&shared_ref, &state.storage)?;
    pause_realtime_if_session_paused(&session_id, &shared_ref, &state.storage)?;
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

    let (elapsed_ms_runtime, rms, peak, pending_segment, runtime_phase, runtime_error, realtime) =
        match &*runtime {
            RecorderRuntime::Active(active) if active.session_id == session_id => {
                let mut shared = active
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
                    realtime_status_snapshot(&mut shared),
                )
            }
            _ => (
                0,
                0.0,
                0.0,
                0,
                RecorderPhase::Idle,
                None,
                RecorderRealtimeStatus {
                    enabled: false,
                    source_language: DEFAULT_REALTIME_SOURCE_LANGUAGE.to_string(),
                    translation_enabled: false,
                    translation_target_language: DEFAULT_REALTIME_TRANSLATION_TARGET_LANGUAGE
                        .to_string(),
                    state: RecorderRealtimeState::Idle,
                    preview_text: String::new(),
                    segment_count: 0,
                    segments: vec![],
                    last_error: None,
                },
            ),
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
        realtime,
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

fn resolve_existing_exported_audio_path(session: &Session, format: &str) -> Option<String> {
    let candidate = match format {
        "wav" => session.exported_wav_path.as_deref(),
        "mp3" => session.exported_mp3_path.as_deref(),
        "m4a" => session.exported_m4a_path.as_deref(),
        _ => None,
    }?;
    let normalized = candidate.trim();
    if normalized.is_empty() || !Path::new(normalized).exists() {
        return None;
    }
    Some(normalized.to_string())
}

fn resolve_fallback_export_source(session: &Session) -> Option<String> {
    [
        session.exported_m4a_path.as_deref(),
        session.exported_mp3_path.as_deref(),
        session.exported_wav_path.as_deref(),
    ]
    .into_iter()
    .flatten()
    .map(str::trim)
    .find(|path| !path.is_empty() && Path::new(path).exists())
    .map(str::to_string)
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

    let (segments, exact_existing_export, fallback_export_source, export_dir) = {
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
        (
            session.audio_segments.clone(),
            resolve_existing_exported_audio_path(session, &format),
            resolve_fallback_export_source(session),
            storage.session_export_dir(&session_id)?,
        )
    };

    if segments.is_empty() && exact_existing_export.is_none() && fallback_export_source.is_none() {
        return Err("no audio segments or merged audio available to export".to_string());
    }

    let base_name = format!("recording-{session_id}");
    let output_path = export_dir.join(format!("{base_name}.{format}"));

    let output_path = if let Some(existing_path) = exact_existing_export {
        PathBuf::from(existing_path)
    } else if !segments.is_empty() {
        merge_segments_with_ffmpeg(&segments, &output_path, &format)?;
        output_path
    } else {
        let source_path = fallback_export_source
            .ok_or_else(|| "no audio segments or merged audio available to export".to_string())?;
        convert_single_file(&source_path, &output_path, &format)?;
        output_path
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

#[cfg(test)]
mod tests {
    use std::{env, fs};

    use super::{
        make_input_device_id, parse_input_device_id, resolve_existing_exported_audio_path,
        resolve_fallback_export_source,
    };
    use crate::models::Session;
    use uuid::Uuid;

    #[test]
    fn parse_input_device_id_accepts_valid_values() {
        let id = make_input_device_id("Built-in Microphone", 2);
        let (ordinal, name) = parse_input_device_id(&id).expect("expected parsed input device id");
        assert_eq!(ordinal, 2);
        assert_eq!(name, "Built-in Microphone");
    }

    #[test]
    fn parse_input_device_id_rejects_invalid_values() {
        assert!(parse_input_device_id("abc").is_none());
        assert!(parse_input_device_id("0:Built-in").is_none());
        assert!(parse_input_device_id("1:   ").is_none());
    }

    #[test]
    fn exported_audio_lookup_ignores_missing_files_and_picks_existing_fallback() {
        let temp_path =
            env::temp_dir().join(format!("open-recorder-export-test-{}.m4a", Uuid::new_v4()));
        fs::write(&temp_path, b"test").expect("expected temp export file");

        let session = Session {
            exported_m4a_path: Some(temp_path.to_string_lossy().to_string()),
            exported_mp3_path: Some("/tmp/does-not-exist.mp3".to_string()),
            ..Default::default()
        };

        assert_eq!(
            resolve_existing_exported_audio_path(&session, "m4a"),
            Some(temp_path.to_string_lossy().to_string())
        );
        assert_eq!(
            resolve_fallback_export_source(&session),
            Some(temp_path.to_string_lossy().to_string())
        );

        let _ = fs::remove_file(&temp_path);
    }
}
