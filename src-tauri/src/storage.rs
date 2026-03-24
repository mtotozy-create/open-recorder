use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::models::{
    merge_session_tags_into_catalog, normalize_tags, InsightCacheEntry, Job, Session, Settings,
    DEFAULT_RECORDING_SESSION_TAG,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PersistedState {
    pub sessions: HashMap<String, Session>,
    pub jobs: HashMap<String, Job>,
    pub insight_cache: HashMap<String, InsightCacheEntry>,
    pub settings: Settings,
    pub session_tags_backfilled: bool,
}

impl Default for PersistedState {
    fn default() -> Self {
        Self {
            sessions: HashMap::new(),
            jobs: HashMap::new(),
            insight_cache: HashMap::new(),
            settings: Settings::default(),
            session_tags_backfilled: false,
        }
    }
}

pub struct Storage {
    file_path: PathBuf,
    pub data: PersistedState,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageUsageSummary {
    pub data_dir_path: String,
    pub total_bytes: u64,
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
                        let migrated = backfill_session_tags_if_needed(&mut data);
                        let storage = Self { file_path, data };
                        if migrated {
                            storage.save()?;
                        }
                        return Ok(storage);
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
        let root = self.data_root_dir()?;
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
        let root = self.data_root_dir()?;
        let export_dir = root.join("exports").join(session_id);
        fs::create_dir_all(&export_dir).map_err(|error| {
            format!(
                "failed to create session export dir {}: {error}",
                export_dir.display()
            )
        })?;
        Ok(export_dir)
    }

    pub fn data_root_dir(&self) -> Result<PathBuf, String> {
        self.file_path
            .parent()
            .map(|path| path.to_path_buf())
            .ok_or_else(|| "failed to resolve data directory".to_string())
    }

    pub fn get_storage_usage(&self) -> Result<StorageUsageSummary, String> {
        let data_dir = self.data_root_dir()?;
        let total_bytes = calculate_dir_size(&data_dir)?;
        Ok(StorageUsageSummary {
            data_dir_path: data_dir.display().to_string(),
            total_bytes,
        })
    }
}

fn backfill_session_tags_if_needed(data: &mut PersistedState) -> bool {
    if data.session_tags_backfilled {
        return false;
    }

    for session in data.sessions.values_mut() {
        let normalized = normalize_tags(&session.tags);
        if normalized.is_empty() {
            session.tags = vec![DEFAULT_RECORDING_SESSION_TAG.to_string()];
            continue;
        }
        if normalized != session.tags {
            session.tags = normalized;
        }
    }

    data.session_tags_backfilled = true;
    for session in data.sessions.values() {
        merge_session_tags_into_catalog(&mut data.settings.session_tag_catalog, &session.tags);
    }
    data.settings.normalize();
    true
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

fn calculate_dir_size(path: &Path) -> Result<u64, String> {
    if !path.exists() {
        return Ok(0);
    }

    let metadata = fs::metadata(path)
        .map_err(|error| format!("failed to read {}: {error}", path.display()))?;

    if metadata.is_file() {
        return Ok(metadata.len());
    }

    if !metadata.is_dir() {
        return Ok(0);
    }

    let mut total_bytes = 0_u64;
    let entries = fs::read_dir(path)
        .map_err(|error| format!("failed to read directory {}: {error}", path.display()))?;

    for entry in entries {
        let entry = entry.map_err(|error| format!("failed to read directory entry: {error}"))?;
        total_bytes += calculate_dir_size(&entry.path())?;
    }

    Ok(total_bytes)
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::{Path, PathBuf},
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::calculate_dir_size;

    struct TempDirGuard {
        path: PathBuf,
    }

    impl TempDirGuard {
        fn new(prefix: &str) -> Result<Self, String> {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|error| format!("failed to build temp dir name: {error}"))?
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "open-recorder-{prefix}-{}-{unique}",
                std::process::id()
            ));
            fs::create_dir_all(&path).map_err(|error| {
                format!("failed to create temp dir {}: {error}", path.display())
            })?;
            Ok(Self { path })
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TempDirGuard {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn calculate_dir_size_returns_zero_for_empty_dir() {
        let temp_dir = TempDirGuard::new("empty").expect("temp dir should be created");

        let size = calculate_dir_size(temp_dir.path()).expect("size should be calculated");

        assert_eq!(size, 0);
    }

    #[test]
    fn calculate_dir_size_sums_nested_files() {
        let temp_dir = TempDirGuard::new("nested").expect("temp dir should be created");
        let nested_dir = temp_dir.path().join("audio/session-1");
        fs::create_dir_all(&nested_dir).expect("nested dir should be created");
        fs::write(temp_dir.path().join("state.json"), b"12345")
            .expect("state file should be written");
        fs::write(nested_dir.join("segment-1.m4a"), b"1234567890")
            .expect("segment file should be written");
        fs::write(nested_dir.join("segment-2.m4a"), b"1234")
            .expect("segment file should be written");

        let size = calculate_dir_size(temp_dir.path()).expect("size should be calculated");

        assert_eq!(size, 19);
    }

    #[test]
    fn calculate_dir_size_returns_zero_for_missing_dir() {
        let missing_dir =
            std::env::temp_dir().join(format!("open-recorder-missing-{}", std::process::id()));

        let size =
            calculate_dir_size(&missing_dir).expect("missing dir should be treated as empty");

        assert_eq!(size, 0);
    }
}
