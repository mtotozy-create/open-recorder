use chrono::Utc;
use tauri::State;
use uuid::Uuid;

use crate::{
    models::{
        Job, JobEnqueueResponse, JobKind, JobStatus, PromptTemplate, ProviderCapability,
        ProviderConfig, ProviderKind, SessionStatus,
    },
    providers::bailian::{summarize_with_chat_compatible, ChatCompatibleSummaryConfig},
    state::AppState,
};

fn now_iso() -> String {
    Utc::now().to_rfc3339()
}

fn provider_supports_summary(provider: &ProviderConfig) -> bool {
    provider.enabled
        && provider
            .capabilities
            .iter()
            .any(|item| *item == ProviderCapability::Summary)
}

fn resolve_summary_config(
    provider: &ProviderConfig,
) -> Result<ChatCompatibleSummaryConfig, String> {
    match provider.kind {
        ProviderKind::Bailian => {
            let bailian = provider
                .bailian
                .as_ref()
                .ok_or_else(|| format!("provider '{}' missing bailian config", provider.name))?;
            let api_key = bailian.api_key.clone().unwrap_or_default();
            if api_key.trim().is_empty() {
                return Err(format!(
                    "provider '{}' requires API key for summary",
                    provider.name
                ));
            }
            if bailian.base_url.trim().is_empty() {
                return Err(format!(
                    "provider '{}' requires base URL for summary",
                    provider.name
                ));
            }
            if bailian.summary_model.trim().is_empty() {
                return Err(format!(
                    "provider '{}' requires summary model",
                    provider.name
                ));
            }

            Ok(ChatCompatibleSummaryConfig {
                provider_name: provider.name.clone(),
                endpoint: crate::providers::bailian::build_endpoint(
                    &bailian.base_url,
                    crate::providers::bailian::BAILIAN_COMPATIBLE_CHAT_PATH,
                ),
                api_key,
                model: bailian.summary_model.clone(),
            })
        }
        ProviderKind::Openrouter => {
            let openrouter = provider
                .openrouter
                .as_ref()
                .ok_or_else(|| format!("provider '{}' missing openrouter config", provider.name))?;
            let api_key = openrouter.api_key.clone().unwrap_or_default();
            if api_key.trim().is_empty() {
                return Err(format!(
                    "provider '{}' requires API key for summary",
                    provider.name
                ));
            }
            if openrouter.base_url.trim().is_empty() {
                return Err(format!(
                    "provider '{}' requires base URL for summary",
                    provider.name
                ));
            }
            if openrouter.summary_model.trim().is_empty() {
                return Err(format!(
                    "provider '{}' requires summary model",
                    provider.name
                ));
            }

            let mut endpoint = openrouter.base_url.trim_end_matches('/').to_string();
            if !endpoint.ends_with("/chat/completions") {
                endpoint.push_str("/chat/completions");
            }

            Ok(ChatCompatibleSummaryConfig {
                provider_name: provider.name.clone(),
                endpoint,
                api_key,
                model: openrouter.summary_model.clone(),
            })
        }
        ProviderKind::AliyunTingwu => Err(format!(
            "provider '{}' does not support summary",
            provider.name
        )),
    }
}

#[tauri::command]
pub fn summary_enqueue(
    session_id: String,
    template_id: Option<String>,
    state: State<'_, AppState>,
) -> Result<JobEnqueueResponse, String> {
    let job_id = Uuid::new_v4().to_string();
    let now = now_iso();

    let (transcript, template, summary_config) = {
        let mut storage = state
            .storage
            .lock()
            .map_err(|_| "failed to acquire storage lock".to_string())?;

        storage.data.jobs.insert(
            job_id.clone(),
            Job {
                id: job_id.clone(),
                session_id: session_id.clone(),
                kind: JobKind::Summary,
                status: JobStatus::Running,
                created_at: now.clone(),
                updated_at: now.clone(),
                error: None,
            },
        );

        storage.data.settings.normalize();
        let settings = storage.data.settings.clone();

        let template = resolve_template(
            &settings.templates,
            template_id
                .as_deref()
                .unwrap_or(settings.default_template_id.as_str()),
        )
        .cloned()
        .ok_or_else(|| "template not found".to_string())?;

        let transcript = {
            let session = storage
                .data
                .sessions
                .get_mut(&session_id)
                .ok_or_else(|| "session not found".to_string())?;
            session.status = SessionStatus::Summarizing;
            session.updated_at = now_iso();
            session.transcript.clone()
        };

        if transcript.is_empty() {
            if let Some(session) = storage.data.sessions.get_mut(&session_id) {
                session.status = SessionStatus::Failed;
                session.updated_at = now_iso();
            }
            if let Some(job) = storage.data.jobs.get_mut(&job_id) {
                job.status = JobStatus::Failed;
                job.error = Some("transcript is empty; run transcription first".to_string());
                job.updated_at = now_iso();
            }
            storage.save()?;
            return Err("transcript is empty; run transcription first".to_string());
        }

        let provider = settings
            .providers
            .iter()
            .find(|provider| provider.id == settings.selected_summary_provider_id)
            .ok_or_else(|| {
                format!(
                    "selected summary provider '{}' not found",
                    settings.selected_summary_provider_id
                )
            })?;

        if !provider_supports_summary(provider) {
            let error = format!(
                "selected summary provider '{}' is disabled or not summary-capable",
                provider.name
            );
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

        let summary_config = match resolve_summary_config(provider) {
            Ok(config) => config,
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
        };

        storage.save()?;
        (transcript, template, summary_config)
    };

    let summary_result = summarize_with_chat_compatible(
        &transcript,
        &template.system_prompt,
        &template.user_prompt,
        &summary_config,
    );

    let mut storage = state
        .storage
        .lock()
        .map_err(|_| "failed to acquire storage lock".to_string())?;

    match summary_result {
        Ok(summary) => {
            if let Some(session) = storage.data.sessions.get_mut(&session_id) {
                session.summary = Some(summary);
                session.status = SessionStatus::Completed;
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

fn resolve_template<'a>(
    templates: &'a [PromptTemplate],
    template_id: &str,
) -> Option<&'a PromptTemplate> {
    templates.iter().find(|template| template.id == template_id)
}
