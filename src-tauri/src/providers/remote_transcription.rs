use std::{
    fs, thread,
    time::{Duration, Instant},
};

use reqwest::blocking::{multipart, Client};
use serde::Deserialize;
use uuid::Uuid;

use crate::models::{AudioSegmentMeta, RemoteTranscriptionProviderSettings, TranscriptSegment};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
struct JobResponse {
    id: String,
    status: String,
    transcript: Option<String>,
    transcript_segments: Option<Vec<RemoteSegment>>,
    error: Option<RemoteError>,
}

#[derive(Debug, Deserialize)]
struct RemoteSegment {
    start_seconds: f64,
    end_seconds: f64,
    text: String,
}

#[derive(Debug, Deserialize)]
struct RemoteError {
    message: String,
}

fn endpoint(base: &str, suffix: &str) -> String {
    let base = base.trim_end_matches('/');
    let jobs_base = if base.ends_with("/api/v1/transcription-jobs") {
        base.to_string()
    } else if base.ends_with("/api/v1") {
        format!("{base}/transcription-jobs")
    } else {
        format!("{base}/api/v1/transcription-jobs")
    };
    format!("{jobs_base}{suffix}")
}

pub fn transcribe_with_remote_service(
    audio_paths: &[String],
    config: &RemoteTranscriptionProviderSettings,
    segment_meta: &[AudioSegmentMeta],
    update_progress: &dyn Fn(&str),
) -> Result<Vec<TranscriptSegment>, String> {
    if audio_paths.len() != 1 {
        return Err("remote transcription requires a merged audio file".to_string());
    }
    let path = &audio_paths[0];
    let metadata = fs::metadata(path).map_err(|e| format!("failed to inspect audio: {e}"))?;
    if metadata.len() > 100 * 1024 * 1024 {
        return Err("audio file exceeds TodoVoice 100 MiB limit".to_string());
    }
    let token = config.api_token.as_deref().unwrap_or("").trim();
    if config.base_url.trim().is_empty() || token.is_empty() {
        return Err("TodoVoice base URL and bearer token are required".to_string());
    }
    let client = Client::builder()
        .timeout(Duration::from_secs(90))
        .build()
        .map_err(|e| e.to_string())?;
    let file_name = std::path::Path::new(path)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("recording.wav")
        .to_string();
    let content_type = match std::path::Path::new(path)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "wav" => "audio/wav",
        "mp3" => "audio/mpeg",
        "m4a" | "mp4" => "audio/mp4",
        "ogg" | "opus" => "audio/ogg",
        "webm" => "audio/webm",
        _ => "application/octet-stream",
    };
    let file = multipart::Part::file(path)
        .map_err(|e| format!("failed to open audio: {e}"))?
        .file_name(file_name)
        .mime_str(content_type)
        .map_err(|e| format!("failed to prepare audio upload: {e}"))?;
    let form = multipart::Form::new().part("file", file);
    update_progress("Uploading audio to TodoVoice...");
    let response = client
        .post(endpoint(&config.base_url, "/upload"))
        .bearer_auth(token)
        .header("Idempotency-Key", Uuid::new_v4().to_string())
        .multipart(form)
        .send()
        .map_err(|e| format!("TodoVoice upload failed: {e}"))?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().unwrap_or_default();
        let detail = if body.trim().is_empty() {
            "no response body".to_string()
        } else {
            body.chars().take(1000).collect()
        };
        return Err(format!(
            "TodoVoice upload failed with HTTP {status}: {detail}"
        ));
    }
    let created: JobResponse = response
        .json()
        .map_err(|e| format!("invalid TodoVoice response: {e}"))?;
    let deadline =
        Instant::now() + Duration::from_secs(config.max_polling_minutes.clamp(1, 720) * 60);
    loop {
        if Instant::now() > deadline {
            return Err("TodoVoice transcription timed out".to_string());
        }
        let poll_response = match client
            .get(endpoint(&config.base_url, &format!("/{}", created.id)))
            .bearer_auth(token)
            .send()
        {
            Ok(response) => response,
            Err(error) => {
                update_progress(&format!("TodoVoice polling retry: {error}"));
                thread::sleep(Duration::from_secs(2));
                continue;
            }
        };
        if !poll_response.status().is_success() {
            let status = poll_response.status();
            let body = poll_response.text().unwrap_or_default();
            let detail = body.chars().take(1000).collect::<String>();
            if status.is_server_error() || status == reqwest::StatusCode::TOO_MANY_REQUESTS {
                update_progress(&format!("TodoVoice polling retry: HTTP {status}"));
                thread::sleep(Duration::from_secs(2));
                continue;
            }
            return Err(format!(
                "TodoVoice polling failed with HTTP {status}: {detail}"
            ));
        }
        let job: JobResponse = poll_response
            .json()
            .map_err(|e| format!("invalid TodoVoice polling response: {e}"))?;
        match job.status.as_str() {
            "succeeded" => {
                if let Some(segments) = job.transcript_segments {
                    return Ok(segments
                        .into_iter()
                        .map(|s| TranscriptSegment {
                            start_ms: (s.start_seconds.max(0.0) * 1000.0) as u64,
                            end_ms: (s.end_seconds.max(0.0) * 1000.0) as u64,
                            text: s.text,
                            translation_text: None,
                            translation_target_language: None,
                            confidence: None,
                            speaker_id: None,
                            speaker_label: None,
                        })
                        .collect());
                }
                let text = job.transcript.unwrap_or_default();
                return Ok(vec![TranscriptSegment {
                    start_ms: 0,
                    end_ms: segment_meta.iter().map(|m| m.duration_ms).sum(),
                    text,
                    translation_text: None,
                    translation_target_language: None,
                    confidence: None,
                    speaker_id: None,
                    speaker_label: None,
                }]);
            }
            "failed" | "cancelled" => {
                return Err(job
                    .error
                    .map(|e| e.message)
                    .unwrap_or_else(|| format!("TodoVoice job {}", job.status)))
            }
            _ => {
                update_progress(&format!("TodoVoice transcription: {}", job.status));
                thread::sleep(Duration::from_secs(
                    config.poll_interval_seconds.clamp(1, 300),
                ));
            }
        }
    }
}
