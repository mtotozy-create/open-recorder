use std::{path::Path, thread, time::Duration};

use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine};
use chrono::Utc;
use hmac::{Hmac, Mac};
use reqwest::{
    blocking::Client,
    header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE, DATE},
    Method,
};
use serde_json::{json, Value};
use sha1::Sha1;
use uuid::Uuid;

use crate::{
    models::{AudioSegmentMeta, TranscriptSegment},
    providers::oss::{upload_segments_and_sign_urls, OssConfig},
};

type HmacSha1 = Hmac<Sha1>;

const ALIYUN_API_VERSION: &str = "2023-09-30";
const ALIYUN_SIGNATURE_METHOD: &str = "HMAC-SHA1";
const ALIYUN_SIGNATURE_VERSION: &str = "1.0";

#[derive(Debug, Clone)]
pub struct AliyunTingwuConfig {
    pub access_key_id: String,
    pub access_key_secret: String,
    pub app_key: String,
    pub endpoint: String,
    pub source_language: String,
    pub file_url_prefix: Option<String>,
    pub oss: Option<OssConfig>,
    pub language_hints: Vec<String>,
    pub transcription_normalization_enabled: bool,
    pub transcription_paragraph_enabled: bool,
    pub transcription_punctuation_prediction_enabled: bool,
    pub transcription_disfluency_removal_enabled: bool,
    pub transcription_speaker_diarization_enabled: bool,
    pub poll_interval_seconds: u64,
    pub max_polling_minutes: u64,
}

pub fn transcribe_with_aliyun_tingwu(
    segment_paths: &[String],
    config: &AliyunTingwuConfig,
    segment_meta: &[AudioSegmentMeta],
    session_id: &str,
    progress_callback: &dyn Fn(&str),
) -> Result<Vec<TranscriptSegment>, String> {
    let client = Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(90))
        .build()
        .map_err(|error| format!("failed to create http client: {error}"))?;

    let endpoint = config.endpoint.trim_end_matches('/');
    let create_path = "/openapi/tingwu/v2/tasks";
    let create_query = "type=offline";
    let create_resource = format!("{create_path}?{create_query}");
    let file_urls =
        resolve_segment_file_urls(segment_paths, config, session_id, progress_callback)?;

    let mut transcript = Vec::with_capacity(segment_paths.len());
    for (index, file_url) in file_urls.iter().enumerate() {
        let input = json!({
            "SourceLanguage": config.source_language,
            "TaskKey": format!("open-recorder-{}", Uuid::new_v4()),
            "FileUrl": file_url
        });

        // 通义听悟 v2 API 要求 Transcription 配置在 Parameters 下
        let mut transcription_params = json!({});
        if config.transcription_disfluency_removal_enabled {
            transcription_params["DisfluencyRemovalEnabled"] = json!(true);
        }
        if config.transcription_speaker_diarization_enabled {
            transcription_params["DiarizationEnabled"] = json!(true);
            transcription_params["Diarization"] = json!({
                "SpeakerCount": 0
            });
        }

        let mut parameters = json!({
            "Transcription": transcription_params
        });

        if !config.language_hints.is_empty() {
            parameters["LanguageHints"] = json!(config.language_hints);
        }

        let create_body = json!({
            "AppKey": config.app_key,
            "Input": input,
            "Parameters": parameters
        });

        progress_callback("Submitting transcription task...");
        let create_url = format!("{endpoint}{create_path}?{create_query}");
        let create_payload = send_signed_json_request(
            &client,
            Method::PUT,
            &create_url,
            &create_resource,
            Some(&create_body),
            config,
        )
        .map_err(|error| {
            format!("failed to create aliyun tingwu task for segment {index}: {error}")
        })?;

        let task_id = extract_string(
            &create_payload,
            &[
                "/Data/TaskId",
                "/Data/taskId",
                "/taskId",
                "/TaskId",
                "/data/taskId",
                "/data/task_id",
            ],
        )
        .ok_or_else(|| {
            format!(
                "aliyun tingwu create task response missing task id for segment {index}: {create_payload}"
            )
        })?;

        let task_status_path = format!("/openapi/tingwu/v2/tasks/{task_id}");
        let task_status_url = format!("{endpoint}{task_status_path}");

        let task_result = poll_task_result_url(
            &client,
            &task_status_url,
            &task_status_path,
            config,
            index,
            &task_id,
            progress_callback,
        )?;

        let text = fetch_or_extract_transcript_text(&client, &task_result).map_err(|error| {
            format!(
                "aliyun tingwu result parsing failed for segment {index} task {task_id}: {error}"
            )
        })?;

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

fn send_signed_json_request(
    client: &Client,
    method: Method,
    url: &str,
    canonicalized_resource: &str,
    body: Option<&Value>,
    config: &AliyunTingwuConfig,
) -> Result<Value, String> {
    let accept = "application/json";
    let body_text = match body {
        Some(value) => Some(
            serde_json::to_string(value)
                .map_err(|error| format!("failed to serialize request body: {error}"))?,
        ),
        None => None,
    };
    let content_type = if body_text.is_some() {
        "application/json; charset=utf-8"
    } else {
        ""
    };
    let content_md5 = body_text
        .as_ref()
        .map(|value| BASE64_STANDARD.encode(md5::compute(value.as_bytes()).0))
        .unwrap_or_default();
    let date = Utc::now().format("%a, %d %b %Y %H:%M:%S GMT").to_string();
    let nonce = Uuid::new_v4().to_string();

    let signature_headers = vec![
        (
            "x-acs-signature-method".to_string(),
            ALIYUN_SIGNATURE_METHOD.to_string(),
        ),
        (
            "x-acs-signature-version".to_string(),
            ALIYUN_SIGNATURE_VERSION.to_string(),
        ),
        ("x-acs-signature-nonce".to_string(), nonce.clone()),
        ("x-acs-version".to_string(), ALIYUN_API_VERSION.to_string()),
    ];
    let canonicalized_headers = build_canonicalized_headers(&signature_headers);

    let string_to_sign = format!(
        "{}\n{}\n{}\n{}\n{}\n{}{}",
        method.as_str(),
        accept,
        content_md5,
        content_type,
        date,
        canonicalized_headers,
        canonicalized_resource
    );

    let mut mac = HmacSha1::new_from_slice(config.access_key_secret.as_bytes())
        .map_err(|error| format!("failed to initialize HMAC: {error}"))?;
    mac.update(string_to_sign.as_bytes());
    let signature = BASE64_STANDARD.encode(mac.finalize().into_bytes());
    let authorization = format!("acs {}:{signature}", config.access_key_id);

    let mut request_builder = client
        .request(method, url)
        .header(ACCEPT, accept)
        .header(DATE, date)
        .header(AUTHORIZATION, authorization)
        .header("x-acs-signature-method", ALIYUN_SIGNATURE_METHOD)
        .header("x-acs-signature-version", ALIYUN_SIGNATURE_VERSION)
        .header("x-acs-signature-nonce", nonce)
        .header("x-acs-version", ALIYUN_API_VERSION);

    if !content_type.is_empty() {
        request_builder = request_builder.header(CONTENT_TYPE, content_type);
    }
    if !content_md5.is_empty() {
        request_builder = request_builder.header("Content-MD5", content_md5);
    }
    if let Some(body_text) = body_text {
        request_builder = request_builder.body(body_text);
    }

    let response = request_builder
        .send()
        .map_err(|error| format!("request failed: {error}"))?;
    let status = response.status();
    let body_text = response
        .text()
        .map_err(|error| format!("failed to read response body: {error}"))?;

    if !status.is_success() {
        return Err(format!("request failed with {status}: {body_text}"));
    }

    serde_json::from_str::<Value>(&body_text)
        .map_err(|error| format!("failed to parse JSON response: {error}; body={body_text}"))
}

fn poll_task_result_url(
    client: &Client,
    status_url: &str,
    status_resource: &str,
    config: &AliyunTingwuConfig,
    segment_index: usize,
    task_id: &str,
    progress_callback: &dyn Fn(&str),
) -> Result<String, String> {
    let poll_interval = Duration::from_secs(config.poll_interval_seconds.clamp(60, 300));
    let max_poll_count = max_poll_count(config);

    progress_callback("Polling transcription result...");
    for _ in 0..max_poll_count {
        let payload = send_signed_json_request(
            client,
            Method::GET,
            status_url,
            status_resource,
            None,
            config,
        )?;

        let status_value = extract_string(
            &payload,
            &[
                "/Data/TaskStatus",
                "/Data/taskStatus",
                "/TaskStatus",
                "/taskStatus",
                "/status",
                "/Status",
            ],
        )
        .unwrap_or_default()
        .to_ascii_uppercase();

        if is_success_status(&status_value) {
            if let Some(result) = extract_task_result_ref(&payload) {
                return Ok(result);
            }

            if let Some(result_payload) = payload
                .pointer("/Data/Result")
                .or_else(|| payload.pointer("/Data/result"))
            {
                if let Some(text) = extract_text_from_payload(result_payload) {
                    return Ok(text);
                }
            }

            return Err(format!(
                "task completed but result url missing for segment {segment_index}, task {task_id}: {payload}"
            ));
        }

        if is_failed_status(&status_value) {
            return Err(format!(
                "task failed for segment {segment_index}, task {task_id}, status={status_value}, payload={payload}"
            ));
        }

        thread::sleep(poll_interval);
    }

    Err(format!(
        "aliyun tingwu polling timed out for segment {segment_index}, task {task_id}"
    ))
}

fn max_poll_count(config: &AliyunTingwuConfig) -> usize {
    let interval = config.poll_interval_seconds.clamp(60, 300);
    let total_seconds = config.max_polling_minutes.clamp(5, 720).saturating_mul(60);
    let count = total_seconds / interval.max(1);
    count.max(1) as usize
}

fn extract_task_result_ref(payload: &Value) -> Option<String> {
    extract_string(
        payload,
        &[
            "/Data/Result/TranscriptionUrl",
            "/Data/result/transcriptionUrl",
            "/Data/Result/Transcription",
            "/Data/result/transcription",
            "/Data/Result",
            "/Data/result",
            "/Result/TranscriptionUrl",
            "/result/transcriptionUrl",
            "/Result/Transcription",
            "/result/transcription",
            "/Data/Result/TranscriptionResult",
            "/Data/result/transcriptionResult",
            "/Data/TranscriptionResult",
            "/TranscriptionResult",
            "/Result",
            "/result",
        ],
    )
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

fn resolve_segment_file_urls(
    segment_paths: &[String],
    config: &AliyunTingwuConfig,
    session_id: &str,
    progress_callback: &dyn Fn(&str),
) -> Result<Vec<String>, String> {
    let mut resolved = vec![String::new(); segment_paths.len()];
    let mut pending_local_paths = vec![];
    let mut pending_local_indexes = vec![];

    for (index, segment_path) in segment_paths.iter().enumerate() {
        if is_http_url(segment_path) {
            resolved[index] = segment_path.clone();
            continue;
        }

        if config.oss.is_some() {
            pending_local_paths.push(segment_path.clone());
            pending_local_indexes.push(index);
            continue;
        }

        resolved[index] = resolve_file_url(segment_path, config.file_url_prefix.as_deref())
            .map_err(|error| {
                format!("segment {index} cannot be used for aliyun tingwu: {error}")
            })?;
    }

    if !pending_local_paths.is_empty() {
        let oss = config.oss.as_ref().ok_or_else(|| {
            "aliyun tingwu requires public FileUrl: configure current OSS for auto upload or set aliyunFileUrlPrefix".to_string()
        })?;
        progress_callback("Uploading to OSS...");
        let signed_urls = upload_segments_and_sign_urls(&pending_local_paths, session_id, oss)?;
        if signed_urls.len() != pending_local_indexes.len() {
            return Err("aliyun tingwu failed to map signed OSS urls to segment list".to_string());
        }
        for (index, url) in pending_local_indexes
            .into_iter()
            .zip(signed_urls.into_iter())
        {
            resolved[index] = url;
        }
    }

    if resolved.iter().any(|item| item.trim().is_empty()) {
        return Err("aliyun tingwu segment file url resolution incomplete".to_string());
    }

    Ok(resolved)
}

fn is_http_url(value: &str) -> bool {
    value.starts_with("http://") || value.starts_with("https://")
}

fn resolve_file_url(segment_path: &str, file_url_prefix: Option<&str>) -> Result<String, String> {
    if is_http_url(segment_path) {
        return Ok(segment_path.to_string());
    }

    let prefix = file_url_prefix
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            "aliyun tingwu requires public FileUrl; set aliyunFileUrlPrefix or provide URL segment path"
                .to_string()
        })?;

    let file_name = Path::new(segment_path)
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| format!("invalid segment file path: {segment_path}"))?;
    Ok(format!("{}/{}", prefix.trim_end_matches('/'), file_name))
}

fn build_canonicalized_headers(headers: &[(String, String)]) -> String {
    let mut entries = headers.to_vec();
    entries.sort_by(|left, right| left.0.cmp(&right.0));
    entries
        .iter()
        .map(|(key, value)| format!("{}:{}\n", key.to_lowercase(), value.trim()))
        .collect::<String>()
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
    if let Some(text) = extract_tingwu_paragraph_text(payload) {
        return Some(text);
    }

    if let Some(text) = extract_string(
        payload,
        &[
            "/text",
            "/Text",
            "/result/text",
            "/result/Text",
            "/output/text",
            "/Data/Text",
            "/Data/Result/Text",
        ],
    ) {
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

fn extract_tingwu_paragraph_text(payload: &Value) -> Option<String> {
    let paragraphs = [
        "/Transcription/Paragraphs",
        "/transcription/paragraphs",
        "/Data/Transcription/Paragraphs",
        "/Data/transcription/paragraphs",
        "/data/Transcription/Paragraphs",
        "/data/transcription/paragraphs",
        "/Paragraphs",
        "/paragraphs",
    ]
    .iter()
    .find_map(|pointer| payload.pointer(pointer).and_then(Value::as_array))?;

    let mut lines: Vec<(Option<String>, String)> = Vec::new();
    for paragraph in paragraphs {
        let Some(text) = extract_paragraph_text(paragraph) else {
            continue;
        };
        let speaker = extract_speaker_id(paragraph);

        if let Some((last_speaker, last_text)) = lines.last_mut() {
            if *last_speaker == speaker {
                last_text.push('\n');
                last_text.push_str(&text);
                continue;
            }
        }

        lines.push((speaker, text));
    }

    let merged = lines
        .into_iter()
        .map(|(speaker, text)| match speaker {
            Some(id) => format!("Speaker {id}: {text}"),
            None => text,
        })
        .collect::<Vec<_>>()
        .join("\n");

    if merged.trim().is_empty() {
        None
    } else {
        Some(merged)
    }
}

fn extract_paragraph_text(paragraph: &Value) -> Option<String> {
    if let Some(text) = extract_string(paragraph, &["/Text", "/text"]) {
        return Some(text);
    }

    let words = paragraph
        .pointer("/Words")
        .or_else(|| paragraph.pointer("/words"))
        .and_then(Value::as_array)?;

    let text = words
        .iter()
        .filter_map(|item| {
            item.pointer("/Text")
                .or_else(|| item.pointer("/text"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
        })
        .collect::<Vec<_>>()
        .join("");

    if text.trim().is_empty() {
        None
    } else {
        Some(text)
    }
}

fn extract_speaker_id(paragraph: &Value) -> Option<String> {
    paragraph
        .pointer("/SpeakerId")
        .or_else(|| paragraph.pointer("/speakerId"))
        .and_then(|value| {
            value
                .as_str()
                .map(str::trim)
                .filter(|text| !text.is_empty())
                .map(str::to_string)
                .or_else(|| value.as_i64().map(|number| number.to_string()))
                .or_else(|| value.as_u64().map(|number| number.to_string()))
        })
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

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::extract_text_from_payload;

    #[test]
    fn extracts_tingwu_paragraphs_with_speaker_labels() {
        let payload = json!({
            "Transcription": {
                "Paragraphs": [
                    {
                        "SpeakerId": "1",
                        "Words": [{ "Text": "您好，" }, { "Text": "我是" }]
                    },
                    {
                        "SpeakerId": "1",
                        "Words": [{ "Text": "小李。" }]
                    },
                    {
                        "SpeakerId": "2",
                        "Words": [{ "Text": "你好。" }]
                    }
                ]
            }
        });

        let text = extract_text_from_payload(&payload).unwrap_or_default();
        assert_eq!(text, "Speaker 1: 您好，我是\n小李。\nSpeaker 2: 你好。");
    }

    #[test]
    fn extracts_tingwu_paragraphs_without_speaker_labels() {
        let payload = json!({
            "Transcription": {
                "Paragraphs": [
                    {
                        "Words": [{ "Text": "第一段" }]
                    },
                    {
                        "Words": [{ "Text": "第二段" }]
                    }
                ]
            }
        });

        let text = extract_text_from_payload(&payload).unwrap_or_default();
        assert_eq!(text, "第一段\n第二段");
    }
}
