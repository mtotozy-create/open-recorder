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

    if let Some(oss_configs) = request.oss_configs {
        storage.data.settings.oss_configs = oss_configs;
    }

    if let Some(selected_oss_config_id) = request.selected_oss_config_id {
        storage.data.settings.selected_oss_config_id = selected_oss_config_id;
    }

    if let Some(legacy_oss) = request.legacy_oss {
        storage.data.settings.legacy_oss = Some(legacy_oss);
    }

    if let Some(selected_transcription_provider_id) = request.selected_transcription_provider_id {
        storage.data.settings.selected_transcription_provider_id =
            selected_transcription_provider_id;
    }

    if let Some(selected_summary_provider_id) = request.selected_summary_provider_id {
        storage.data.settings.selected_summary_provider_id = selected_summary_provider_id;
    }

    if let Some(recording_segment_seconds) = request.recording_segment_seconds {
        storage.data.settings.recording_segment_seconds = recording_segment_seconds;
    }

    if let Some(session_tag_catalog) = request.session_tag_catalog {
        storage.data.settings.session_tag_catalog = session_tag_catalog;
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
