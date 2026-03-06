use tauri::State;

use crate::{models::Job, state::AppState};

#[tauri::command]
pub fn job_get(job_id: String, state: State<'_, AppState>) -> Result<Job, String> {
    let storage = state
        .storage
        .lock()
        .map_err(|_| "failed to acquire storage lock".to_string())?;
    storage
        .data
        .jobs
        .get(&job_id)
        .cloned()
        .ok_or_else(|| "job not found".to_string())
}
