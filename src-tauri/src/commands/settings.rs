use tauri::State;

use crate::{
    models::{Settings, SettingsPatch},
    state::AppState,
};

fn trim_to_option(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        if value.trim().is_empty() {
            None
        } else {
            Some(value)
        }
    })
}

#[tauri::command]
pub fn settings_get(state: State<'_, AppState>) -> Result<Settings, String> {
    let storage = state
        .storage
        .lock()
        .map_err(|_| "failed to acquire storage lock".to_string())?;
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

    if let Some(transcription_provider) = request.transcription_provider {
        storage.data.settings.transcription_provider = transcription_provider;
    }

    if let Some(bailian_api_key) = request.bailian_api_key {
        storage.data.settings.bailian_api_key = trim_to_option(bailian_api_key);
    }

    if let Some(bailian_base_url) = request.bailian_base_url {
        storage.data.settings.bailian_base_url = bailian_base_url;
    }

    if let Some(bailian_transcription_model) = request.bailian_transcription_model {
        storage.data.settings.bailian_transcription_model = bailian_transcription_model;
    }

    if let Some(bailian_summary_model) = request.bailian_summary_model {
        storage.data.settings.bailian_summary_model = bailian_summary_model;
    }

    if let Some(bailian_oss_access_key_id) = request.bailian_oss_access_key_id {
        storage.data.settings.bailian_oss_access_key_id = trim_to_option(bailian_oss_access_key_id);
    }

    if let Some(bailian_oss_access_key_secret) = request.bailian_oss_access_key_secret {
        storage.data.settings.bailian_oss_access_key_secret =
            trim_to_option(bailian_oss_access_key_secret);
    }

    if let Some(bailian_oss_endpoint) = request.bailian_oss_endpoint {
        storage.data.settings.bailian_oss_endpoint = trim_to_option(bailian_oss_endpoint);
    }

    if let Some(bailian_oss_bucket) = request.bailian_oss_bucket {
        storage.data.settings.bailian_oss_bucket = trim_to_option(bailian_oss_bucket);
    }

    if let Some(bailian_oss_path_prefix) = request.bailian_oss_path_prefix {
        storage.data.settings.bailian_oss_path_prefix = trim_to_option(bailian_oss_path_prefix);
    }

    if let Some(bailian_oss_signed_url_ttl_seconds) = request.bailian_oss_signed_url_ttl_seconds {
        storage.data.settings.bailian_oss_signed_url_ttl_seconds =
            bailian_oss_signed_url_ttl_seconds.clamp(60, 86_400);
    }

    if let Some(aliyun_access_key_id) = request.aliyun_access_key_id {
        storage.data.settings.aliyun_access_key_id = trim_to_option(aliyun_access_key_id);
    }

    if let Some(aliyun_access_key_secret) = request.aliyun_access_key_secret {
        storage.data.settings.aliyun_access_key_secret = trim_to_option(aliyun_access_key_secret);
    }

    if let Some(aliyun_app_key) = request.aliyun_app_key {
        storage.data.settings.aliyun_app_key = trim_to_option(aliyun_app_key);
    }

    if let Some(aliyun_endpoint) = request.aliyun_endpoint {
        storage.data.settings.aliyun_endpoint = aliyun_endpoint;
    }

    if let Some(aliyun_source_language) = request.aliyun_source_language {
        storage.data.settings.aliyun_source_language = aliyun_source_language;
    }

    if let Some(aliyun_file_url_prefix) = request.aliyun_file_url_prefix {
        storage.data.settings.aliyun_file_url_prefix = trim_to_option(aliyun_file_url_prefix);
    }

    if let Some(default_template_id) = request.default_template_id {
        storage.data.settings.default_template_id = default_template_id;
    }

    if let Some(templates) = request.templates {
        storage.data.settings.templates = templates;
    }

    storage.save()?;
    Ok(storage.data.settings.clone())
}
