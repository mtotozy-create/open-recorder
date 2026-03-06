use tauri::State;

use crate::{
    models::{Session, SessionSummary},
    state::AppState,
};

#[tauri::command]
pub fn session_list(state: State<'_, AppState>) -> Result<Vec<SessionSummary>, String> {
    let storage = state
        .storage
        .lock()
        .map_err(|_| "failed to acquire storage lock".to_string())?;
    let mut list: Vec<SessionSummary> = storage
        .data
        .sessions
        .values()
        .map(SessionSummary::from)
        .collect();
    list.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(list)
}

#[tauri::command]
pub fn session_get(session_id: String, state: State<'_, AppState>) -> Result<Session, String> {
    let storage = state
        .storage
        .lock()
        .map_err(|_| "failed to acquire storage lock".to_string())?;
    storage
        .data
        .sessions
        .get(&session_id)
        .cloned()
        .ok_or_else(|| "session not found".to_string())
}
