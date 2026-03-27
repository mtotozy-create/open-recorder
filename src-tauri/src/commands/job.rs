use tauri::State;

use crate::{models::Job, state::AppState};

#[tauri::command]
pub fn job_get(job_id: String, state: State<'_, AppState>) -> Result<Job, String> {
    let storage = state
        .storage
        .lock()
        .map_err(|_| "failed to acquire storage lock".to_string())?;
    storage
        .get_job(&job_id)?
        .ok_or_else(|| "job not found".to_string())
}

#[tauri::command]
pub fn session_jobs(session_id: String, state: State<'_, AppState>) -> Result<Vec<Job>, String> {
    let storage = state
        .storage
        .lock()
        .map_err(|_| "failed to acquire storage lock".to_string())?;
    storage.list_jobs_for_session(&session_id)
}
