use tauri::State;

use crate::{
    models::{Settings, SettingsPatch},
    state::AppState,
};

#[tauri::command]
pub fn settings_get(state: State<'_, AppState>) -> Result<Settings, String> {
    let mut storage = state
        .storage
        .lock()
        .map_err(|_| "failed to acquire storage lock".to_string())?;
    storage.data.settings.normalize();
    Ok(storage.data.settings.clone())
}

#[tauri::command]
pub fn settings_update(
    request: SettingsPatch,
    state: State<'_, AppState>,
) -> Result<Settings, String> {
    let mut storage = state
        .storage
        .lock()
        .map_err(|_| "failed to acquire storage lock".to_string())?;

    if let Some(providers) = request.providers {
        storage.data.settings.providers = providers;
    }

    if let Some(selected_transcription_provider_id) = request.selected_transcription_provider_id {
        storage.data.settings.selected_transcription_provider_id = selected_transcription_provider_id;
    }

    if let Some(selected_summary_provider_id) = request.selected_summary_provider_id {
        storage.data.settings.selected_summary_provider_id = selected_summary_provider_id;
    }

    if let Some(default_template_id) = request.default_template_id {
        storage.data.settings.default_template_id = default_template_id;
    }

    if let Some(templates) = request.templates {
        storage.data.settings.templates = templates;
    }

    storage.data.settings.normalize();
    storage.save()?;
    Ok(storage.data.settings.clone())
}
