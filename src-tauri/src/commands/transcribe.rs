use chrono::Utc;
use tauri::State;
use uuid::Uuid;

use crate::{
    models::{
        AudioSegmentMeta, Job, JobEnqueueResponse, JobKind, JobStatus, SessionStatus,
        TranscriptSegment, TranscriptionProvider,
    },
    providers::{
        aliyun_oss::AliyunOssConfig,
        aliyun_tingwu::{transcribe_with_aliyun_tingwu, AliyunTingwuConfig},
        bailian::{transcribe_with_bailian, BailianConfig},
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

enum ActiveTranscriptionConfig {
    Bailian(BailianConfig),
    AliyunTingwu(AliyunTingwuConfig),
    Mock,
}

#[tauri::command]
pub fn transcribe_enqueue(
    session_id: String,
    language_hint: Option<String>,
    state: State<'_, AppState>,
) -> Result<JobEnqueueResponse, String> {
    let job_id = Uuid::new_v4().to_string();
    let now = now_iso();

    let (segment_paths, segment_meta, transcription_config) = {
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
            },
        );

        let settings = storage.data.settings.clone();
        let (segment_paths, segment_meta) = {
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
            )
        };

        if segment_paths.is_empty() {
            if let Some(session) = storage.data.sessions.get_mut(&session_id) {
                session.status = SessionStatus::Failed;
                session.updated_at = now_iso();
            }
            if let Some(job) = storage.data.jobs.get_mut(&job_id) {
                job.status = JobStatus::Failed;
                job.error = Some("audio segments are empty; record audio first".to_string());
                job.updated_at = now_iso();
            }
            storage.save()?;
            return Err("audio segments are empty; record audio first".to_string());
        }

        storage.save()?;

        let config = match settings.transcription_provider.clone() {
            TranscriptionProvider::Bailian => settings
                .bailian_api_key
                .clone()
                .map(|api_key| {
                    let has_oss_credential = settings
                        .bailian_oss_access_key_id
                        .as_deref()
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .is_some()
                        && settings
                            .bailian_oss_access_key_secret
                            .as_deref()
                            .map(str::trim)
                            .filter(|value| !value.is_empty())
                            .is_some()
                        && settings
                            .bailian_oss_endpoint
                            .as_deref()
                            .map(str::trim)
                            .filter(|value| !value.is_empty())
                            .is_some()
                        && settings
                            .bailian_oss_bucket
                            .as_deref()
                            .map(str::trim)
                            .filter(|value| !value.is_empty())
                            .is_some();

                    ActiveTranscriptionConfig::Bailian(BailianConfig {
                        base_url: settings.bailian_base_url.clone(),
                        api_key,
                        transcription_model: settings.bailian_transcription_model.clone(),
                        summary_model: settings.bailian_summary_model.clone(),
                        oss: has_oss_credential.then(|| AliyunOssConfig {
                            access_key_id: settings
                                .bailian_oss_access_key_id
                                .clone()
                                .unwrap_or_default(),
                            access_key_secret: settings
                                .bailian_oss_access_key_secret
                                .clone()
                                .unwrap_or_default(),
                            endpoint: settings.bailian_oss_endpoint.clone().unwrap_or_default(),
                            bucket: settings.bailian_oss_bucket.clone().unwrap_or_default(),
                            path_prefix: settings.bailian_oss_path_prefix.clone(),
                            signed_url_ttl_seconds: settings.bailian_oss_signed_url_ttl_seconds,
                        }),
                    })
                })
                .unwrap_or(ActiveTranscriptionConfig::Mock),
            TranscriptionProvider::AliyunTingwu => {
                let has_full_credential = settings
                    .aliyun_access_key_id
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .is_some()
                    && settings
                        .aliyun_access_key_secret
                        .as_deref()
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .is_some()
                    && settings
                        .aliyun_app_key
                        .as_deref()
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .is_some();

                if has_full_credential {
                    let has_oss_credential = settings
                        .bailian_oss_access_key_id
                        .as_deref()
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .is_some()
                        && settings
                            .bailian_oss_access_key_secret
                            .as_deref()
                            .map(str::trim)
                            .filter(|value| !value.is_empty())
                            .is_some()
                        && settings
                            .bailian_oss_endpoint
                            .as_deref()
                            .map(str::trim)
                            .filter(|value| !value.is_empty())
                            .is_some()
                        && settings
                            .bailian_oss_bucket
                            .as_deref()
                            .map(str::trim)
                            .filter(|value| !value.is_empty())
                            .is_some();

                    ActiveTranscriptionConfig::AliyunTingwu(AliyunTingwuConfig {
                        access_key_id: settings.aliyun_access_key_id.unwrap_or_default(),
                        access_key_secret: settings.aliyun_access_key_secret.unwrap_or_default(),
                        app_key: settings.aliyun_app_key.unwrap_or_default(),
                        endpoint: settings.aliyun_endpoint.clone(),
                        source_language: settings.aliyun_source_language.clone(),
                        file_url_prefix: settings.aliyun_file_url_prefix.clone(),
                        oss: has_oss_credential.then(|| AliyunOssConfig {
                            access_key_id: settings
                                .bailian_oss_access_key_id
                                .clone()
                                .unwrap_or_default(),
                            access_key_secret: settings
                                .bailian_oss_access_key_secret
                                .clone()
                                .unwrap_or_default(),
                            endpoint: settings.bailian_oss_endpoint.clone().unwrap_or_default(),
                            bucket: settings.bailian_oss_bucket.clone().unwrap_or_default(),
                            path_prefix: settings.bailian_oss_path_prefix.clone(),
                            signed_url_ttl_seconds: settings.bailian_oss_signed_url_ttl_seconds,
                        }),
                        language_hints: parse_language_hints(
                            settings.aliyun_language_hints.as_deref(),
                        ),
                        transcription_normalization_enabled: settings
                            .aliyun_transcription_normalization_enabled,
                        transcription_paragraph_enabled: settings
                            .aliyun_transcription_paragraph_enabled,
                        transcription_punctuation_prediction_enabled: settings
                            .aliyun_transcription_punctuation_prediction_enabled,
                        transcription_disfluency_removal_enabled: settings
                            .aliyun_transcription_disfluency_removal_enabled,
                        transcription_speaker_diarization_enabled: settings
                            .aliyun_transcription_speaker_diarization_enabled,
                        poll_interval_seconds: settings.aliyun_poll_interval_seconds.clamp(60, 300),
                        max_polling_minutes: settings.aliyun_max_polling_minutes.clamp(5, 720),
                    })
                } else {
                    ActiveTranscriptionConfig::Mock
                }
            }
        };

        (segment_paths, segment_meta, config)
    };

    let transcript_result = match transcription_config {
        ActiveTranscriptionConfig::Bailian(config) => transcribe_with_bailian(
            &segment_paths,
            language_hint.as_deref(),
            &config,
            &segment_meta,
            &session_id,
        ),
        ActiveTranscriptionConfig::AliyunTingwu(config) => {
            transcribe_with_aliyun_tingwu(&segment_paths, &config, &segment_meta, &session_id)
        }
        ActiveTranscriptionConfig::Mock => Ok(mock_transcript(&segment_paths, &segment_meta)),
    };

    let mut storage = state
        .storage
        .lock()
        .map_err(|_| "failed to acquire storage lock".to_string())?;

    match transcript_result {
        Ok(transcript) => {
            if let Some(session) = storage.data.sessions.get_mut(&session_id) {
                session.transcript = transcript;
                session.status = SessionStatus::Stopped;
                session.updated_at = now_iso();
            }

            if let Some(job) = storage.data.jobs.get_mut(&job_id) {
                job.status = JobStatus::Completed;
                job.error = None;
                job.updated_at = now_iso();
            }
        }
        Err(error) => {
            if let Some(session) = storage.data.sessions.get_mut(&session_id) {
                session.status = SessionStatus::Failed;
                session.updated_at = now_iso();
            }

            if let Some(job) = storage.data.jobs.get_mut(&job_id) {
                job.status = JobStatus::Failed;
                job.error = Some(error.clone());
                job.updated_at = now_iso();
            }

            storage.save()?;
            return Err(error);
        }
    }

    storage.save()?;
    Ok(JobEnqueueResponse { job_id })
}

fn mock_transcript(
    segment_paths: &[String],
    segment_meta: &[AudioSegmentMeta],
) -> Vec<TranscriptSegment> {
    segment_paths
        .iter()
        .enumerate()
        .map(|(index, path)| {
            let start_ms = segment_meta
                .iter()
                .take(index)
                .map(|value| value.duration_ms)
                .sum::<u64>();
            let duration_ms = segment_meta
                .get(index)
                .map(|value| value.duration_ms)
                .unwrap_or(600_000);
            let end_ms = start_ms + duration_ms;
            TranscriptSegment {
                start_ms,
                end_ms,
                text: format!("[mock] transcript from segment file: {path}"),
                confidence: Some(0.8),
            }
        })
        .collect()
}
