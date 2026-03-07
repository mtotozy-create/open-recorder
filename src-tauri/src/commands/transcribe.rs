use chrono::Utc;
use serde::Serialize;
use std::path::Path;
use tauri::State;
use uuid::Uuid;

use crate::{
    commands::recorder::merge_segments_with_ffmpeg,
    models::{
        AudioSegmentMeta, Job, JobEnqueueResponse, JobKind, JobStatus, OssConfig, OssProviderKind,
        ProviderCapability, ProviderKind, SessionStatus, Settings,
    },
    providers::{
        aliyun_tingwu::{transcribe_with_aliyun_tingwu, AliyunTingwuConfig},
        bailian::{transcribe_with_bailian, BailianConfig},
        oss::{OssConfig as ProviderOssConfig, OssProviderKind as ProviderOssProviderKind},
    },
    state::AppState,
};

fn now_iso() -> String {
    Utc::now().to_rfc3339()
}

fn parse_language_hints(raw: Option<&str>) -> Vec<String> {
    raw.map(|value| {
        let normalized = value.replace('，', ",");
        normalized
            .split(',')
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .map(str::to_string)
            .collect::<Vec<_>>()
    })
    .unwrap_or_default()
}

fn provider_supports_transcription(
    provider_capabilities: &[ProviderCapability],
    enabled: bool,
) -> bool {
    enabled
        && provider_capabilities
            .iter()
            .any(|item| *item == ProviderCapability::Transcription)
}

fn has_oss_credential(oss: &OssConfig) -> bool {
    oss.access_key_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .is_some()
        && oss
            .access_key_secret
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .is_some()
        && oss
            .endpoint
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .is_some()
        && oss
            .bucket
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .is_some()
}

fn resolve_selected_oss_config(settings: &Settings) -> Result<ProviderOssConfig, String> {
    let selected = settings
        .oss_configs
        .iter()
        .find(|config| config.id == settings.selected_oss_config_id)
        .ok_or_else(|| {
            format!(
                "selected OSS config '{}' not found",
                settings.selected_oss_config_id
            )
        })?;

    if !has_oss_credential(selected) {
        return Err(format!(
            "selected OSS config '{}' is incomplete; access key id/secret, endpoint and bucket are required",
            selected.name
        ));
    }

    let kind = match selected.kind {
        OssProviderKind::Aliyun => ProviderOssProviderKind::Aliyun,
        OssProviderKind::R2 => ProviderOssProviderKind::R2,
    };

    Ok(ProviderOssConfig {
        kind,
        access_key_id: selected.access_key_id.clone().unwrap_or_default(),
        access_key_secret: selected.access_key_secret.clone().unwrap_or_default(),
        endpoint: selected.endpoint.clone().unwrap_or_default(),
        bucket: selected.bucket.clone().unwrap_or_default(),
        path_prefix: selected.path_prefix.clone(),
        signed_url_ttl_seconds: selected.signed_url_ttl_seconds.clamp(60, 86_400),
    })
}

enum ActiveTranscriptionConfig {
    Bailian(BailianConfig),
    AliyunTingwu(AliyunTingwuConfig),
}

struct PreparedTranscriptionAudio {
    segment_paths: Vec<String>,
    segment_meta: Vec<AudioSegmentMeta>,
    selected_path: String,
    selected_format: String,
    merged: bool,
    merged_file_size_bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionPreparedAudioResponse {
    pub path: String,
    pub format: String,
    pub merged: bool,
}

fn to_exported_audio_meta(
    exported_path: &str,
    raw_segment_meta: &[AudioSegmentMeta],
    session_elapsed_ms: u64,
    session_sample_rate: u32,
    session_channels: u16,
) -> Vec<AudioSegmentMeta> {
    let total_duration_ms: u64 = raw_segment_meta.iter().map(|m| m.duration_ms).sum();
    let duration_ms = if total_duration_ms > 0 {
        total_duration_ms
    } else if session_elapsed_ms > 0 {
        session_elapsed_ms
    } else {
        600_000
    };
    let sample_rate = raw_segment_meta
        .first()
        .map(|m| m.sample_rate)
        .filter(|rate| *rate > 0)
        .unwrap_or_else(|| {
            if session_sample_rate > 0 {
                session_sample_rate
            } else {
                48_000
            }
        });
    let channels = raw_segment_meta
        .first()
        .map(|m| m.channels)
        .filter(|count| *count > 0)
        .unwrap_or_else(|| {
            if session_channels > 0 {
                session_channels
            } else {
                1
            }
        });
    let format = Path::new(exported_path)
        .extension()
        .and_then(|ext| ext.to_str())
        .map(str::to_lowercase)
        .unwrap_or_else(|| "m4a".to_string());
    vec![AudioSegmentMeta {
        path: exported_path.to_string(),
        sequence: 0,
        started_at: raw_segment_meta
            .first()
            .map(|m| m.started_at.clone())
            .unwrap_or_default(),
        ended_at: raw_segment_meta
            .last()
            .map(|m| m.ended_at.clone())
            .unwrap_or_default(),
        duration_ms,
        sample_rate,
        channels,
        format,
        file_size_bytes: std::fs::metadata(exported_path).map(|m| m.len()).unwrap_or(0),
    }]
}

fn resolve_transcription_audio_input(
    session_id: &str,
    raw_segment_paths: Vec<String>,
    raw_segment_meta: Vec<AudioSegmentMeta>,
    session_elapsed_ms: u64,
    session_sample_rate: u32,
    session_channels: u16,
    exported_m4a_path: Option<String>,
    exported_mp3_path: Option<String>,
    export_dir: &Path,
    allow_merge_failure_fallback: bool,
    update_progress: &dyn Fn(&str),
) -> Result<PreparedTranscriptionAudio, String> {
    let exported_audio_for_transcription =
        [exported_m4a_path.as_deref(), exported_mp3_path.as_deref()]
            .into_iter()
            .flatten()
            .find(|path| Path::new(path).is_file())
            .map(str::to_string);

    if raw_segment_paths.is_empty() && exported_audio_for_transcription.is_none() {
        return Err("audio segments and exported files are empty; record or export audio first".to_string());
    }

    if let Some(exported_path) = exported_audio_for_transcription {
        let exported_meta = to_exported_audio_meta(
            &exported_path,
            &raw_segment_meta,
            session_elapsed_ms,
            session_sample_rate,
            session_channels,
        );
        let selected_format = exported_meta
            .first()
            .map(|meta| meta.format.clone())
            .unwrap_or_else(|| "m4a".to_string());
        return Ok(PreparedTranscriptionAudio {
            segment_paths: vec![exported_path.clone()],
            segment_meta: exported_meta,
            selected_path: exported_path,
            selected_format,
            merged: false,
            merged_file_size_bytes: None,
        });
    }

    if raw_segment_paths.len() <= 1 {
        let selected_path = raw_segment_paths
            .first()
            .cloned()
            .ok_or_else(|| "audio segments and exported files are empty; record or export audio first".to_string())?;
        let selected_format = Path::new(selected_path.as_str())
            .extension()
            .and_then(|ext| ext.to_str())
            .map(str::to_lowercase)
            .or_else(|| raw_segment_meta.first().map(|item| item.format.to_lowercase()))
            .unwrap_or_else(|| "wav".to_string());
        return Ok(PreparedTranscriptionAudio {
            segment_paths: raw_segment_paths,
            segment_meta: raw_segment_meta,
            selected_path,
            selected_format,
            merged: false,
            merged_file_size_bytes: None,
        });
    }

    update_progress("合并音频中...");
    let merged_path = export_dir.join(format!("recording-{session_id}.m4a"));

    match merge_segments_with_ffmpeg(&raw_segment_paths, &merged_path, "m4a") {
        Ok(()) => {
            let total_duration_ms: u64 = raw_segment_meta.iter().map(|m| m.duration_ms).sum();
            let file_size_bytes = std::fs::metadata(&merged_path).map(|m| m.len()).unwrap_or(0);
            let merged_path_str = merged_path.to_string_lossy().to_string();
            let merged_meta = vec![AudioSegmentMeta {
                path: merged_path_str.clone(),
                sequence: 0,
                started_at: raw_segment_meta
                    .first()
                    .map(|m| m.started_at.clone())
                    .unwrap_or_default(),
                ended_at: raw_segment_meta
                    .last()
                    .map(|m| m.ended_at.clone())
                    .unwrap_or_default(),
                duration_ms: total_duration_ms,
                sample_rate: raw_segment_meta.first().map(|m| m.sample_rate).unwrap_or(48000),
                channels: raw_segment_meta.first().map(|m| m.channels).unwrap_or(1),
                format: "m4a".to_string(),
                file_size_bytes,
            }];
            Ok(PreparedTranscriptionAudio {
                segment_paths: vec![merged_path_str.clone()],
                segment_meta: merged_meta,
                selected_path: merged_path_str,
                selected_format: "m4a".to_string(),
                merged: true,
                merged_file_size_bytes: Some(file_size_bytes),
            })
        }
        Err(merge_err) => {
            if allow_merge_failure_fallback {
                eprintln!(
                    "[transcribe] segment merge failed, falling back to multi-segment: {merge_err}"
                );
                Ok(PreparedTranscriptionAudio {
                    selected_path: raw_segment_paths[0].clone(),
                    selected_format: Path::new(raw_segment_paths[0].as_str())
                        .extension()
                        .and_then(|ext| ext.to_str())
                        .map(str::to_lowercase)
                        .unwrap_or_else(|| "wav".to_string()),
                    segment_paths: raw_segment_paths,
                    segment_meta: raw_segment_meta,
                    merged: false,
                    merged_file_size_bytes: None,
                })
            } else {
                Err(format!("failed to merge audio segments: {merge_err}"))
            }
        }
    }
}

#[tauri::command]
pub fn session_prepare_transcription_audio(
    session_id: String,
    state: State<'_, AppState>,
) -> Result<SessionPreparedAudioResponse, String> {
    let (
        raw_segment_paths,
        raw_segment_meta,
        session_elapsed_ms,
        session_sample_rate,
        session_channels,
        exported_m4a_path,
        exported_mp3_path,
        export_dir,
    ) = {
        let storage = state
            .storage
            .lock()
            .map_err(|_| "failed to acquire storage lock".to_string())?;
        let session = storage
            .data
            .sessions
            .get(&session_id)
            .ok_or_else(|| "session not found".to_string())?;

        (
            session.audio_segments.clone(),
            session.audio_segment_meta.clone(),
            session.elapsed_ms,
            session.sample_rate,
            session.channels,
            session.exported_m4a_path.clone(),
            session.exported_mp3_path.clone(),
            storage.session_export_dir(&session_id)?,
        )
    };

    let prepared = resolve_transcription_audio_input(
        &session_id,
        raw_segment_paths,
        raw_segment_meta,
        session_elapsed_ms,
        session_sample_rate,
        session_channels,
        exported_m4a_path,
        exported_mp3_path,
        &export_dir,
        false,
        &|_| {},
    )?;

    if prepared.merged {
        let mut storage = state
            .storage
            .lock()
            .map_err(|_| "failed to acquire storage lock".to_string())?;
        if let Some(session) = storage.data.sessions.get_mut(&session_id) {
            session.exported_m4a_path = Some(prepared.selected_path.clone());
            session.exported_m4a_size = Some(prepared.merged_file_size_bytes.unwrap_or(0));
            session.exported_m4a_created_at = Some(now_iso());
            session.updated_at = now_iso();
        }
        storage.save()?;
    }

    Ok(SessionPreparedAudioResponse {
        path: prepared.selected_path,
        format: prepared.selected_format,
        merged: prepared.merged,
    })
}

#[tauri::command]
pub fn transcribe_enqueue(
    session_id: String,
    language_hint: Option<String>,
    state: State<'_, AppState>,
) -> Result<JobEnqueueResponse, String> {
    let job_id = Uuid::new_v4().to_string();
    let now = now_iso();

    // 第一步：校验、创建 job 记录、提取配置（持有短暂锁）
    let (
        raw_segment_paths,
        raw_segment_meta,
        session_elapsed_ms,
        session_sample_rate,
        session_channels,
        exported_m4a_path,
        exported_mp3_path,
        export_dir,
        transcription_config,
    ) = {
        let mut storage = state
            .storage
            .lock()
            .map_err(|_| "failed to acquire storage lock".to_string())?;

        storage.data.jobs.insert(
            job_id.clone(),
            Job {
                id: job_id.clone(),
                session_id: session_id.clone(),
                kind: JobKind::Transcription,
                status: JobStatus::Running,
                created_at: now.clone(),
                updated_at: now.clone(),
                error: None,
                progress_msg: None,
            },
        );

        storage.data.settings.normalize();
        let settings = storage.data.settings.clone();
        let (
            raw_segment_paths,
            raw_segment_meta,
            exported_m4a_path,
            exported_mp3_path,
            session_elapsed_ms,
            session_sample_rate,
            session_channels,
        ) = {
            let session = storage
                .data
                .sessions
                .get_mut(&session_id)
                .ok_or_else(|| "session not found".to_string())?;
            session.status = SessionStatus::Transcribing;
            session.updated_at = now_iso();
            (
                session.audio_segments.clone(),
                session.audio_segment_meta.clone(),
                session.exported_m4a_path.clone(),
                session.exported_mp3_path.clone(),
                session.elapsed_ms,
                session.sample_rate,
                session.channels,
            )
        };

        if raw_segment_paths.is_empty() && exported_m4a_path.is_none() && exported_mp3_path.is_none()
        {
            if let Some(session) = storage.data.sessions.get_mut(&session_id) {
                session.status = SessionStatus::Failed;
                session.updated_at = now_iso();
            }
            if let Some(job) = storage.data.jobs.get_mut(&job_id) {
                job.status = JobStatus::Failed;
                job.error = Some(
                    "audio segments and exported files are empty; record or export audio first"
                        .to_string(),
                );
                job.updated_at = now_iso();
            }
            storage.save()?;
            return Err(
                "audio segments and exported files are empty; record or export audio first"
                    .to_string(),
            );
        }

        let export_dir = storage.session_export_dir(&session_id)?;

        let provider = settings
            .providers
            .iter()
            .find(|provider| provider.id == settings.selected_transcription_provider_id)
            .ok_or_else(|| {
                format!(
                    "selected transcription provider '{}' not found",
                    settings.selected_transcription_provider_id
                )
            })?;

        if !provider_supports_transcription(&provider.capabilities, provider.enabled) {
            return Err(format!(
                "selected transcription provider '{}' is disabled or not transcription-capable",
                provider.name
            ));
        }

        let selected_oss_config = resolve_selected_oss_config(&settings)?;

        let config = match provider.kind {
            ProviderKind::Bailian => {
                let bailian = provider.bailian.as_ref().ok_or_else(|| {
                    format!("provider '{}' missing bailian config", provider.name)
                })?;

                let api_key = bailian.api_key.clone().unwrap_or_default();
                if api_key.trim().is_empty() {
                    return Err(format!(
                        "provider '{}' requires API key for transcription",
                        provider.name
                    ));
                }

                ActiveTranscriptionConfig::Bailian(BailianConfig {
                    base_url: bailian.base_url.clone(),
                    api_key,
                    transcription_model: bailian.transcription_model.clone(),
                    oss: Some(selected_oss_config.clone()),
                })
            }
            ProviderKind::AliyunTingwu => {
                let aliyun = provider.aliyun_tingwu.as_ref().ok_or_else(|| {
                    format!("provider '{}' missing aliyun_tingwu config", provider.name)
                })?;

                let has_full_credential = aliyun
                    .access_key_id
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .is_some()
                    && aliyun
                        .access_key_secret
                        .as_deref()
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .is_some()
                    && aliyun
                        .app_key
                        .as_deref()
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .is_some();

                if !has_full_credential {
                    return Err(format!(
                        "provider '{}' requires access key id, access key secret, and app key",
                        provider.name
                    ));
                }

                ActiveTranscriptionConfig::AliyunTingwu(AliyunTingwuConfig {
                    access_key_id: aliyun.access_key_id.clone().unwrap_or_default(),
                    access_key_secret: aliyun.access_key_secret.clone().unwrap_or_default(),
                    app_key: aliyun.app_key.clone().unwrap_or_default(),
                    endpoint: aliyun.endpoint.clone(),
                    source_language: aliyun.source_language.clone(),
                    file_url_prefix: aliyun.file_url_prefix.clone(),
                    oss: Some(selected_oss_config.clone()),
                    language_hints: parse_language_hints(aliyun.language_hints.as_deref()),
                    transcription_normalization_enabled: aliyun.transcription_normalization_enabled,
                    transcription_paragraph_enabled: aliyun.transcription_paragraph_enabled,
                    transcription_punctuation_prediction_enabled: aliyun
                        .transcription_punctuation_prediction_enabled,
                    transcription_disfluency_removal_enabled: aliyun
                        .transcription_disfluency_removal_enabled,
                    transcription_speaker_diarization_enabled: aliyun
                        .transcription_speaker_diarization_enabled,
                    poll_interval_seconds: aliyun.poll_interval_seconds.clamp(60, 300),
                    max_polling_minutes: aliyun.max_polling_minutes.clamp(5, 720),
                })
            }
            ProviderKind::Openrouter => {
                return Err(format!(
                    "provider '{}' does not support transcription",
                    provider.name
                ))
            }
        };

        (
            raw_segment_paths,
            raw_segment_meta,
            session_elapsed_ms,
            session_sample_rate,
            session_channels,
            exported_m4a_path,
            exported_mp3_path,
            export_dir,
            config,
        )
    };

    // 第二步：clone Arc 到后台线程执行实际转写，包含合并分片部分，释放 UI 阻塞
    let storage_arc = state.storage.clone();
    let job_id_clone = job_id.clone();
    let session_id_clone = session_id.clone();
    let language_hint_clone = language_hint.clone();

    std::thread::spawn(move || {
        let update_progress = |msg: &str| {
            if let Ok(mut storage) = storage_arc.lock() {
                if let Some(job) = storage.data.jobs.get_mut(&job_id_clone) {
                    job.progress_msg = Some(msg.to_string());
                    job.updated_at = now_iso();
                }
                let _ = storage.save();
            }
        };

        let prepared_audio = match resolve_transcription_audio_input(
            &session_id_clone,
            raw_segment_paths,
            raw_segment_meta,
            session_elapsed_ms,
            session_sample_rate,
            session_channels,
            exported_m4a_path,
            exported_mp3_path,
            &export_dir,
            true,
            &update_progress,
        ) {
            Ok(value) => value,
            Err(error) => {
                if let Ok(mut storage) = storage_arc.lock() {
                    if let Some(session) = storage.data.sessions.get_mut(&session_id_clone) {
                        session.status = SessionStatus::Failed;
                        session.updated_at = now_iso();
                    }
                    if let Some(job) = storage.data.jobs.get_mut(&job_id_clone) {
                        job.status = JobStatus::Failed;
                        job.progress_msg = None;
                        job.error = Some(error);
                        job.updated_at = now_iso();
                    }
                    let _ = storage.save();
                }
                return;
            }
        };

        if prepared_audio.merged {
            if let Ok(mut storage) = storage_arc.lock() {
                if let Some(session) = storage.data.sessions.get_mut(&session_id_clone) {
                    session.exported_m4a_path = Some(prepared_audio.selected_path.clone());
                    session.exported_m4a_size = Some(prepared_audio.merged_file_size_bytes.unwrap_or(0));
                    session.exported_m4a_created_at = Some(now_iso());
                    session.updated_at = now_iso();
                }
                let _ = storage.save();
            }
        }

        let segment_paths = prepared_audio.segment_paths;
        let segment_meta = prepared_audio.segment_meta;

        let transcript_result = match transcription_config {
            ActiveTranscriptionConfig::Bailian(config) => transcribe_with_bailian(
                &segment_paths,
                language_hint_clone.as_deref(),
                &config,
                &segment_meta,
                &session_id_clone,
                &update_progress,
            ),
            ActiveTranscriptionConfig::AliyunTingwu(config) => transcribe_with_aliyun_tingwu(
                &segment_paths,
                &config,
                &segment_meta,
                &session_id_clone,
                &update_progress,
            ),
        };

        // 回写结果到 storage
        if let Ok(mut storage) = storage_arc.lock() {
            match transcript_result {
                Ok(transcript) => {
                    if let Some(session) = storage.data.sessions.get_mut(&session_id_clone) {
                        session.transcript = transcript;
                        session.status = SessionStatus::Stopped;
                        session.updated_at = now_iso();
                    }
                    if let Some(job) = storage.data.jobs.get_mut(&job_id_clone) {
                        job.status = JobStatus::Completed;
                        job.error = None;
                        job.progress_msg = None;
                        job.updated_at = now_iso();
                    }
                }
                Err(error) => {
                    if let Some(session) = storage.data.sessions.get_mut(&session_id_clone) {
                        session.status = SessionStatus::Failed;
                        session.updated_at = now_iso();
                    }
                    if let Some(job) = storage.data.jobs.get_mut(&job_id_clone) {
                        job.status = JobStatus::Failed;
                        job.progress_msg = None;
                        job.error = Some(error);
                        job.updated_at = now_iso();
                    }
                }
            }
            let _ = storage.save();
        }
    });

    Ok(JobEnqueueResponse { job_id })
}
