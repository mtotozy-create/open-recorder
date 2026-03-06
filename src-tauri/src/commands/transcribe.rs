use chrono::Utc;
use std::path::Path;
use tauri::State;
use uuid::Uuid;

use crate::{
    commands::recorder::merge_segments_with_ffmpeg,
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

    // 第一步：校验、创建 job 记录、提取配置（持有短暂锁）
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

        let exported_audio_for_transcription = [exported_m4a_path.as_deref(), exported_mp3_path.as_deref()]
            .into_iter()
            .flatten()
            .find(|path| Path::new(path).is_file())
            .map(str::to_string);

        if raw_segment_paths.is_empty() && exported_audio_for_transcription.is_none() {
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

        // 优先复用已导出的单文件（m4a > mp3），确保导出文件可直接用于后续转写。
        let (segment_paths, segment_meta) = if let Some(exported_path) = exported_audio_for_transcription
        {
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
                .unwrap_or_else(|| if session_sample_rate > 0 { session_sample_rate } else { 48_000 });
            let channels = raw_segment_meta
                .first()
                .map(|m| m.channels)
                .filter(|count| *count > 0)
                .unwrap_or_else(|| if session_channels > 0 { session_channels } else { 1 });
            let format = Path::new(&exported_path)
                .extension()
                .and_then(|ext| ext.to_str())
                .map(str::to_lowercase)
                .unwrap_or_else(|| "m4a".to_string());
            let exported_meta = vec![AudioSegmentMeta {
                path: exported_path.clone(),
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
            }];
            (vec![exported_path], exported_meta)
        } else if raw_segment_paths.len() <= 1 {
            (raw_segment_paths, raw_segment_meta)
        } else {
            // NOTE: 将多个分片合并为单一文件再转写，避免串行多次 API 调用导致等待过长
            // 合并后的文件放在 session audio 目录下，转写完成后无需保留（不影响原始分片）
            let audio_dir = storage.session_audio_dir(&session_id)?;
            let merged_path = audio_dir.join("_merged_for_transcription.m4a");

            match merge_segments_with_ffmpeg(&raw_segment_paths, &merged_path, "m4a") {
                Ok(()) => {
                    // 合并总时长 = 各分片时长之和
                    let total_duration_ms: u64 =
                        raw_segment_meta.iter().map(|m| m.duration_ms).sum();
                    let merged_meta = vec![AudioSegmentMeta {
                        path: merged_path.to_string_lossy().to_string(),
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
                        sample_rate: raw_segment_meta
                            .first()
                            .map(|m| m.sample_rate)
                            .unwrap_or(48000),
                        channels: raw_segment_meta.first().map(|m| m.channels).unwrap_or(1),
                        format: "m4a".to_string(),
                    }];
                    let merged_path_str = merged_path.to_string_lossy().to_string();
                    (vec![merged_path_str], merged_meta)
                }
                Err(merge_err) => {
                    // 合并失败时回退到原始多分片方式，不中断流程
                    eprintln!("[transcribe] segment merge failed, falling back to multi-segment: {merge_err}");
                    (raw_segment_paths, raw_segment_meta)
                }
            }
        };

        storage.save()?;

        let config = match settings.transcription_provider.clone() {
            TranscriptionProvider::Bailian => {
                let api_key = settings.bailian_api_key.clone().unwrap_or_default();
                if api_key.trim().is_empty() {
                    return Err("bailian transcription requires api key".to_string());
                }

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
            }
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

                if !has_full_credential {
                    return Err("aliyun tingwu transcription requires access key id, access key secret, and app key".to_string());
                }

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
                    language_hints: parse_language_hints(settings.aliyun_language_hints.as_deref()),
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
            }
        };

        (segment_paths, segment_meta, config)
    };

    // 第二步：clone Arc 到后台线程执行实际转写，命令立即返回
    let storage_arc = state.storage.clone();
    let job_id_clone = job_id.clone();
    let session_id_clone = session_id.clone();

    std::thread::spawn(move || {
        let transcript_result = match transcription_config {
            ActiveTranscriptionConfig::Bailian(config) => transcribe_with_bailian(
                &segment_paths,
                language_hint.as_deref(),
                &config,
                &segment_meta,
                &session_id_clone,
            ),
            ActiveTranscriptionConfig::AliyunTingwu(config) => transcribe_with_aliyun_tingwu(
                &segment_paths,
                &config,
                &segment_meta,
                &session_id_clone,
            ),
            ActiveTranscriptionConfig::Mock => Ok(mock_transcript(&segment_paths, &segment_meta)),
        };

        // 转写完成后清理合并的临时文件（若存在）
        // NOTE: 仅清理名为 _merged_for_transcription.m4a 的临时文件
        for path_str in &segment_paths {
            if path_str.ends_with("/_merged_for_transcription.m4a")
                || path_str.ends_with("\\_merged_for_transcription.m4a")
                || path_str == "_merged_for_transcription.m4a"
            {
                let _ = std::fs::remove_file(path_str);
            }
        }

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
