use chrono::Utc;
use tauri::State;
use uuid::Uuid;

use crate::{
    models::{
        Job, JobEnqueueResponse, JobKind, JobStatus, PromptTemplate, SessionStatus, SummaryResult,
        TranscriptSegment,
    },
    providers::bailian::{summarize_with_bailian, BailianConfig},
    state::AppState,
};

fn now_iso() -> String {
    Utc::now().to_rfc3339()
}

#[tauri::command]
pub fn summary_enqueue(
    session_id: String,
    template_id: Option<String>,
    prompt_override: Option<String>,
    state: State<'_, AppState>,
) -> Result<JobEnqueueResponse, String> {
    let job_id = Uuid::new_v4().to_string();
    let now = now_iso();

    let (transcript, template, bailian_config) = {
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

        storage.save()?;

        let config = settings
            .bailian_api_key
            .clone()
            .map(|api_key| BailianConfig {
                base_url: settings.bailian_base_url.clone(),
                api_key,
                transcription_model: settings.bailian_transcription_model.clone(),
                summary_model: settings.bailian_summary_model.clone(),
                oss: None,
            });

        (transcript, template, config)
    };

    let summary_result = if let Some(config) = bailian_config {
        summarize_with_bailian(
            &transcript,
            &template.system_prompt,
            &template.user_prompt,
            prompt_override.as_deref(),
            &config,
        )
    } else {
        Ok(mock_summary(&transcript, prompt_override.as_deref()))
    };

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

fn mock_summary(transcript: &[TranscriptSegment], prompt_override: Option<&str>) -> SummaryResult {
    let transcript_size = transcript.len();

    SummaryResult {
        title: "[mock] Meeting Summary".to_string(),
        decisions: vec!["Initial architecture and flow are confirmed.".to_string()],
        action_items: vec![
            "Implement real recorder pipeline in Rust.".to_string(),
            "Integrate Bailian ASR endpoint for transcription.".to_string(),
        ],
        risks: vec!["Long recording stability and retry strategy need stress testing.".to_string()],
        timeline: vec!["M1: skeleton", "M2: transcription", "M3: summary"]
            .into_iter()
            .map(str::to_string)
            .collect(),
        raw_markdown: format!(
            "# Meeting Summary\n\nTranscript segments: {transcript_size}\n\nPrompt override: {}\n",
            prompt_override.unwrap_or("<none>")
        ),
    }
}
