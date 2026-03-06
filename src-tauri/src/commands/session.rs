use chrono::Utc;
use tauri::State;

use crate::{
    models::{Session, SessionSummary},
    state::AppState,
};

fn now_iso() -> String {
    Utc::now().to_rfc3339()
}

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

#[tauri::command]
pub fn session_rename(
    session_id: String,
    name: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let mut storage = state
        .storage
        .lock()
        .map_err(|_| "failed to acquire storage lock".to_string())?;

    let normalized = name.trim();
    let session = storage
        .data
        .sessions
        .get_mut(&session_id)
        .ok_or_else(|| "session not found".to_string())?;
    session.name = if normalized.is_empty() {
        None
    } else {
        Some(normalized.to_string())
    };
    session.updated_at = now_iso();
    storage.save()?;
    Ok(())
}

#[tauri::command]
pub fn session_delete(session_id: String, state: State<'_, AppState>) -> Result<(), String> {
    let mut storage = state
        .storage
        .lock()
        .map_err(|_| "failed to acquire storage lock".to_string())?;

    let session = storage
        .data
        .sessions
        .remove(&session_id)
        .ok_or_else(|| "session not found".to_string())?;

    for path in session.audio_segments {
        let _ = std::fs::remove_file(&path);
    }

    // also remove jobs matching this session_id
    storage.data.jobs.retain(|_, job| job.session_id != session_id);
    
    storage.save()?;
    Ok(())
}
