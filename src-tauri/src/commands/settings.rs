use std::{
    fs::File,
    io::{Read, Seek, Write},
    path::Path,
};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use tauri::State;
use zip::{write::SimpleFileOptions, CompressionMethod, ZipArchive, ZipWriter};

use crate::{
    models::{Settings, SettingsPatch},
    state::AppState,
    storage::StorageUsageSummary,
};

const SETTINGS_BACKUP_FORMAT_VERSION: u16 = 1;
const SETTINGS_BACKUP_MANIFEST_FILE: &str = "manifest.json";
const SETTINGS_BACKUP_SETTINGS_FILE: &str = "settings.json";
const SETTINGS_BACKUP_UI_FILE: &str = "ui.json";

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsBackupExportRequest {
    pub file_path: String,
    pub settings: Settings,
    pub locale: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsBackupExportResult {
    pub file_path: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsBackupImportResult {
    pub settings: Settings,
    pub locale: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SettingsBackupManifest {
    format_version: u16,
    app_version: String,
    exported_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SettingsBackupUi {
    locale: String,
}

#[tauri::command]
pub fn settings_get(state: State<'_, AppState>) -> Result<Settings, String> {
    let storage = state
        .storage
        .lock()
        .map_err(|_| "failed to acquire storage lock".to_string())?;
    storage.get_settings()
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
    let mut settings = storage.get_settings()?;
    let mut should_save_person_name_mappings = false;

    if let Some(providers) = request.providers {
        settings.providers = providers;
    }

    if let Some(oss_configs) = request.oss_configs {
        settings.oss_configs = oss_configs;
    }

    if let Some(selected_oss_config_id) = request.selected_oss_config_id {
        settings.selected_oss_config_id = selected_oss_config_id;
    }

    if let Some(legacy_oss) = request.legacy_oss {
        settings.legacy_oss = Some(legacy_oss);
    }

    if let Some(selected_transcription_provider_id) = request.selected_transcription_provider_id {
        settings.selected_transcription_provider_id = selected_transcription_provider_id;
    }

    if let Some(selected_summary_provider_id) = request.selected_summary_provider_id {
        settings.selected_summary_provider_id = selected_summary_provider_id;
    }

    if let Some(selected_discover_provider_id) = request.selected_discover_provider_id {
        settings.selected_discover_provider_id = selected_discover_provider_id;
    }

    if let Some(recording_segment_seconds) = request.recording_segment_seconds {
        settings.recording_segment_seconds = recording_segment_seconds;
    }

    if let Some(recording_input_device_id) = request.recording_input_device_id {
        settings.recording_input_device_id = Some(recording_input_device_id);
    }

    if let Some(summary_export_folder_path) = request.summary_export_folder_path {
        settings.summary_export_folder_path = Some(summary_export_folder_path);
    }

    if let Some(session_tag_catalog) = request.session_tag_catalog {
        settings.session_tag_catalog = session_tag_catalog;
    }

    if let Some(person_name_mappings) = request.person_name_mappings {
        settings.person_name_mappings = person_name_mappings;
        should_save_person_name_mappings = true;
    }

    if let Some(default_template_id) = request.default_template_id {
        settings.default_template_id = default_template_id;
    }

    if let Some(templates) = request.templates {
        settings.templates = templates;
    }

    settings.normalize();
    if should_save_person_name_mappings {
        storage.save_settings_and_person_name_mappings(&settings)?;
    } else {
        storage.save_settings(&settings)?;
    }
    Ok(settings)
}

#[tauri::command]
pub fn settings_get_storage_usage(
    state: State<'_, AppState>,
) -> Result<StorageUsageSummary, String> {
    let storage = state
        .storage
        .lock()
        .map_err(|_| "failed to acquire storage lock".to_string())?;

    storage.get_storage_usage()
}

#[tauri::command]
pub fn settings_export_backup(
    request: SettingsBackupExportRequest,
) -> Result<SettingsBackupExportResult, String> {
    let path = Path::new(request.file_path.trim());
    write_settings_backup(path, &request.settings, &request.locale)?;
    Ok(SettingsBackupExportResult {
        file_path: path.display().to_string(),
    })
}

#[tauri::command]
pub fn settings_import_backup(
    file_path: String,
    state: State<'_, AppState>,
) -> Result<SettingsBackupImportResult, String> {
    let path = Path::new(file_path.trim());
    let mut result = read_settings_backup(path)?;

    let mut storage = state
        .storage
        .lock()
        .map_err(|_| "failed to acquire storage lock".to_string())?;
    storage.replace_settings(&result.settings)?;
    result.settings = storage.get_settings()?;
    Ok(result)
}

fn write_settings_backup(
    file_path: &Path,
    settings: &Settings,
    locale: &str,
) -> Result<(), String> {
    validate_backup_file_path(file_path)?;
    let locale = validate_backup_locale(locale)?;
    let mut normalized = settings.clone();
    normalized.normalize();

    let file = File::create(file_path).map_err(|error| {
        format!(
            "failed to create settings backup {}: {error}",
            file_path.display()
        )
    })?;
    let mut archive = ZipWriter::new(file);
    let manifest = SettingsBackupManifest {
        format_version: SETTINGS_BACKUP_FORMAT_VERSION,
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        exported_at: Utc::now().to_rfc3339(),
    };
    let ui = SettingsBackupUi { locale };

    write_backup_json_entry(&mut archive, SETTINGS_BACKUP_MANIFEST_FILE, &manifest)?;
    write_backup_json_entry(&mut archive, SETTINGS_BACKUP_SETTINGS_FILE, &normalized)?;
    write_backup_json_entry(&mut archive, SETTINGS_BACKUP_UI_FILE, &ui)?;
    archive
        .finish()
        .map_err(|error| format!("failed to finalize settings backup zip: {error}"))?;
    Ok(())
}

fn read_settings_backup(file_path: &Path) -> Result<SettingsBackupImportResult, String> {
    if file_path.as_os_str().is_empty() {
        return Err("settings backup path is required".to_string());
    }
    let file = File::open(file_path).map_err(|error| {
        format!(
            "failed to open settings backup {}: {error}",
            file_path.display()
        )
    })?;
    let mut archive = ZipArchive::new(file)
        .map_err(|error| format!("failed to read settings backup zip: {error}"))?;

    let manifest: SettingsBackupManifest = read_backup_json_entry(
        &mut archive,
        SETTINGS_BACKUP_MANIFEST_FILE,
        "settings backup manifest",
    )?;
    if manifest.format_version != SETTINGS_BACKUP_FORMAT_VERSION {
        return Err(format!(
            "unsupported settings backup format version: {}",
            manifest.format_version
        ));
    }

    let mut settings: Settings = read_backup_json_entry(
        &mut archive,
        SETTINGS_BACKUP_SETTINGS_FILE,
        "settings backup settings",
    )?;
    let ui: SettingsBackupUi =
        read_backup_json_entry(&mut archive, SETTINGS_BACKUP_UI_FILE, "settings backup ui")?;
    let locale = validate_backup_locale(&ui.locale)?;
    settings.normalize();

    Ok(SettingsBackupImportResult { settings, locale })
}

fn validate_backup_file_path(file_path: &Path) -> Result<(), String> {
    if file_path.as_os_str().is_empty() {
        return Err("settings backup path is required".to_string());
    }
    let parent = file_path.parent().ok_or_else(|| {
        format!(
            "settings backup path must include a parent folder: {}",
            file_path.display()
        )
    })?;
    if !parent.as_os_str().is_empty() && !parent.is_dir() {
        return Err(format!(
            "settings backup parent folder does not exist: {}",
            parent.display()
        ));
    }
    Ok(())
}

fn validate_backup_locale(locale: &str) -> Result<String, String> {
    let locale = locale.trim();
    if locale == "zh-CN" || locale == "en-US" {
        return Ok(locale.to_string());
    }
    Err(format!("unsupported settings backup locale: {locale}"))
}

fn backup_zip_options() -> SimpleFileOptions {
    SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .unix_permissions(0o600)
}

fn write_backup_json_entry<T: Serialize>(
    archive: &mut ZipWriter<File>,
    name: &str,
    value: &T,
) -> Result<(), String> {
    let payload = serde_json::to_vec_pretty(value)
        .map_err(|error| format!("failed to serialize {name}: {error}"))?;
    archive
        .start_file(name, backup_zip_options())
        .map_err(|error| format!("failed to start settings backup entry {name}: {error}"))?;
    archive
        .write_all(&payload)
        .map_err(|error| format!("failed to write settings backup entry {name}: {error}"))?;
    Ok(())
}

fn read_backup_json_entry<T, R>(
    archive: &mut ZipArchive<R>,
    name: &str,
    label: &str,
) -> Result<T, String>
where
    T: for<'de> Deserialize<'de>,
    R: Read + Seek,
{
    let mut file = archive
        .by_name(name)
        .map_err(|error| format!("failed to find {label}: {error}"))?;
    let mut payload = String::new();
    file.read_to_string(&mut payload)
        .map_err(|error| format!("failed to read {label}: {error}"))?;
    serde_json::from_str(&payload).map_err(|error| format!("failed to parse {label}: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::PersonNameMapping;

    fn temp_backup_path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("open-recorder-{name}-{}.zip", uuid::Uuid::new_v4()))
    }

    #[test]
    fn settings_backup_round_trips_settings_and_locale() {
        let path = temp_backup_path("settings-backup-round-trip");
        let mut settings = Settings::default();
        settings.recording_segment_seconds = 300;
        settings.person_name_mappings = vec![PersonNameMapping {
            id: "person-name-1".to_string(),
            source_name: "ren shee".to_string(),
            target_name: "Renxi".to_string(),
        }];

        write_settings_backup(&path, &settings, "en-US").expect("backup export should succeed");
        let result = read_settings_backup(&path).expect("backup import should succeed");
        let _ = std::fs::remove_file(&path);

        assert_eq!(result.locale, "en-US");
        assert_eq!(result.settings.recording_segment_seconds, 300);
        assert_eq!(result.settings.person_name_mappings.len(), 1);
        assert_eq!(result.settings.person_name_mappings[0].target_name, "Renxi");
    }

    #[test]
    fn settings_backup_rejects_invalid_locale() {
        let path = temp_backup_path("settings-backup-invalid-locale");
        let result = write_settings_backup(&path, &Settings::default(), "fr-FR");
        assert!(result.is_err());
        let _ = std::fs::remove_file(&path);
    }
}
