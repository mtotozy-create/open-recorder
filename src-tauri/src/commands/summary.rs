use std::collections::HashSet;

use chrono::Utc;
use tauri::State;
use uuid::Uuid;

use crate::{
    models::{
        Job, JobEnqueueResponse, JobKind, JobStatus, PersonNameMapping, PromptTemplate,
        ProviderCapability, ProviderConfig, ProviderKind, SessionStatus, TranscriptSegment,
    },
    providers::bailian::{summarize_with_chat_compatible, ChatCompatibleSummaryConfig},
    state::AppState,
};

fn now_iso() -> String {
    Utc::now().to_rfc3339()
}

pub(crate) fn provider_supports_summary(provider: &ProviderConfig) -> bool {
    provider.enabled
        && provider
            .capabilities
            .iter()
            .any(|item| *item == ProviderCapability::Summary)
}

pub(crate) fn resolve_summary_config(
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
                api_key: Some(api_key),
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
                api_key: Some(api_key),
                model: openrouter.summary_model.clone(),
            })
        }
        ProviderKind::Ollama => {
            let ollama = provider
                .ollama
                .as_ref()
                .ok_or_else(|| format!("provider '{}' missing ollama config", provider.name))?;
            if ollama.base_url.trim().is_empty() {
                return Err(format!(
                    "provider '{}' requires base URL for summary",
                    provider.name
                ));
            }
            if ollama.summary_model.trim().is_empty() {
                return Err(format!(
                    "provider '{}' requires summary model",
                    provider.name
                ));
            }

            let mut endpoint = ollama.base_url.trim_end_matches('/').to_string();
            if !endpoint.ends_with("/chat/completions") {
                endpoint.push_str("/chat/completions");
            }

            Ok(ChatCompatibleSummaryConfig {
                provider_name: provider.name.clone(),
                endpoint,
                api_key: ollama.api_key.clone(),
                model: ollama.summary_model.clone(),
            })
        }
        ProviderKind::AliyunTingwu => Err(format!(
            "provider '{}' does not support summary",
            provider.name
        )),
        ProviderKind::LocalStt => Err(format!(
            "provider '{}' does not support summary",
            provider.name
        )),
    }
}

#[tauri::command]
pub fn summary_enqueue(
    session_id: String,
    template_id: Option<String>,
    provider_id: Option<String>,
    state: State<'_, AppState>,
) -> Result<JobEnqueueResponse, String> {
    let job_id = Uuid::new_v4().to_string();
    let now = now_iso();

    let (transcript, template, summary_config) = {
        let mut storage = state
            .storage
            .lock()
            .map_err(|_| "failed to acquire storage lock".to_string())?;
        let mut settings = storage.get_settings()?;
        let mut job = Job {
            id: job_id.clone(),
            session_id: session_id.clone(),
            kind: JobKind::Summary,
            status: JobStatus::Running,
            created_at: now.clone(),
            updated_at: now.clone(),
            error: None,
            progress_msg: None,
        };
        let mut session = storage
            .get_session(&session_id)?
            .ok_or_else(|| "session not found".to_string())?;

        settings.normalize();

        let template = resolve_template(
            &settings.templates,
            template_id
                .as_deref()
                .unwrap_or(settings.default_template_id.as_str()),
        )
        .cloned()
        .ok_or_else(|| "template not found".to_string())?;

        session.status = SessionStatus::Summarizing;
        session.updated_at = now_iso();
        let transcript =
            apply_person_name_mappings(&session.transcript, &settings.person_name_mappings);

        if transcript.is_empty() {
            session.status = SessionStatus::Failed;
            session.updated_at = now_iso();
            job.status = JobStatus::Failed;
            job.error = Some("transcript is empty; run transcription first".to_string());
            job.updated_at = now_iso();
            storage.save_session_and_job(&session, &job)?;
            return Err("transcript is empty; run transcription first".to_string());
        }

        let selected_provider_id = provider_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(settings.selected_summary_provider_id.as_str());

        let provider = settings
            .providers
            .iter()
            .find(|provider| provider.id == selected_provider_id)
            .ok_or_else(|| {
                format!(
                    "selected summary provider '{}' not found",
                    selected_provider_id
                )
            })?;

        if !provider_supports_summary(provider) {
            let error = format!(
                "selected summary provider '{}' is disabled or not summary-capable",
                provider.name
            );
            session.status = SessionStatus::Failed;
            session.updated_at = now_iso();
            job.status = JobStatus::Failed;
            job.error = Some(error.clone());
            job.updated_at = now_iso();
            storage.save_session_and_job(&session, &job)?;
            return Err(error);
        }

        let summary_config = match resolve_summary_config(provider) {
            Ok(config) => config,
            Err(error) => {
                session.status = SessionStatus::Failed;
                session.updated_at = now_iso();
                job.status = JobStatus::Failed;
                job.error = Some(error.clone());
                job.updated_at = now_iso();
                storage.save_session_and_job(&session, &job)?;
                return Err(error);
            }
        };

        storage.save_session_and_job(&session, &job)?;
        (transcript, template, summary_config)
    };

    let storage_arc = state.storage.clone();
    let job_id_clone = job_id.clone();
    let session_id_clone = session_id.clone();

    std::thread::spawn(move || {
        let update_progress = |msg: &str| {
            if let Ok(storage) = storage_arc.lock() {
                if let Ok(Some(mut job)) = storage.get_job(&job_id_clone) {
                    job.progress_msg = Some(msg.to_string());
                    job.updated_at = now_iso();
                    let _ = storage.upsert_job(&job);
                }
            }
        };

        update_progress("Generating summary...");

        let summary_result = summarize_with_chat_compatible(
            &transcript,
            &template.system_prompt,
            &template.user_prompt,
            &summary_config,
        );

        if let Ok(storage) = storage_arc.lock() {
            match summary_result {
                Ok(summary) => {
                    if let Ok(Some(mut session)) = storage.get_session(&session_id_clone) {
                        session.summary = Some(summary);
                        session.status = SessionStatus::Completed;
                        session.updated_at = now_iso();
                        let _ = storage.upsert_session(&session);
                    }

                    if let Ok(Some(mut job)) = storage.get_job(&job_id_clone) {
                        job.status = JobStatus::Completed;
                        job.error = None;
                        job.updated_at = now_iso();
                        let _ = storage.upsert_job(&job);
                    }
                }
                Err(error) => {
                    if let Ok(Some(mut session)) = storage.get_session(&session_id_clone) {
                        session.status = SessionStatus::Failed;
                        session.updated_at = now_iso();
                        let _ = storage.upsert_session(&session);
                    }

                    if let Ok(Some(mut job)) = storage.get_job(&job_id_clone) {
                        job.status = JobStatus::Failed;
                        job.error = Some(error);
                        job.updated_at = now_iso();
                        let _ = storage.upsert_job(&job);
                    }
                }
            }
        }
    });

    Ok(JobEnqueueResponse { job_id })
}

fn resolve_template<'a>(
    templates: &'a [PromptTemplate],
    template_id: &str,
) -> Option<&'a PromptTemplate> {
    templates.iter().find(|template| template.id == template_id)
}

pub(crate) fn apply_person_name_mappings(
    transcript: &[TranscriptSegment],
    mappings: &[PersonNameMapping],
) -> Vec<TranscriptSegment> {
    let rules = normalized_person_name_mapping_rules(mappings);
    if rules.is_empty() {
        return transcript.to_vec();
    }

    transcript
        .iter()
        .cloned()
        .map(|mut segment| {
            for (source_name, target_name) in &rules {
                segment.text = segment.text.replace(source_name, target_name);
            }
            segment
        })
        .collect()
}

fn normalized_person_name_mapping_rules(mappings: &[PersonNameMapping]) -> Vec<(String, String)> {
    let mut seen_sources = HashSet::new();
    let mut rules = Vec::with_capacity(mappings.len());

    for mapping in mappings {
        let source_name = mapping.source_name.trim();
        let target_name = mapping.target_name.trim();
        if source_name.is_empty() || target_name.is_empty() || source_name == target_name {
            continue;
        }
        if !seen_sources.insert(source_name.to_string()) {
            continue;
        }
        rules.push((source_name.to_string(), target_name.to_string()));
    }

    rules.sort_by(|(left, _), (right, _)| right.len().cmp(&left.len()));
    rules
}

#[cfg(test)]
mod tests {
    use super::apply_person_name_mappings;
    use crate::models::{PersonNameMapping, TranscriptSegment};

    fn mapping(id: &str, source_name: &str, target_name: &str) -> PersonNameMapping {
        PersonNameMapping {
            id: id.to_string(),
            source_name: source_name.to_string(),
            target_name: target_name.to_string(),
        }
    }

    fn segment(text: &str) -> TranscriptSegment {
        TranscriptSegment {
            start_ms: 0,
            end_ms: 1000,
            text: text.to_string(),
            translation_text: Some("translation stays original".to_string()),
            translation_target_language: Some("en".to_string()),
            confidence: Some(0.9),
            speaker_id: Some("speaker-1".to_string()),
            speaker_label: Some("Speaker 1".to_string()),
        }
    }

    #[test]
    fn applies_exact_replacements_across_segments() {
        let transcript = vec![segment("任希介绍方案"), segment("请任希跟进")];
        let corrected = apply_person_name_mappings(&transcript, &[mapping("one", "任希", "任曦")]);

        assert_eq!(corrected[0].text, "任曦介绍方案");
        assert_eq!(corrected[1].text, "请任曦跟进");
    }

    #[test]
    fn ignores_invalid_duplicate_and_noop_mappings() {
        let transcript = vec![segment("Alice and Bob")];
        let corrected = apply_person_name_mappings(
            &transcript,
            &[
                mapping("empty-source", " ", "Nobody"),
                mapping("empty-target", "Bob", " "),
                mapping("noop", "Alice", "Alice"),
                mapping("first", "Alice", "Alicia"),
                mapping("duplicate", "Alice", "Ally"),
                mapping("shared-target", "Bob", "Alicia"),
            ],
        );

        assert_eq!(corrected[0].text, "Alicia and Alicia");
    }

    #[test]
    fn applies_longer_source_names_before_shorter_sources() {
        let transcript = vec![segment("王小明和王都参加")];
        let corrected = apply_person_name_mappings(
            &transcript,
            &[
                mapping("short", "王", "Wang"),
                mapping("long", "王小明", "Alex Wang"),
            ],
        );

        assert_eq!(corrected[0].text, "Alex Wang和Wang都参加");
    }

    #[test]
    fn does_not_mutate_original_transcript_or_non_text_fields() {
        let transcript = vec![segment("Ren Shee joined")];
        let corrected =
            apply_person_name_mappings(&transcript, &[mapping("one", "Ren Shee", "Renxi")]);

        assert_eq!(transcript[0].text, "Ren Shee joined");
        assert_eq!(corrected[0].text, "Renxi joined");
        assert_eq!(
            corrected[0].translation_text,
            transcript[0].translation_text
        );
        assert_eq!(corrected[0].speaker_label, transcript[0].speaker_label);
    }
}
