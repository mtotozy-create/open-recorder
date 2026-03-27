use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Duration, Utc};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::State;
use uuid::Uuid;

use crate::{
    models::{
        InsightAction, InsightCacheEntry, InsightPerson, InsightResult, InsightSuggestion,
        InsightSuggestionPriority, InsightTopic, InsightTopicStatus, Job, JobEnqueueResponse,
        JobKind, JobStatus, ProviderConfig, ProviderKind, Session,
    },
    providers::bailian::ChatCompatibleSummaryConfig,
    state::AppState,
};

use super::summary::{provider_supports_summary, resolve_summary_config};

const DISCOVER_JOB_SESSION_ID: &str = "__discover__";
const INSIGHT_PROMPT_VERSION: &str = "discover-v2";
const MAX_SELECTED_SESSION_IDS: usize = 20;

fn now_iso() -> String {
    Utc::now().to_rfc3339()
}

#[derive(Debug, Clone)]
struct InsightSourceSession {
    id: String,
    name: String,
    created_at: String,
    updated_at: String,
    raw_markdown: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InsightExtractionPayload {
    people: Vec<InsightPerson>,
    topics: Vec<InsightTopic>,
    upcoming_actions: Vec<InsightAction>,
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
pub struct InsightQueryRequest {
    pub selection_mode: Option<String>,
    pub time_range: Option<String>,
    pub session_ids: Option<Vec<String>>,
    pub keyword: Option<String>,
    pub include_suggestions: Option<bool>,
}

#[derive(Debug, Clone)]
enum InsightSelection {
    TimeRange { time_range: String },
    Sessions { session_ids: Vec<String> },
}

#[derive(Debug, Clone)]
struct NormalizedInsightQuery {
    selection: InsightSelection,
    selection_key: String,
    result_scope: String,
    keyword: Option<String>,
    include_suggestions: bool,
    empty_source_error: String,
}

#[tauri::command]
pub fn insight_get_cached(
    request: InsightQueryRequest,
    state: State<'_, AppState>,
) -> Result<Option<InsightResult>, String> {
    let storage = state
        .storage
        .lock()
        .map_err(|_| "failed to acquire storage lock".to_string())?;
    let settings = storage.get_settings()?;
    let normalized_query = normalize_insight_query(request)?;
    let provider = resolve_discover_provider(&settings)?;
    let summary_config = resolve_discover_config(provider)?;
    let sessions = collect_source_sessions(&storage.list_all_sessions()?, &normalized_query.selection)?;

    if sessions.is_empty() {
        return Ok(None);
    }

    let cache_key = build_cache_key(
        normalized_query.selection_key.as_str(),
        normalized_query.keyword.as_deref(),
        normalized_query.include_suggestions,
        provider.id.as_str(),
        summary_config.model.as_str(),
        build_session_fingerprint(&sessions).as_str(),
    );

    Ok(storage
        .get_insight_cache(&cache_key)?
        .map(|entry| entry.result.clone()))
}

#[tauri::command]
pub fn insight_enqueue(
    request: InsightQueryRequest,
    force_refresh: Option<bool>,
    state: State<'_, AppState>,
) -> Result<JobEnqueueResponse, String> {
    let job_id = Uuid::new_v4().to_string();
    let now = now_iso();
    let force_refresh = force_refresh.unwrap_or(false);

    let (
        storage_arc,
        job_id_clone,
        cache_key,
        keyword,
        sessions,
        provider_id,
        provider_model,
        result_scope,
        session_fingerprint,
        include_suggestions,
        summary_config,
    ) = {
        let storage = state
            .storage
            .lock()
            .map_err(|_| "failed to acquire storage lock".to_string())?;
        let settings = storage.get_settings()?;

        let normalized_query = normalize_insight_query(request)?;

        let provider = resolve_discover_provider(&settings)?;
        let provider_id = provider.id.clone();
        let summary_config = resolve_discover_config(provider)?;

        let sessions = collect_source_sessions(&storage.list_all_sessions()?, &normalized_query.selection)?;
        if sessions.is_empty() {
            return Err(normalized_query.empty_source_error);
        }

        let session_fingerprint = build_session_fingerprint(&sessions);
        let cache_key = build_cache_key(
            normalized_query.selection_key.as_str(),
            normalized_query.keyword.as_deref(),
            normalized_query.include_suggestions,
            provider_id.as_str(),
            summary_config.model.as_str(),
            session_fingerprint.as_str(),
        );

        if !force_refresh && storage.get_insight_cache(&cache_key)?.is_some() {
            storage.upsert_job(&Job {
                id: job_id.clone(),
                session_id: DISCOVER_JOB_SESSION_ID.to_string(),
                kind: JobKind::Insight,
                status: JobStatus::Completed,
                created_at: now.clone(),
                updated_at: now.clone(),
                error: None,
                progress_msg: Some("cache hit".to_string()),
            })?;
            return Ok(JobEnqueueResponse { job_id });
        }

        storage.upsert_job(&Job {
            id: job_id.clone(),
            session_id: DISCOVER_JOB_SESSION_ID.to_string(),
            kind: JobKind::Insight,
            status: JobStatus::Running,
            created_at: now.clone(),
            updated_at: now,
            error: None,
            progress_msg: Some("queued".to_string()),
        })?;

        (
            state.storage.clone(),
            job_id.clone(),
            cache_key,
            normalized_query.keyword,
            sessions,
            provider_id,
            summary_config.model.clone(),
            normalized_query.result_scope,
            session_fingerprint,
            normalized_query.include_suggestions,
            summary_config,
        )
    };

    std::thread::spawn(move || {
        let update_progress = |message: String| {
            if let Ok(storage) = storage_arc.lock() {
                if let Ok(Some(mut job)) = storage.get_job(&job_id_clone) {
                    job.progress_msg = Some(message);
                    job.updated_at = now_iso();
                    let _ = storage.upsert_job(&job);
                }
            }
        };

        let mut extracted = Vec::with_capacity(sessions.len());
        let valid_session_ids: HashSet<String> =
            sessions.iter().map(|session| session.id.clone()).collect();

        for (index, session) in sessions.iter().enumerate() {
            update_progress(format!(
                "analyzing session {}/{}",
                index + 1,
                sessions.len()
            ));

            let payload = match analyze_single_session(
                session,
                keyword.as_deref(),
                include_suggestions,
                &summary_config,
            ) {
                Ok(value) => value,
                Err(error) => {
                    if let Ok(storage) = storage_arc.lock() {
                        if let Ok(Some(mut job)) = storage.get_job(&job_id_clone) {
                            job.status = JobStatus::Failed;
                            job.error = Some(error);
                            job.updated_at = now_iso();
                            let _ = storage.upsert_job(&job);
                        }
                    }
                    return;
                }
            };

            extracted.push(normalize_extraction_payload(
                payload,
                session,
                &valid_session_ids,
            ));
        }

        update_progress("merging insights".to_string());
        let merged = merge_extractions(extracted, keyword.as_deref());
        let session_ids = sessions
            .iter()
            .map(|session| session.id.clone())
            .collect::<Vec<_>>();

        let insight_result = InsightResult {
            people: merged.people,
            topics: merged.topics,
            upcoming_actions: merged.upcoming_actions,
            generated_at: now_iso(),
            time_range_type: result_scope,
            session_ids,
        };

        if let Ok(mut storage) = storage_arc.lock() {
            let cache_entry = InsightCacheEntry {
                key: cache_key,
                result: insight_result,
                cached_at: now_iso(),
                session_fingerprint,
                provider_id,
                model: provider_model,
                prompt_version: INSIGHT_PROMPT_VERSION.to_string(),
                keyword,
            };
            if let Ok(Some(mut job)) = storage.get_job(&job_id_clone) {
                job.status = JobStatus::Completed;
                job.error = None;
                job.updated_at = now_iso();
                job.progress_msg = Some("done".to_string());
                let _ = storage.save_job_and_insight_cache(&job, &cache_entry);
            } else {
                let _ = storage.upsert_insight_cache(&cache_entry);
            }
        }
    });

    Ok(JobEnqueueResponse { job_id })
}

fn resolve_discover_provider<'a>(
    settings: &'a crate::models::Settings,
) -> Result<&'a crate::models::ProviderConfig, String> {
    let provider = settings
        .providers
        .iter()
        .find(|provider| provider.id == settings.selected_discover_provider_id)
        .ok_or_else(|| {
            format!(
                "selected discover provider '{}' not found",
                settings.selected_discover_provider_id
            )
        })?;

    if !provider_supports_summary(provider) {
        return Err(format!(
            "selected discover provider '{}' is disabled or not summary-capable",
            provider.name
        ));
    }

    Ok(provider)
}

fn resolve_discover_config(
    provider: &ProviderConfig,
) -> Result<ChatCompatibleSummaryConfig, String> {
    let mut config = resolve_summary_config(provider)?;
    if provider.kind == ProviderKind::Openrouter {
        if let Some(openrouter) = provider.openrouter.as_ref() {
            let discover_model = openrouter.discover_model.trim();
            if !discover_model.is_empty() {
                config.model = discover_model.to_string();
            }
        }
    }
    Ok(config)
}

fn normalize_time_range(raw: &str) -> Result<String, String> {
    let normalized = raw.trim().to_lowercase();
    if matches!(normalized.as_str(), "1d" | "2d" | "3d" | "1w" | "1m") {
        return Ok(normalized);
    }
    Err("invalid time range; expected one of: 1d, 2d, 3d, 1w, 1m".to_string())
}

fn normalize_keyword(keyword: Option<String>) -> Option<String> {
    keyword.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn normalize_insight_query(request: InsightQueryRequest) -> Result<NormalizedInsightQuery, String> {
    let normalized_mode = normalize_selection_mode(request.selection_mode.as_deref());
    let has_time_range = request
        .time_range
        .as_deref()
        .map(str::trim)
        .is_some_and(|value| !value.is_empty());
    let has_session_ids = request
        .session_ids
        .as_ref()
        .map(|values| values.iter().any(|value| !value.trim().is_empty()))
        .unwrap_or(false);

    let mode = normalized_mode.unwrap_or_else(|| {
        if has_session_ids {
            "sessions".to_string()
        } else {
            "timeRange".to_string()
        }
    });

    let keyword = normalize_keyword(request.keyword);
    let include_suggestions = request.include_suggestions.unwrap_or(false);

    match mode.as_str() {
        "timeRange" => {
            if has_session_ids {
                return Err("sessionIds must be empty when selectionMode is timeRange".to_string());
            }
            let raw_time_range = request.time_range.ok_or_else(|| {
                "timeRange is required when selectionMode is timeRange".to_string()
            })?;
            let time_range = normalize_time_range(raw_time_range.as_str())?;
            Ok(NormalizedInsightQuery {
                selection: InsightSelection::TimeRange {
                    time_range: time_range.clone(),
                },
                selection_key: format!("timeRange:{time_range}"),
                result_scope: time_range,
                keyword,
                include_suggestions,
                empty_source_error: "no sessions with summary found in selected time range"
                    .to_string(),
            })
        }
        "sessions" => {
            if has_time_range {
                return Err("timeRange must be empty when selectionMode is sessions".to_string());
            }
            let session_ids = normalize_session_ids(request.session_ids)?;
            if session_ids.is_empty() {
                return Err(
                    "at least one session id is required when selectionMode is sessions"
                        .to_string(),
                );
            }
            let mut sorted_ids = session_ids.clone();
            sorted_ids.sort();
            Ok(NormalizedInsightQuery {
                selection: InsightSelection::Sessions {
                    session_ids: session_ids.clone(),
                },
                selection_key: format!("sessions:{}", sorted_ids.join(",")),
                result_scope: "sessions".to_string(),
                keyword,
                include_suggestions,
                empty_source_error:
                    "no discoverable sessions with summary found in selected sessions".to_string(),
            })
        }
        _ => Err("selectionMode must be one of: timeRange, sessions".to_string()),
    }
}

fn normalize_selection_mode(raw: Option<&str>) -> Option<String> {
    let value = raw?.trim().to_lowercase();
    if value.is_empty() {
        return None;
    }
    if matches!(value.as_str(), "timerange" | "time_range" | "time-range") {
        return Some("timeRange".to_string());
    }
    if value == "sessions" {
        return Some("sessions".to_string());
    }
    Some(value)
}

fn normalize_session_ids(values: Option<Vec<String>>) -> Result<Vec<String>, String> {
    let mut seen = HashSet::new();
    let mut result = Vec::new();
    for value in values.unwrap_or_default() {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            continue;
        }
        if seen.insert(trimmed.to_string()) {
            result.push(trimmed.to_string());
        }
    }
    if result.len() > MAX_SELECTED_SESSION_IDS {
        return Err(format!(
            "sessionIds exceeds max limit {MAX_SELECTED_SESSION_IDS}"
        ));
    }
    Ok(result)
}

fn resolve_cutoff(time_range: &str) -> Result<DateTime<Utc>, String> {
    let now = Utc::now();
    let cutoff = match time_range {
        "1d" => now - Duration::days(1),
        "2d" => now - Duration::days(2),
        "3d" => now - Duration::days(3),
        "1w" => now - Duration::weeks(1),
        "1m" => now - Duration::days(30),
        _ => return Err("invalid time range".to_string()),
    };
    Ok(cutoff)
}

fn collect_source_sessions(
    sessions: &[Session],
    selection: &InsightSelection,
) -> Result<Vec<InsightSourceSession>, String> {
    match selection {
        InsightSelection::TimeRange { time_range } => {
            collect_source_sessions_by_time_range(sessions, time_range.as_str())
        }
        InsightSelection::Sessions { session_ids } => {
            collect_source_sessions_by_ids(sessions, session_ids)
        }
    }
}

fn collect_source_sessions_by_time_range(
    sessions: &[Session],
    time_range: &str,
) -> Result<Vec<InsightSourceSession>, String> {
    let cutoff = resolve_cutoff(time_range)?;

    let mut result = sessions
        .iter()
        .filter_map(|session| {
            let source = build_source_session(session)?;
            let updated_at = parse_utc(session.updated_at.as_str());
            if let Some(value) = updated_at {
                if value < cutoff {
                    return None;
                }
            }
            Some(source)
        })
        .collect::<Vec<_>>();

    result.sort_by(|a, b| a.created_at.cmp(&b.created_at));
    Ok(result)
}

fn collect_source_sessions_by_ids(
    sessions: &[Session],
    session_ids: &[String],
) -> Result<Vec<InsightSourceSession>, String> {
    let mut result = Vec::with_capacity(session_ids.len());
    for session_id in session_ids {
        let Some(session) = sessions.iter().find(|session| session.id == *session_id) else {
            continue;
        };
        if let Some(source) = build_source_session(session) {
            result.push(source);
        }
    }
    result.sort_by(|a, b| a.created_at.cmp(&b.created_at));
    Ok(result)
}

fn build_source_session(session: &Session) -> Option<InsightSourceSession> {
    if !session.discoverable {
        return None;
    }
    let summary = session.summary.as_ref()?;
    let raw_markdown = summary.raw_markdown.trim();
    if raw_markdown.is_empty() {
        return None;
    }

    Some(InsightSourceSession {
        id: session.id.clone(),
        name: session.name.clone().unwrap_or_else(|| {
            format!("Session {}", session.id.chars().take(8).collect::<String>())
        }),
        created_at: session.created_at.clone(),
        updated_at: session.updated_at.clone(),
        raw_markdown: summary.raw_markdown.clone(),
    })
}

fn build_session_fingerprint(sessions: &[InsightSourceSession]) -> String {
    let mut hasher = Sha256::new();
    for session in sessions {
        hasher.update(session.id.as_bytes());
        hasher.update(b"|");
        hasher.update(session.updated_at.as_bytes());
        hasher.update(b";");
    }
    hex::encode(hasher.finalize())
}

fn build_cache_key(
    selection_key: &str,
    keyword: Option<&str>,
    include_suggestions: bool,
    provider_id: &str,
    model: &str,
    session_fingerprint: &str,
) -> String {
    let keyword_part = keyword.unwrap_or("").to_lowercase();
    let raw = format!(
        "{selection_key}|{keyword_part}|{include_suggestions}|{provider_id}|{model}|{}|{session_fingerprint}",
        INSIGHT_PROMPT_VERSION
    );
    let mut hasher = Sha256::new();
    hasher.update(raw.as_bytes());
    hex::encode(hasher.finalize())
}

fn parse_utc(input: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(input)
        .ok()
        .map(|value| value.with_timezone(&Utc))
}

fn build_session_prompt(
    session: &InsightSourceSession,
    keyword: Option<&str>,
    include_suggestions: bool,
) -> String {
    let keyword_line = keyword
        .map(|value| format!("事项关键词: {value}"))
        .unwrap_or_else(|| "事项关键词: (无，提取全部)".to_string());

    if include_suggestions {
        return format!(
            "请基于以下单个会议纪要提取结构化信息。\n\
            你必须只输出 JSON，不要输出任何解释。\n\
            你必须保留并回填 sourceSessionId 为输入的 sessionId。\n\
            额外要求：请给人员和事项分别提供可执行、建设性、可落地的下一步行动建议。\n\
            建议必须具体，避免空话；优先给出 1-2 周内可执行的动作。\n\
            {keyword_line}\n\n\
            输入:\n\
            - sessionId: {session_id}\n\
            - sessionName: {session_name}\n\
            - sessionDate: {session_date}\n\n\
            纪要原文:\n\
            {raw_markdown}\n\n\
            输出 JSON 结构:\n\
            {{\n\
              \"people\": [{{\n\
                \"name\": \"\",\n\
                \"tasks\": [{{\n\
                  \"description\": \"\",\n\
                  \"status\": \"pending|in_progress|completed\",\n\
                  \"deadline\": \"\",\n\
                  \"sourceSessionId\": \"{session_id}\",\n\
                  \"sourceDate\": \"\"\n\
                }}],\n\
                \"decisions\": [\"\"],\n\
                \"risks\": [\"\"],\n\
                \"suggestions\": [{{\n\
                  \"title\": \"\",\n\
                  \"rationale\": \"\",\n\
                  \"priority\": \"high|medium|low\",\n\
                  \"ownerHint\": \"\",\n\
                  \"sourceSessionIds\": [\"{session_id}\"]\n\
                }}]\n\
              }}],\n\
              \"topics\": [{{\n\
                \"name\": \"\",\n\
                \"progress\": [{{\n\
                  \"date\": \"\",\n\
                  \"description\": \"\",\n\
                  \"sourceSessionId\": \"{session_id}\"\n\
                }}],\n\
                \"status\": \"active|completed|blocked\",\n\
                \"relatedPeople\": [\"\"],\n\
                \"suggestions\": [{{\n\
                  \"title\": \"\",\n\
                  \"rationale\": \"\",\n\
                  \"priority\": \"high|medium|low\",\n\
                  \"ownerHint\": \"\",\n\
                  \"sourceSessionIds\": [\"{session_id}\"]\n\
                }}]\n\
              }}],\n\
              \"upcomingActions\": [{{\n\
                \"description\": \"\",\n\
                \"assignee\": \"\",\n\
                \"deadline\": \"\",\n\
                \"sourceSessionId\": \"{session_id}\",\n\
                \"sourceDate\": \"\"\n\
              }}]\n\
            }}",
            session_id = session.id,
            session_name = session.name,
            session_date = session.created_at,
            raw_markdown = session.raw_markdown
        );
    }

    format!(
        "请基于以下单个会议纪要提取结构化信息。\n\
        你必须只输出 JSON，不要输出任何解释。\n\
        你必须保留并回填 sourceSessionId 为输入的 sessionId。\n\
        {keyword_line}\n\n\
        输入:\n\
        - sessionId: {session_id}\n\
        - sessionName: {session_name}\n\
        - sessionDate: {session_date}\n\n\
        纪要原文:\n\
        {raw_markdown}\n\n\
        输出 JSON 结构:\n\
        {{\n\
          \"people\": [{{\n\
            \"name\": \"\",\n\
            \"tasks\": [{{\n\
              \"description\": \"\",\n\
              \"status\": \"pending|in_progress|completed\",\n\
              \"deadline\": \"\",\n\
              \"sourceSessionId\": \"{session_id}\",\n\
              \"sourceDate\": \"\"\n\
            }}],\n\
            \"decisions\": [\"\"],\n\
            \"risks\": [\"\"]\n\
          }}],\n\
          \"topics\": [{{\n\
            \"name\": \"\",\n\
            \"progress\": [{{\n\
              \"date\": \"\",\n\
              \"description\": \"\",\n\
              \"sourceSessionId\": \"{session_id}\"\n\
            }}],\n\
            \"status\": \"active|completed|blocked\",\n\
            \"relatedPeople\": [\"\"]\n\
          }}],\n\
          \"upcomingActions\": [{{\n\
            \"description\": \"\",\n\
            \"assignee\": \"\",\n\
            \"deadline\": \"\",\n\
            \"sourceSessionId\": \"{session_id}\",\n\
            \"sourceDate\": \"\"\n\
          }}]\n\
        }}",
        session_id = session.id,
        session_name = session.name,
        session_date = session.created_at,
        raw_markdown = session.raw_markdown
    )
}

fn analyze_single_session(
    session: &InsightSourceSession,
    keyword: Option<&str>,
    include_suggestions: bool,
    config: &ChatCompatibleSummaryConfig,
) -> Result<InsightExtractionPayload, String> {
    const SYSTEM_PROMPT: &str = "你是一个会议纪要分析助手。请严格输出 JSON。";

    let base_prompt = build_session_prompt(session, keyword, include_suggestions);
    let retry_hint = "\n\n上一次输出不符合要求。请只输出 JSON 对象本体，不要使用 markdown 代码块，不要附加说明。";
    let mut last_error = String::new();

    for attempt in 0..2 {
        let user_prompt = if attempt == 0 {
            base_prompt.clone()
        } else {
            format!("{base_prompt}{retry_hint}")
        };

        let raw = match invoke_chat_completion(config, SYSTEM_PROMPT, user_prompt.as_str()) {
            Ok(value) => value,
            Err(error) => {
                last_error = error;
                continue;
            }
        };

        let parsed = parse_json_payload::<InsightExtractionPayload>(raw.as_str());
        match parsed {
            Ok(value) => return Ok(value),
            Err(error) => {
                last_error = error;
            }
        }
    }

    Err(format!(
        "failed to parse insight extraction for session '{}' after retry: {last_error}",
        session.id
    ))
}

fn invoke_chat_completion(
    config: &ChatCompatibleSummaryConfig,
    system_prompt: &str,
    user_prompt: &str,
) -> Result<String, String> {
    let request = ChatCompletionsRequest {
        model: config.model.clone(),
        messages: vec![
            ChatMessage {
                role: "system".to_string(),
                content: system_prompt.to_string(),
            },
            ChatMessage {
                role: "user".to_string(),
                content: user_prompt.to_string(),
            },
        ],
        temperature: 0.1,
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

    Ok(raw_content)
}

fn parse_json_payload<T: for<'de> Deserialize<'de>>(raw: &str) -> Result<T, String> {
    let text = extract_json(raw);
    serde_json::from_str::<T>(text)
        .map_err(|error| format!("invalid JSON payload: {error}; raw={raw}"))
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

fn normalize_extraction_payload(
    mut payload: InsightExtractionPayload,
    session: &InsightSourceSession,
    valid_session_ids: &HashSet<String>,
) -> InsightExtractionPayload {
    payload.people = payload
        .people
        .into_iter()
        .filter_map(|mut person| {
            person.name = person.name.trim().to_string();
            if person.name.is_empty() {
                return None;
            }

            person.tasks = person
                .tasks
                .into_iter()
                .filter_map(|mut task| {
                    task.description = task.description.trim().to_string();
                    if task.description.is_empty() {
                        return None;
                    }
                    if !valid_session_ids.contains(&task.source_session_id) {
                        task.source_session_id = session.id.clone();
                    }
                    if task.source_session_id != session.id {
                        task.source_session_id = session.id.clone();
                    }
                    if task.source_date.trim().is_empty() {
                        task.source_date = session.created_at.clone();
                    }
                    task.deadline = task
                        .deadline
                        .map(|value| value.trim().to_string())
                        .filter(|value| !value.is_empty());
                    Some(task)
                })
                .collect();

            person.decisions = dedup_strings(person.decisions);
            person.risks = dedup_strings(person.risks);
            person.suggestions =
                normalize_suggestions(person.suggestions, session, valid_session_ids);
            Some(person)
        })
        .collect();

    payload.topics = payload
        .topics
        .into_iter()
        .filter_map(|mut topic| {
            topic.name = topic.name.trim().to_string();
            if topic.name.is_empty() {
                return None;
            }
            topic.progress = topic
                .progress
                .into_iter()
                .filter_map(|mut progress| {
                    progress.description = progress.description.trim().to_string();
                    if progress.description.is_empty() {
                        return None;
                    }
                    if !valid_session_ids.contains(&progress.source_session_id) {
                        progress.source_session_id = session.id.clone();
                    }
                    if progress.source_session_id != session.id {
                        progress.source_session_id = session.id.clone();
                    }
                    if progress.date.trim().is_empty() {
                        progress.date = session.created_at.clone();
                    }
                    Some(progress)
                })
                .collect();
            topic.related_people = dedup_strings(topic.related_people);
            topic.suggestions =
                normalize_suggestions(topic.suggestions, session, valid_session_ids);
            Some(topic)
        })
        .collect();

    payload.upcoming_actions = payload
        .upcoming_actions
        .into_iter()
        .filter_map(|mut action| {
            action.description = action.description.trim().to_string();
            if action.description.is_empty() {
                return None;
            }
            if !valid_session_ids.contains(&action.source_session_id) {
                action.source_session_id = session.id.clone();
            }
            if action.source_session_id != session.id {
                action.source_session_id = session.id.clone();
            }
            if action.source_date.trim().is_empty() {
                action.source_date = session.created_at.clone();
            }
            action.deadline = action
                .deadline
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty());
            action.assignee = action
                .assignee
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty());
            Some(action)
        })
        .collect();

    payload
}

fn dedup_strings(input: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut result = Vec::with_capacity(input.len());
    for value in input {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            continue;
        }
        let key = trimmed.to_lowercase();
        if seen.insert(key) {
            result.push(trimmed.to_string());
        }
    }
    result
}

fn normalize_suggestions(
    input: Vec<InsightSuggestion>,
    session: &InsightSourceSession,
    valid_session_ids: &HashSet<String>,
) -> Vec<InsightSuggestion> {
    input
        .into_iter()
        .filter_map(|mut suggestion| {
            suggestion.title = suggestion.title.trim().to_string();
            suggestion.rationale = suggestion.rationale.trim().to_string();
            if suggestion.title.is_empty() || suggestion.rationale.is_empty() {
                return None;
            }
            suggestion.owner_hint = suggestion
                .owner_hint
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty());
            suggestion.source_session_ids = suggestion
                .source_session_ids
                .into_iter()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
                .filter(|value| valid_session_ids.contains(value))
                .collect();
            if suggestion.source_session_ids.is_empty()
                || suggestion
                    .source_session_ids
                    .iter()
                    .any(|value| value != &session.id)
            {
                suggestion.source_session_ids = vec![session.id.clone()];
            }
            Some(suggestion)
        })
        .collect()
}

fn merge_extractions(
    payloads: Vec<InsightExtractionPayload>,
    keyword: Option<&str>,
) -> InsightExtractionPayload {
    let mut people_map: HashMap<String, InsightPerson> = HashMap::new();
    let mut topics_map: HashMap<String, InsightTopic> = HashMap::new();
    let mut actions = Vec::new();

    for payload in payloads {
        for person in payload.people {
            let key = person.name.trim().to_lowercase();
            let entry = people_map.entry(key).or_insert_with(|| InsightPerson {
                name: person.name.clone(),
                tasks: vec![],
                decisions: vec![],
                risks: vec![],
                suggestions: vec![],
            });

            if entry.name.trim().is_empty() {
                entry.name = person.name.clone();
            }
            entry.tasks.extend(person.tasks);
            entry.decisions.extend(person.decisions);
            entry.risks.extend(person.risks);
            entry.suggestions.extend(person.suggestions);
            dedup_person(entry);
        }

        for topic in payload.topics {
            let key = topic.name.trim().to_lowercase();
            let entry = topics_map.entry(key).or_insert_with(|| InsightTopic {
                name: topic.name.clone(),
                progress: vec![],
                status: topic.status.clone(),
                related_people: vec![],
                suggestions: vec![],
            });

            if entry.name.trim().is_empty() {
                entry.name = topic.name.clone();
            }
            entry.progress.extend(topic.progress);
            entry.related_people.extend(topic.related_people);
            entry.suggestions.extend(topic.suggestions);
            entry.status = merge_topic_status(&entry.status, &topic.status);
            dedup_topic(entry);
        }

        actions.extend(payload.upcoming_actions);
    }

    dedup_actions(&mut actions);

    let mut people = people_map.into_values().collect::<Vec<_>>();
    let mut topics = topics_map.into_values().collect::<Vec<_>>();

    if let Some(value) = keyword {
        let normalized = value.trim().to_lowercase();
        if !normalized.is_empty() {
            people = people
                .into_iter()
                .filter_map(|mut person| {
                    person.tasks = person
                        .tasks
                        .into_iter()
                        .filter(|task| task.description.to_lowercase().contains(&normalized))
                        .collect();
                    person.decisions = person
                        .decisions
                        .into_iter()
                        .filter(|item| item.to_lowercase().contains(&normalized))
                        .collect();
                    person.risks = person
                        .risks
                        .into_iter()
                        .filter(|item| item.to_lowercase().contains(&normalized))
                        .collect();
                    person.suggestions = person
                        .suggestions
                        .into_iter()
                        .filter(|item| {
                            item.title.to_lowercase().contains(&normalized)
                                || item.rationale.to_lowercase().contains(&normalized)
                                || item
                                    .owner_hint
                                    .as_ref()
                                    .map(|value| value.to_lowercase().contains(&normalized))
                                    .unwrap_or(false)
                        })
                        .collect();

                    if person.name.to_lowercase().contains(&normalized)
                        || !person.tasks.is_empty()
                        || !person.decisions.is_empty()
                        || !person.risks.is_empty()
                        || !person.suggestions.is_empty()
                    {
                        Some(person)
                    } else {
                        None
                    }
                })
                .collect();

            topics = topics
                .into_iter()
                .filter_map(|mut topic| {
                    topic.progress = topic
                        .progress
                        .into_iter()
                        .filter(|item| item.description.to_lowercase().contains(&normalized))
                        .collect();
                    topic.related_people = topic
                        .related_people
                        .into_iter()
                        .filter(|item| item.to_lowercase().contains(&normalized))
                        .collect();
                    topic.suggestions = topic
                        .suggestions
                        .into_iter()
                        .filter(|item| {
                            item.title.to_lowercase().contains(&normalized)
                                || item.rationale.to_lowercase().contains(&normalized)
                                || item
                                    .owner_hint
                                    .as_ref()
                                    .map(|value| value.to_lowercase().contains(&normalized))
                                    .unwrap_or(false)
                        })
                        .collect();

                    if topic.name.to_lowercase().contains(&normalized)
                        || !topic.progress.is_empty()
                        || !topic.related_people.is_empty()
                        || !topic.suggestions.is_empty()
                    {
                        Some(topic)
                    } else {
                        None
                    }
                })
                .collect();

            actions = actions
                .into_iter()
                .filter(|item| {
                    item.description.to_lowercase().contains(&normalized)
                        || item
                            .assignee
                            .as_ref()
                            .map(|value| value.to_lowercase().contains(&normalized))
                            .unwrap_or(false)
                })
                .collect();
        }
    }

    people.sort_by(|a, b| {
        b.tasks
            .len()
            .cmp(&a.tasks.len())
            .then_with(|| a.name.cmp(&b.name))
    });

    topics.sort_by(|a, b| a.name.cmp(&b.name));

    actions.sort_by(|a, b| {
        compare_optional_date(a.deadline.as_deref(), b.deadline.as_deref()).then_with(|| {
            compare_optional_date(Some(a.source_date.as_str()), Some(b.source_date.as_str()))
        })
    });

    InsightExtractionPayload {
        people,
        topics,
        upcoming_actions: actions,
    }
}

fn compare_optional_date(a: Option<&str>, b: Option<&str>) -> std::cmp::Ordering {
    let a_key = a
        .and_then(parse_utc)
        .map(|value| value.timestamp())
        .unwrap_or(i64::MAX);
    let b_key = b
        .and_then(parse_utc)
        .map(|value| value.timestamp())
        .unwrap_or(i64::MAX);
    a_key.cmp(&b_key)
}

fn dedup_person(person: &mut InsightPerson) {
    let mut task_seen = HashSet::new();
    person.tasks.retain(|task| {
        let key = format!(
            "{}|{}|{}|{:?}",
            task.description.trim().to_lowercase(),
            task.source_session_id,
            task.source_date,
            task.status
        );
        task_seen.insert(key)
    });
    person.decisions = dedup_strings(std::mem::take(&mut person.decisions));
    person.risks = dedup_strings(std::mem::take(&mut person.risks));
    dedup_suggestions(&mut person.suggestions);
}

fn dedup_topic(topic: &mut InsightTopic) {
    let mut progress_seen = HashSet::new();
    topic.progress.retain(|item| {
        let key = format!(
            "{}|{}|{}",
            item.description.trim().to_lowercase(),
            item.source_session_id,
            item.date
        );
        progress_seen.insert(key)
    });
    topic.related_people = dedup_strings(std::mem::take(&mut topic.related_people));
    dedup_suggestions(&mut topic.suggestions);
}

fn dedup_actions(actions: &mut Vec<InsightAction>) {
    let mut seen = HashSet::new();
    actions.retain(|action| {
        let key = format!(
            "{}|{}|{}|{}",
            action.description.trim().to_lowercase(),
            action.assignee.as_deref().unwrap_or(""),
            action.source_session_id,
            action.source_date
        );
        seen.insert(key)
    });
}

fn suggestion_priority_key(priority: &InsightSuggestionPriority) -> &'static str {
    match priority {
        InsightSuggestionPriority::High => "high",
        InsightSuggestionPriority::Medium => "medium",
        InsightSuggestionPriority::Low => "low",
    }
}

fn dedup_suggestions(suggestions: &mut Vec<InsightSuggestion>) {
    let mut seen = HashSet::new();
    let mut deduped = Vec::with_capacity(suggestions.len());

    for mut suggestion in std::mem::take(suggestions) {
        let mut source_ids = suggestion
            .source_session_ids
            .into_iter()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .collect::<Vec<_>>();
        source_ids.sort();
        source_ids.dedup();
        if source_ids.is_empty() {
            continue;
        }
        suggestion.source_session_ids = source_ids;

        let key = format!(
            "{}|{}|{}",
            suggestion.title.trim().to_lowercase(),
            suggestion_priority_key(&suggestion.priority),
            suggestion.source_session_ids.join(",")
        );
        if seen.insert(key) {
            deduped.push(suggestion);
        }
    }

    *suggestions = deduped;
}

fn merge_topic_status(
    current: &InsightTopicStatus,
    next: &InsightTopicStatus,
) -> InsightTopicStatus {
    fn score(status: &InsightTopicStatus) -> u8 {
        match status {
            InsightTopicStatus::Blocked => 3,
            InsightTopicStatus::Active => 2,
            InsightTopicStatus::Completed => 1,
        }
    }

    if score(next) >= score(current) {
        next.clone()
    } else {
        current.clone()
    }
}
