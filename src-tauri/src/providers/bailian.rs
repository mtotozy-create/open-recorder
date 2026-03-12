use std::{thread, time::Duration};

use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{
    models::{AudioSegmentMeta, SummaryResult, TranscriptSegment},
    providers::oss::{upload_segments_and_sign_urls, OssConfig},
};

// pub is needed for summary.rs
pub const BAILIAN_ASR_PATH: &str = "/api/v1/services/audio/asr/transcription";
pub const BAILIAN_COMPATIBLE_AUDIO_PATH: &str = "/compatible-mode/v1/audio/transcriptions";
pub const BAILIAN_COMPATIBLE_CHAT_PATH: &str = "/compatible-mode/v1/chat/completions";
const BAILIAN_TASK_PATH_PREFIX: &str = "/api/v1/tasks";
const MAX_BAILIAN_POLL_COUNT: usize = 120;
const BAILIAN_POLL_INTERVAL: Duration = Duration::from_secs(2);

#[derive(Debug, Clone)]
pub struct BailianConfig {
    pub base_url: String,
    pub api_key: String,
    pub transcription_model: String,
    pub oss: Option<OssConfig>,
}

#[derive(Debug, Clone)]
pub struct ChatCompatibleSummaryConfig {
    pub provider_name: String,
    pub endpoint: String,
    pub api_key: Option<String>,
    pub model: String,
}

#[derive(Debug, Serialize)]
struct ChatCompletionsRequest {
    model: String,
    messages: Vec<ChatMessage>,
    temperature: f32,
}

#[derive(Debug, Serialize, Deserialize)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionsResponse {
    choices: Vec<Choice>,
}

#[derive(Debug, Deserialize)]
struct Choice {
    message: ChatMessage,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SummaryPayload {
    title: String,
    decisions: Vec<String>,
    action_items: Vec<String>,
    risks: Vec<String>,
    timeline: Vec<String>,
    raw_markdown: String,
}

pub fn transcribe_with_bailian(
    segment_paths: &[String],
    _language_hint: Option<&str>,
    config: &BailianConfig,
    segment_meta: &[AudioSegmentMeta],
    session_id: &str,
    progress_callback: &dyn Fn(&str),
) -> Result<Vec<TranscriptSegment>, String> {
    let oss = config.oss.as_ref().ok_or_else(|| {
        "bailian transcription requires OSS config (access key, secret, endpoint, bucket)"
            .to_string()
    })?;
    progress_callback("Uploading to OSS...");
    let file_urls = upload_segments_and_sign_urls(segment_paths, session_id, oss)?;
    let endpoint = build_endpoint(&config.base_url, BAILIAN_ASR_PATH);

    let client = Client::builder()
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(600))
        .build()
        .map_err(|error| format!("failed to create http client: {error}"))?;

    let mut transcript = Vec::with_capacity(segment_paths.len());
    for (index, file_url) in file_urls.iter().enumerate() {
        let request = json!({
            "model": config.transcription_model.clone(),
            "input": {
                "file_urls": [file_url]
            }
        });

        progress_callback("Submitting transcription task...");
        let response = client
            .post(endpoint.as_str())
            .bearer_auth(&config.api_key)
            .header("X-DashScope-Async", "enable")
            .json(&request)
            .send()
            .map_err(|error| {
                format!("bailian transcription request failed for segment {index}: {error}")
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().unwrap_or_default();
            return Err(format!(
                "bailian transcription failed for segment {index} with {status}: {body}"
            ));
        }

        let payload: Value = response
            .json()
            .map_err(|error| format!("failed to parse transcription response: {error}"))?;

        let text = resolve_transcription_text(
            &client,
            &config.base_url,
            &config.api_key,
            &payload,
            index,
            progress_callback,
        )?;

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
        transcript.push(TranscriptSegment {
            start_ms,
            end_ms,
            text,
            translation_text: None,
            translation_target_language: None,
            confidence: None,
            speaker_id: None,
            speaker_label: None,
        });
    }

    Ok(transcript)
}

fn resolve_transcription_text(
    client: &Client,
    base_url: &str,
    api_key: &str,
    payload: &Value,
    segment_index: usize,
    progress_callback: &dyn Fn(&str),
) -> Result<String, String> {
    if let Some(text) = extract_transcription_text(payload) {
        return Ok(text);
    }

    let task_id = extract_string(
        payload,
        &[
            "/output/task_id",
            "/output/taskId",
            "/task_id",
            "/taskId",
            "/data/task_id",
            "/data/taskId",
        ],
    )
    .ok_or_else(|| {
        format!(
            "transcription response missing text/task_id for segment {segment_index}: {payload}"
        )
    })?;

    progress_callback("Polling transcription result...");
    let task_payload = poll_bailian_task(
        client,
        base_url,
        api_key,
        &task_id,
        segment_index,
        progress_callback,
    )?;
    if let Some(text) = extract_transcription_text(&task_payload) {
        return Ok(text);
    }

    let result_ref = extract_string(
        &task_payload,
        &[
            "/output/result",
            "/output/results/0/transcription_url",
            "/output/results/0/transcriptionUrl",
            "/output/results/0/result_url",
            "/output/results/0/resultUrl",
            "/result",
            "/data/result",
        ],
    )
    .ok_or_else(|| {
        format!("task succeeded but no transcription text/result for segment {segment_index}, task {task_id}: {task_payload}")
    })?;

    fetch_or_extract_transcript_text(client, &result_ref).map_err(|error| {
        format!(
            "failed to load transcription result for segment {segment_index}, task {task_id}: {error}"
        )
    })
}

fn poll_bailian_task(
    client: &Client,
    base_url: &str,
    api_key: &str,
    task_id: &str,
    segment_index: usize,
    progress_callback: &dyn Fn(&str),
) -> Result<Value, String> {
    let endpoint = format!(
        "{}/{}",
        normalize_bailian_base_url(base_url),
        format!(
            "{}/{}",
            BAILIAN_TASK_PATH_PREFIX.trim_start_matches('/'),
            task_id
        )
    );

    progress_callback("Polling transcription result...");
    for _ in 0..MAX_BAILIAN_POLL_COUNT {
        let response = client
            .get(endpoint.as_str())
            .bearer_auth(api_key)
            .send()
            .map_err(|error| {
                format!(
                    "failed to poll bailian task for segment {segment_index}, task {task_id}: {error}"
                )
            })?;
        let status = response.status();
        let payload: Value = response
            .json()
            .map_err(|error| format!("failed to parse poll response JSON: {error}"))?;

        if !status.is_success() {
            return Err(format!(
                "polling task failed for segment {segment_index}, task {task_id} with {status}: {payload}"
            ));
        }

        let task_status = extract_string(
            &payload,
            &[
                "/output/task_status",
                "/output/taskStatus",
                "/task_status",
                "/taskStatus",
                "/status",
            ],
        )
        .unwrap_or_default()
        .to_ascii_uppercase();

        if is_success_status(&task_status) {
            return Ok(payload);
        }

        if is_failed_status(&task_status) {
            return Err(format!(
                "task failed for segment {segment_index}, task {task_id}, status={task_status}, payload={payload}"
            ));
        }

        thread::sleep(BAILIAN_POLL_INTERVAL);
    }

    Err(format!(
        "bailian polling timed out for segment {segment_index}, task {task_id}"
    ))
}

fn fetch_or_extract_transcript_text(client: &Client, result_ref: &str) -> Result<String, String> {
    let result_ref = result_ref.trim();
    if result_ref.is_empty() {
        return Err("empty result ref".to_string());
    }

    if result_ref.starts_with("http://") || result_ref.starts_with("https://") {
        let response = client
            .get(result_ref)
            .send()
            .map_err(|error| format!("failed to fetch result url: {error}"))?;
        let status = response.status();
        let raw_text = response
            .text()
            .map_err(|error| format!("failed to read result response body: {error}"))?;

        if !status.is_success() {
            return Err(format!(
                "result url request failed with {status}; body={raw_text}"
            ));
        }

        if let Ok(payload) = serde_json::from_str::<Value>(&raw_text) {
            extract_text_from_payload(&payload)
                .ok_or_else(|| format!("result payload has no transcript text: {payload}"))
        } else if !raw_text.trim().is_empty() {
            Ok(raw_text.trim().to_string())
        } else {
            Err("result response body is empty".to_string())
        }
    } else {
        Ok(result_ref.to_string())
    }
}

pub fn summarize_with_chat_compatible(
    transcript: &[TranscriptSegment],
    system_prompt: &str,
    user_prompt: &str,
    config: &ChatCompatibleSummaryConfig,
) -> Result<SummaryResult, String> {
    let transcript_text = transcript
        .iter()
        .map(|segment| segment.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");

    let request = ChatCompletionsRequest {
        model: config.model.clone(),
        messages: vec![
            ChatMessage {
                role: "system".to_string(),
                content: system_prompt.to_string(),
            },
            ChatMessage {
                role: "user".to_string(),
                content: format!("{user_prompt}\n\n{transcript_text}"),
            },
        ],
        temperature: 0.2,
    };

    let client = Client::builder()
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(600))
        .build()
        .map_err(|error| format!("failed to create http client: {error}"))?;

    let request_builder = client.post(&config.endpoint);
    let request_builder = if let Some(api_key) = config
        .api_key
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        request_builder.bearer_auth(api_key)
    } else {
        request_builder
    };

    let response = request_builder
        .json(&request)
        .send()
        .map_err(|error| format!("{} request failed: {error}", config.provider_name))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().unwrap_or_default();
        return Err(format!(
            "{} request failed with {status}: {body}",
            config.provider_name
        ));
    }

    let payload: ChatCompletionsResponse = response
        .json()
        .map_err(|error| format!("failed to parse {} response: {error}", config.provider_name))?;

    let raw_content = payload
        .choices
        .first()
        .ok_or_else(|| format!("{} response has empty choices", config.provider_name))?
        .message
        .content
        .trim()
        .to_string();

    let json_text = extract_json(&raw_content);
    if let Ok(summary) = serde_json::from_str::<SummaryPayload>(json_text) {
        return Ok(SummaryResult {
            title: summary.title,
            decisions: summary.decisions,
            action_items: summary.action_items,
            risks: summary.risks,
            timeline: summary.timeline,
            raw_markdown: summary.raw_markdown,
        });
    }

    Ok(SummaryResult {
        title: derive_summary_title(&raw_content),
        decisions: vec![],
        action_items: vec![],
        risks: vec![],
        timeline: vec![],
        raw_markdown: raw_content,
    })
}

fn extract_transcription_text(payload: &Value) -> Option<String> {
    [
        "/text",
        "/output/text",
        "/output/transcript",
        "/output/transcription",
        "/output/sentence",
        "/result/text",
        "/result/transcript",
        "/Data/Text",
        "/Data/Result/Text",
    ]
    .iter()
    .find_map(|pointer| {
        payload
            .pointer(pointer)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    })
}

fn extract_string(payload: &Value, pointers: &[&str]) -> Option<String> {
    pointers.iter().find_map(|pointer| {
        payload
            .pointer(pointer)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    })
}

fn extract_text_from_payload(payload: &Value) -> Option<String> {
    if let Some(text) = extract_transcription_text(payload) {
        return Some(text);
    }

    let mut lines = vec![];
    collect_text_lines(payload, &mut lines);
    let merged = lines.join("\n");
    if merged.trim().is_empty() {
        None
    } else {
        Some(merged)
    }
}

fn collect_text_lines(value: &Value, lines: &mut Vec<String>) {
    match value {
        Value::Object(map) => {
            for (key, item) in map {
                if item.is_string() && is_text_key(key) {
                    if let Some(text) = item.as_str() {
                        let text = text.trim();
                        if !text.is_empty() {
                            lines.push(text.to_string());
                        }
                    }
                } else {
                    collect_text_lines(item, lines);
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_text_lines(item, lines);
            }
        }
        _ => {}
    }
}

fn is_text_key(key: &str) -> bool {
    matches!(
        key.to_ascii_lowercase().as_str(),
        "text" | "content" | "transcript" | "sentence"
    )
}

fn is_success_status(status: &str) -> bool {
    matches!(status, "SUCCEEDED" | "SUCCESS" | "COMPLETED" | "FINISHED")
}

fn is_failed_status(status: &str) -> bool {
    matches!(
        status,
        "FAILED" | "FAILURE" | "ERROR" | "CANCELED" | "CANCELLED"
    )
}

fn extract_json(raw: &str) -> &str {
    let trimmed = raw.trim();
    if let Some(stripped) = trimmed
        .strip_prefix("```json")
        .and_then(|value| value.strip_suffix("```"))
    {
        return stripped.trim();
    }

    if let Some(stripped) = trimmed
        .strip_prefix("```")
        .and_then(|value| value.strip_suffix("```"))
    {
        return stripped.trim();
    }

    trimmed
}

fn derive_summary_title(raw: &str) -> String {
    let first_line = raw
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or_default()
        .trim_start_matches('#')
        .trim();

    if first_line.is_empty() {
        "Meeting Summary".to_string()
    } else {
        first_line.to_string()
    }
}

pub fn build_endpoint(base_url: &str, path: &str) -> String {
    format!("{}{}", normalize_bailian_base_url(base_url), path)
}

pub fn normalize_bailian_base_url(base_url: &str) -> String {
    let trimmed = base_url.trim().trim_end_matches('/');
    let known_suffixes = [
        BAILIAN_ASR_PATH,
        BAILIAN_COMPATIBLE_AUDIO_PATH,
        BAILIAN_COMPATIBLE_CHAT_PATH,
    ];

    for suffix in known_suffixes {
        if let Some(stripped) = trimmed.strip_suffix(suffix) {
            return stripped.trim_end_matches('/').to_string();
        }
    }

    trimmed.to_string()
}

#[cfg(test)]
mod tests {
    use super::{
        build_endpoint, BAILIAN_ASR_PATH, BAILIAN_COMPATIBLE_AUDIO_PATH,
        BAILIAN_COMPATIBLE_CHAT_PATH,
    };

    #[test]
    fn build_endpoint_uses_new_asr_path() {
        let endpoint = build_endpoint("https://dashscope.aliyuncs.com", BAILIAN_ASR_PATH);
        assert_eq!(
            endpoint,
            "https://dashscope.aliyuncs.com/api/v1/services/audio/asr/transcription"
        );
    }

    #[test]
    fn build_endpoint_accepts_full_asr_url_as_base() {
        let endpoint = build_endpoint(
            "https://dashscope.aliyuncs.com/api/v1/services/audio/asr/transcription",
            BAILIAN_ASR_PATH,
        );
        assert_eq!(
            endpoint,
            "https://dashscope.aliyuncs.com/api/v1/services/audio/asr/transcription"
        );
    }

    #[test]
    fn build_endpoint_strips_known_transcription_suffix_for_summary() {
        let endpoint = build_endpoint(
            "https://dashscope.aliyuncs.com/compatible-mode/v1/audio/transcriptions",
            BAILIAN_COMPATIBLE_CHAT_PATH,
        );
        assert_eq!(
            endpoint,
            "https://dashscope.aliyuncs.com/compatible-mode/v1/chat/completions"
        );
    }

    #[test]
    fn build_endpoint_strips_full_asr_url_for_summary() {
        let endpoint = build_endpoint(
            "https://dashscope.aliyuncs.com/api/v1/services/audio/asr/transcription",
            BAILIAN_COMPATIBLE_CHAT_PATH,
        );
        assert_eq!(
            endpoint,
            "https://dashscope.aliyuncs.com/compatible-mode/v1/chat/completions"
        );
    }

    #[test]
    fn build_endpoint_strips_chat_suffix_for_transcription() {
        let endpoint = build_endpoint(
            "https://dashscope.aliyuncs.com/compatible-mode/v1/chat/completions",
            BAILIAN_COMPATIBLE_AUDIO_PATH,
        );
        assert_eq!(
            endpoint,
            "https://dashscope.aliyuncs.com/compatible-mode/v1/audio/transcriptions"
        );
    }
}
