use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::models::{Job, Session, Settings};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PersistedState {
    pub sessions: HashMap<String, Session>,
    pub jobs: HashMap<String, Job>,
    pub settings: Settings,
}

impl Default for PersistedState {
    fn default() -> Self {
        Self {
            sessions: HashMap::new(),
            jobs: HashMap::new(),
            settings: Settings::default(),
        }
    }
}

pub struct Storage {
    file_path: PathBuf,
    pub data: PersistedState,
}

impl Storage {
    pub fn load() -> Result<Self, String> {
        let mut errors: Vec<String> = vec![];

        for data_dir in resolve_data_dir_candidates() {
            if let Err(error) = fs::create_dir_all(&data_dir) {
                errors.push(format!(
                    "failed to create data dir {}: {error}",
                    data_dir.display()
                ));
                continue;
            }

            let file_path = data_dir.join("state.json");
            if !file_path.exists() {
                let mut data = PersistedState::default();
                data.settings.normalize();
                return Ok(Self { file_path, data });
            }

            match fs::read_to_string(&file_path) {
                Ok(raw) => match serde_json::from_str::<PersistedState>(&raw) {
                    Ok(mut data) => {
                        data.settings.normalize();
                        return Ok(Self { file_path, data });
                    }
                    Err(error) => {
                        errors.push(format!(
                            "failed to parse persisted state {}: {error}",
                            file_path.display()
                        ));
                    }
                },
                Err(error) => {
                    errors.push(format!("failed to read {}: {error}", file_path.display()));
                }
            }
        }

        Err(format!(
            "failed to initialize persisted storage; attempts: {}",
            errors.join(" | ")
        ))
    }

    pub fn save(&self) -> Result<(), String> {
        let payload = serde_json::to_string_pretty(&self.data)
            .map_err(|error| format!("failed to serialize state: {error}"))?;

        write_atomic(&self.file_path, payload.as_bytes())
    }

    pub fn session_audio_dir(&self, session_id: &str) -> Result<PathBuf, String> {
        let root = self
            .file_path
            .parent()
            .ok_or_else(|| "failed to resolve data directory".to_string())?;
        let audio_dir = root.join("audio").join(session_id).join("segments");
        fs::create_dir_all(&audio_dir).map_err(|error| {
            format!(
                "failed to create session audio dir {}: {error}",
                audio_dir.display()
            )
        })?;
        Ok(audio_dir)
    }

    pub fn session_export_dir(&self, session_id: &str) -> Result<PathBuf, String> {
        let root = self
            .file_path
            .parent()
            .ok_or_else(|| "failed to resolve data directory".to_string())?;
        let export_dir = root.join("exports").join(session_id);
        fs::create_dir_all(&export_dir).map_err(|error| {
            format!(
                "failed to create session export dir {}: {error}",
                export_dir.display()
            )
        })?;
        Ok(export_dir)
    }
}

fn resolve_data_dir_candidates() -> Vec<PathBuf> {
    let mut candidates: Vec<PathBuf> = vec![];

    if let Ok(path) = std::env::var("OPEN_RECORDER_DATA_DIR") {
        candidates.push(PathBuf::from(path));
    }

    #[cfg(target_os = "macos")]
    {
        if let Ok(home) = std::env::var("HOME") {
            candidates.push(
                PathBuf::from(home)
                    .join("Library")
                    .join("Application Support")
                    .join("Open Recorder"),
            );
        }
    }

    if let Ok(home) = std::env::var("HOME") {
        candidates.push(PathBuf::from(home).join(".open-recorder-data"));
    }

    if let Ok(current_dir) = std::env::current_dir() {
        candidates.push(current_dir.join(".open-recorder-data"));
    }

    candidates.push(std::env::temp_dir().join("open-recorder-data"));

    let mut deduped: Vec<PathBuf> = vec![];
    for path in candidates {
        if !deduped.iter().any(|value| value == &path) {
            deduped.push(path);
        }
    }

    deduped
}

fn write_atomic(path: &Path, data: &[u8]) -> Result<(), String> {
    let tmp_path = path.with_extension("json.tmp");
    fs::write(&tmp_path, data)
        .map_err(|error| format!("failed to write {}: {error}", tmp_path.display()))?;
    fs::rename(&tmp_path, path)
        .map_err(|error| format!("failed to replace {}: {error}", path.display()))?;
    Ok(())
}
