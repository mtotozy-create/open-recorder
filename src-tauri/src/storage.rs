use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{de::DeserializeOwned, Deserialize, Serialize};

use crate::models::{
    merge_session_tags_into_catalog, normalize_tags, InsightCacheEntry, Job, Session,
    SessionStatus, SessionSummary, Settings, DEFAULT_RECORDING_SESSION_TAG,
};

const DATABASE_FILE_NAME: &str = "open-recorder.db";
const DATABASE_TEMP_FILE_NAME: &str = "open-recorder.db.tmp";
const LEGACY_STATE_FILE_NAME: &str = "state.json";
const SCHEMA_VERSION: &str = "1";

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
    data_dir: PathBuf,
    connection: Connection,
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
            match Self::load_from_dir(&data_dir) {
                Ok(storage) => return Ok(storage),
                Err(error) => errors.push(error),
            }
        }

        Err(format!(
            "failed to initialize persisted storage; attempts: {}",
            errors.join(" | ")
        ))
    }

    fn load_from_dir(data_dir: &Path) -> Result<Self, String> {
        fs::create_dir_all(data_dir).map_err(|error| {
            format!("failed to create data dir {}: {error}", data_dir.display())
        })?;

        let db_path = data_dir.join(DATABASE_FILE_NAME);
        let temp_db_path = data_dir.join(DATABASE_TEMP_FILE_NAME);
        let legacy_path = data_dir.join(LEGACY_STATE_FILE_NAME);

        if db_path.exists() {
            return Self::open_existing(data_dir.to_path_buf(), &db_path);
        }

        if legacy_path.exists() {
            migrate_legacy_state_to_database(&legacy_path, &temp_db_path)?;
            fs::rename(&temp_db_path, &db_path).map_err(|error| {
                format!(
                    "failed to finalize migrated database {} -> {}: {error}",
                    temp_db_path.display(),
                    db_path.display()
                )
            })?;

            let backup_path = data_dir.join("state.json.bak");
            if backup_path.exists() {
                let _ = fs::remove_file(&backup_path);
            }
            fs::rename(&legacy_path, &backup_path).map_err(|error| {
                format!(
                    "failed to back up legacy state {} -> {}: {error}",
                    legacy_path.display(),
                    backup_path.display()
                )
            })?;

            return Self::open_existing(data_dir.to_path_buf(), &db_path);
        }

        if temp_db_path.exists() {
            let _ = fs::remove_file(&temp_db_path);
        }

        let connection = open_connection(&db_path)?;
        initialize_schema(&connection)?;
        ensure_settings_row(&connection)?;

        Ok(Self {
            data_dir: data_dir.to_path_buf(),
            connection,
        })
    }

    fn open_existing(data_dir: PathBuf, db_path: &Path) -> Result<Self, String> {
        let connection = open_connection(db_path)?;
        initialize_schema(&connection)?;
        ensure_settings_row(&connection)?;
        recover_interrupted_sessions(&connection)?;

        Ok(Self {
            data_dir,
            connection,
        })
    }

    pub fn get_settings(&self) -> Result<Settings, String> {
        load_settings(&self.connection)
    }

    pub fn save_settings(&self, settings: &Settings) -> Result<(), String> {
        upsert_settings(&self.connection, settings)
    }

    pub fn list_sessions(&self) -> Result<Vec<SessionSummary>, String> {
        let sessions = load_all_sessions(&self.connection)?;
        let mut summaries: Vec<SessionSummary> =
            sessions.iter().map(SessionSummary::from).collect();
        summaries.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(summaries)
    }

    pub fn list_all_sessions(&self) -> Result<Vec<Session>, String> {
        load_all_sessions(&self.connection)
    }

    pub fn get_session(&self, session_id: &str) -> Result<Option<Session>, String> {
        load_session(&self.connection, session_id)
    }

    pub fn upsert_session(&self, session: &Session) -> Result<(), String> {
        upsert_session(&self.connection, session)
    }

    pub fn save_settings_and_session(
        &mut self,
        settings: &Settings,
        session: &Session,
    ) -> Result<(), String> {
        let tx = self
            .connection
            .transaction()
            .map_err(|error| format!("failed to start settings/session transaction: {error}"))?;
        upsert_settings(&tx, settings)?;
        upsert_session(&tx, session)?;
        tx.commit()
            .map_err(|error| format!("failed to commit settings/session transaction: {error}"))?;
        Ok(())
    }

    pub fn save_session_and_job(&mut self, session: &Session, job: &Job) -> Result<(), String> {
        let tx = self
            .connection
            .transaction()
            .map_err(|error| format!("failed to start session/job transaction: {error}"))?;
        upsert_session(&tx, session)?;
        upsert_job(&tx, job)?;
        tx.commit()
            .map_err(|error| format!("failed to commit session/job transaction: {error}"))?;
        Ok(())
    }

    pub fn get_job(&self, job_id: &str) -> Result<Option<Job>, String> {
        load_job(&self.connection, job_id)
    }

    pub fn list_jobs_for_session(&self, session_id: &str) -> Result<Vec<Job>, String> {
        load_jobs_for_session(&self.connection, session_id)
    }

    pub fn upsert_job(&self, job: &Job) -> Result<(), String> {
        upsert_job(&self.connection, job)
    }

    pub fn get_insight_cache(&self, key: &str) -> Result<Option<InsightCacheEntry>, String> {
        load_insight_cache_entry(&self.connection, key)
    }

    pub fn upsert_insight_cache(&self, entry: &InsightCacheEntry) -> Result<(), String> {
        upsert_insight_cache(&self.connection, entry)
    }

    pub fn save_job_and_insight_cache(
        &mut self,
        job: &Job,
        cache: &InsightCacheEntry,
    ) -> Result<(), String> {
        let tx = self
            .connection
            .transaction()
            .map_err(|error| format!("failed to start insight transaction: {error}"))?;
        upsert_job(&tx, job)?;
        upsert_insight_cache(&tx, cache)?;
        tx.commit()
            .map_err(|error| format!("failed to commit insight transaction: {error}"))?;
        Ok(())
    }

    pub fn delete_session_and_jobs(&mut self, session_id: &str) -> Result<Option<Session>, String> {
        let tx = self
            .connection
            .transaction()
            .map_err(|error| format!("failed to start delete session transaction: {error}"))?;
        let session = load_session(&tx, session_id)?;
        tx.execute(
            "DELETE FROM jobs WHERE session_id = ?1",
            params![session_id],
        )
        .map_err(|error| format!("failed to delete jobs for session '{session_id}': {error}"))?;
        tx.execute("DELETE FROM sessions WHERE id = ?1", params![session_id])
            .map_err(|error| format!("failed to delete session '{session_id}': {error}"))?;
        tx.commit()
            .map_err(|error| format!("failed to commit delete session transaction: {error}"))?;
        Ok(session)
    }

    pub fn session_audio_dir(&self, session_id: &str) -> Result<PathBuf, String> {
        let audio_dir = self
            .data_dir
            .join("audio")
            .join(session_id)
            .join("segments");
        fs::create_dir_all(&audio_dir).map_err(|error| {
            format!(
                "failed to create session audio dir {}: {error}",
                audio_dir.display()
            )
        })?;
        Ok(audio_dir)
    }

    pub fn session_export_dir(&self, session_id: &str) -> Result<PathBuf, String> {
        let export_dir = self.data_dir.join("exports").join(session_id);
        fs::create_dir_all(&export_dir).map_err(|error| {
            format!(
                "failed to create session export dir {}: {error}",
                export_dir.display()
            )
        })?;
        Ok(export_dir)
    }

    pub fn data_root_dir(&self) -> Result<PathBuf, String> {
        Ok(self.data_dir.clone())
    }

    pub fn get_storage_usage(&self) -> Result<StorageUsageSummary, String> {
        let total_bytes = calculate_dir_size(&self.data_dir)?;
        Ok(StorageUsageSummary {
            data_dir_path: self.data_dir.display().to_string(),
            total_bytes,
        })
    }
}

fn open_connection(path: &Path) -> Result<Connection, String> {
    let connection = Connection::open(path)
        .map_err(|error| format!("failed to open database {}: {error}", path.display()))?;
    connection
        .execute_batch(
            "
            PRAGMA foreign_keys = ON;
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = NORMAL;
            ",
        )
        .map_err(|error| format!("failed to configure database {}: {error}", path.display()))?;
    Ok(connection)
}

fn initialize_schema(connection: &Connection) -> Result<(), String> {
    connection
        .execute_batch(
            "
            CREATE TABLE IF NOT EXISTS meta (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS settings (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                payload_json TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS sessions (
                id TEXT PRIMARY KEY,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                status TEXT NOT NULL,
                discoverable INTEGER NOT NULL,
                name TEXT,
                payload_json TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS jobs (
                id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                kind TEXT NOT NULL,
                status TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                payload_json TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS insight_cache (
                key TEXT PRIMARY KEY,
                cached_at TEXT NOT NULL,
                provider_id TEXT NOT NULL,
                model TEXT NOT NULL,
                payload_json TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_sessions_created_at ON sessions(created_at DESC);
            CREATE INDEX IF NOT EXISTS idx_sessions_updated_at ON sessions(updated_at DESC);
            CREATE INDEX IF NOT EXISTS idx_sessions_discoverable_created_at ON sessions(discoverable, created_at DESC);
            CREATE INDEX IF NOT EXISTS idx_jobs_session_created_at ON jobs(session_id, created_at ASC);
            CREATE INDEX IF NOT EXISTS idx_jobs_status_updated_at ON jobs(status, updated_at DESC);
            CREATE INDEX IF NOT EXISTS idx_insight_cache_cached_at ON insight_cache(cached_at DESC);
            ",
        )
        .map_err(|error| format!("failed to initialize sqlite schema: {error}"))?;

    connection
        .execute(
            "INSERT INTO meta (key, value) VALUES ('schema_version', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![SCHEMA_VERSION],
        )
        .map_err(|error| format!("failed to store schema version: {error}"))?;

    Ok(())
}

fn ensure_settings_row(connection: &Connection) -> Result<(), String> {
    let exists = connection
        .query_row("SELECT 1 FROM settings WHERE id = 1", [], |_| Ok(()))
        .optional()
        .map_err(|error| format!("failed to check settings row: {error}"))?
        .is_some();

    if exists {
        return Ok(());
    }

    let mut settings = Settings::default();
    settings.normalize();
    upsert_settings(connection, &settings)
}

fn migrate_legacy_state_to_database(legacy_path: &Path, temp_db_path: &Path) -> Result<(), String> {
    if temp_db_path.exists() {
        let _ = fs::remove_file(temp_db_path);
    }

    let raw = fs::read_to_string(legacy_path).map_err(|error| {
        format!(
            "failed to read legacy state {}: {error}",
            legacy_path.display()
        )
    })?;
    let mut data = serde_json::from_str::<PersistedState>(&raw).map_err(|error| {
        format!(
            "failed to parse legacy state {}: {error}",
            legacy_path.display()
        )
    })?;

    data.settings.normalize();
    backfill_session_tags_if_needed(&mut data);

    let mut connection = open_connection(temp_db_path)?;
    initialize_schema(&connection)?;

    let tx = connection
        .transaction()
        .map_err(|error| format!("failed to start migration transaction: {error}"))?;
    upsert_settings(&tx, &data.settings)?;
    for session in data.sessions.values() {
        upsert_session(&tx, session)?;
    }
    for job in data.jobs.values() {
        upsert_job(&tx, job)?;
    }
    for entry in data.insight_cache.values() {
        upsert_insight_cache(&tx, entry)?;
    }
    tx.commit()
        .map_err(|error| format!("failed to commit migration transaction: {error}"))?;

    Ok(())
}

fn load_settings(connection: &Connection) -> Result<Settings, String> {
    let payload: String = connection
        .query_row(
            "SELECT payload_json FROM settings WHERE id = 1",
            [],
            |row| row.get(0),
        )
        .map_err(|error| format!("failed to load settings: {error}"))?;
    let mut settings = deserialize_json::<Settings>(&payload, "settings")?;
    settings.normalize();
    Ok(settings)
}

fn upsert_settings(connection: &Connection, settings: &Settings) -> Result<(), String> {
    let mut normalized = settings.clone();
    normalized.normalize();
    let payload = serialize_json(&normalized, "settings")?;
    connection
        .execute(
            "INSERT INTO settings (id, payload_json) VALUES (1, ?1)
             ON CONFLICT(id) DO UPDATE SET payload_json = excluded.payload_json",
            params![payload],
        )
        .map_err(|error| format!("failed to save settings: {error}"))?;
    Ok(())
}

fn load_all_sessions(connection: &Connection) -> Result<Vec<Session>, String> {
    let mut stmt = connection
        .prepare("SELECT payload_json FROM sessions ORDER BY created_at DESC")
        .map_err(|error| format!("failed to prepare session list query: {error}"))?;
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|error| format!("failed to query sessions: {error}"))?;

    let mut sessions = Vec::new();
    for row in rows {
        let payload = row.map_err(|error| format!("failed to read session row: {error}"))?;
        sessions.push(deserialize_json::<Session>(&payload, "session")?);
    }
    Ok(sessions)
}

fn load_session(connection: &Connection, session_id: &str) -> Result<Option<Session>, String> {
    let payload = connection
        .query_row(
            "SELECT payload_json FROM sessions WHERE id = ?1",
            params![session_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| format!("failed to load session '{session_id}': {error}"))?;

    payload
        .map(|value| deserialize_json::<Session>(&value, "session"))
        .transpose()
}

fn upsert_session(connection: &Connection, session: &Session) -> Result<(), String> {
    let payload = serialize_json(session, "session")?;
    connection
        .execute(
            "INSERT INTO sessions (id, created_at, updated_at, status, discoverable, name, payload_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(id) DO UPDATE SET
               created_at = excluded.created_at,
               updated_at = excluded.updated_at,
               status = excluded.status,
               discoverable = excluded.discoverable,
               name = excluded.name,
               payload_json = excluded.payload_json",
            params![
                session.id,
                session.created_at,
                session.updated_at,
                session_status_name(session),
                if session.discoverable { 1_i64 } else { 0_i64 },
                session.name,
                payload,
            ],
        )
        .map_err(|error| format!("failed to save session '{}': {error}", session.id))?;
    Ok(())
}

fn recover_interrupted_sessions(connection: &Connection) -> Result<usize, String> {
    let sessions = load_all_sessions(connection)?;
    let mut recovered = 0_usize;

    for mut session in sessions {
        if !matches!(
            session.status,
            SessionStatus::Recording | SessionStatus::Paused
        ) {
            continue;
        }

        session.status = SessionStatus::Stopped;
        session.updated_at = Utc::now().to_rfc3339();
        upsert_session(connection, &session)?;
        recovered += 1;
    }

    Ok(recovered)
}

fn load_job(connection: &Connection, job_id: &str) -> Result<Option<Job>, String> {
    let payload = connection
        .query_row(
            "SELECT payload_json FROM jobs WHERE id = ?1",
            params![job_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| format!("failed to load job '{job_id}': {error}"))?;

    payload
        .map(|value| deserialize_json::<Job>(&value, "job"))
        .transpose()
}

fn load_jobs_for_session(connection: &Connection, session_id: &str) -> Result<Vec<Job>, String> {
    let mut stmt = connection
        .prepare("SELECT payload_json FROM jobs WHERE session_id = ?1 ORDER BY created_at ASC")
        .map_err(|error| format!("failed to prepare job list query: {error}"))?;
    let rows = stmt
        .query_map(params![session_id], |row| row.get::<_, String>(0))
        .map_err(|error| format!("failed to query jobs for session '{session_id}': {error}"))?;

    let mut jobs = Vec::new();
    for row in rows {
        let payload = row.map_err(|error| format!("failed to read job row: {error}"))?;
        jobs.push(deserialize_json::<Job>(&payload, "job")?);
    }
    Ok(jobs)
}

fn upsert_job(connection: &Connection, job: &Job) -> Result<(), String> {
    let payload = serialize_json(job, "job")?;
    connection
        .execute(
            "INSERT INTO jobs (id, session_id, kind, status, created_at, updated_at, payload_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(id) DO UPDATE SET
               session_id = excluded.session_id,
               kind = excluded.kind,
               status = excluded.status,
               created_at = excluded.created_at,
               updated_at = excluded.updated_at,
               payload_json = excluded.payload_json",
            params![
                job.id,
                job.session_id,
                job_kind_name(job),
                job_status_name(job),
                job.created_at,
                job.updated_at,
                payload,
            ],
        )
        .map_err(|error| format!("failed to save job '{}': {error}", job.id))?;
    Ok(())
}

fn load_insight_cache_entry(
    connection: &Connection,
    key: &str,
) -> Result<Option<InsightCacheEntry>, String> {
    let payload = connection
        .query_row(
            "SELECT payload_json FROM insight_cache WHERE key = ?1",
            params![key],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| format!("failed to load insight cache entry '{key}': {error}"))?;

    payload
        .map(|value| deserialize_json::<InsightCacheEntry>(&value, "insight cache entry"))
        .transpose()
}

fn upsert_insight_cache(connection: &Connection, entry: &InsightCacheEntry) -> Result<(), String> {
    let payload = serialize_json(entry, "insight cache entry")?;
    connection
        .execute(
            "INSERT INTO insight_cache (key, cached_at, provider_id, model, payload_json)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(key) DO UPDATE SET
               cached_at = excluded.cached_at,
               provider_id = excluded.provider_id,
               model = excluded.model,
               payload_json = excluded.payload_json",
            params![
                entry.key,
                entry.cached_at,
                entry.provider_id,
                entry.model,
                payload,
            ],
        )
        .map_err(|error| {
            format!(
                "failed to save insight cache entry '{}': {error}",
                entry.key
            )
        })?;
    Ok(())
}

fn serialize_json<T: Serialize>(value: &T, label: &str) -> Result<String, String> {
    serde_json::to_string(value).map_err(|error| format!("failed to serialize {label}: {error}"))
}

fn deserialize_json<T: DeserializeOwned>(raw: &str, label: &str) -> Result<T, String> {
    serde_json::from_str(raw).map_err(|error| format!("failed to parse {label}: {error}"))
}

fn session_status_name(session: &Session) -> &'static str {
    match session.status {
        crate::models::SessionStatus::Recording => "recording",
        crate::models::SessionStatus::Paused => "paused",
        crate::models::SessionStatus::Processing => "processing",
        crate::models::SessionStatus::Stopped => "stopped",
        crate::models::SessionStatus::Transcribing => "transcribing",
        crate::models::SessionStatus::Summarizing => "summarizing",
        crate::models::SessionStatus::Completed => "completed",
        crate::models::SessionStatus::Failed => "failed",
    }
}

fn job_kind_name(job: &Job) -> &'static str {
    match job.kind {
        crate::models::JobKind::Transcription => "transcription",
        crate::models::JobKind::Summary => "summary",
        crate::models::JobKind::Insight => "insight",
    }
}

fn job_status_name(job: &Job) -> &'static str {
    match job.status {
        crate::models::JobStatus::Queued => "queued",
        crate::models::JobStatus::Running => "running",
        crate::models::JobStatus::Completed => "completed",
        crate::models::JobStatus::Failed => "failed",
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

    use crate::models::{JobKind, JobStatus, SessionStatus};

    use super::{
        calculate_dir_size, migrate_legacy_state_to_database, open_connection, PersistedState,
        Session, Storage, DATABASE_FILE_NAME,
    };

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

    #[test]
    fn legacy_json_migrates_into_sqlite() {
        let temp_dir = TempDirGuard::new("migrate").expect("temp dir should be created");
        let legacy_path = temp_dir.path().join("state.json");
        let db_path = temp_dir.path().join(DATABASE_FILE_NAME);
        let temp_db_path = temp_dir.path().join("migrate.db.tmp");

        let mut legacy_state = PersistedState::default();
        legacy_state.sessions.insert(
            "session-1".to_string(),
            Session {
                id: "session-1".to_string(),
                created_at: "2024-01-01T00:00:00Z".to_string(),
                updated_at: "2024-01-01T00:00:00Z".to_string(),
                status: SessionStatus::Stopped,
                tags: vec!["#or".to_string()],
                ..Default::default()
            },
        );
        legacy_state.jobs.insert(
            "job-1".to_string(),
            crate::models::Job {
                id: "job-1".to_string(),
                session_id: "session-1".to_string(),
                kind: JobKind::Transcription,
                status: JobStatus::Completed,
                created_at: "2024-01-01T00:00:00Z".to_string(),
                updated_at: "2024-01-01T00:00:00Z".to_string(),
                error: None,
                progress_msg: None,
            },
        );
        fs::write(
            &legacy_path,
            serde_json::to_string(&legacy_state).expect("legacy state should serialize"),
        )
        .expect("legacy state should be written");

        migrate_legacy_state_to_database(&legacy_path, &temp_db_path)
            .expect("legacy migration should succeed");
        fs::rename(&temp_db_path, &db_path).expect("temp db should be finalized");

        let storage = Storage::open_existing(temp_dir.path().to_path_buf(), &db_path)
            .expect("migrated db should open");
        assert!(storage
            .get_session("session-1")
            .expect("session lookup should work")
            .is_some());
        assert!(storage
            .get_job("job-1")
            .expect("job lookup should work")
            .is_some());
    }

    #[test]
    fn save_settings_and_session_persists_both_records() {
        let temp_dir = TempDirGuard::new("txn").expect("temp dir should be created");
        let db_path = temp_dir.path().join(DATABASE_FILE_NAME);
        let connection = open_connection(&db_path).expect("db should open");
        super::initialize_schema(&connection).expect("schema should initialize");
        super::ensure_settings_row(&connection).expect("settings row should exist");

        let mut storage = Storage {
            data_dir: temp_dir.path().to_path_buf(),
            connection,
        };
        let mut settings = storage.get_settings().expect("settings should load");
        settings.recording_segment_seconds = 300;
        let session = Session {
            id: "session-2".to_string(),
            created_at: "2024-01-02T00:00:00Z".to_string(),
            updated_at: "2024-01-02T00:00:00Z".to_string(),
            status: SessionStatus::Stopped,
            tags: vec!["#or".to_string()],
            ..Default::default()
        };

        storage
            .save_settings_and_session(&settings, &session)
            .expect("settings/session transaction should succeed");

        assert_eq!(
            storage
                .get_settings()
                .expect("settings should reload")
                .recording_segment_seconds,
            300
        );
        assert!(storage
            .get_session("session-2")
            .expect("session should reload")
            .is_some());
    }

    #[test]
    fn open_existing_recovers_interrupted_recording_sessions() {
        let temp_dir = TempDirGuard::new("recover").expect("temp dir should be created");
        let db_path = temp_dir.path().join(DATABASE_FILE_NAME);
        let connection = open_connection(&db_path).expect("db should open");
        super::initialize_schema(&connection).expect("schema should initialize");
        super::ensure_settings_row(&connection).expect("settings row should exist");

        let interrupted_recording = Session {
            id: "session-recording".to_string(),
            created_at: "2024-01-03T00:00:00Z".to_string(),
            updated_at: "2024-01-03T00:00:00Z".to_string(),
            status: SessionStatus::Recording,
            tags: vec!["#or".to_string()],
            ..Default::default()
        };
        let interrupted_paused = Session {
            id: "session-paused".to_string(),
            created_at: "2024-01-03T01:00:00Z".to_string(),
            updated_at: "2024-01-03T01:00:00Z".to_string(),
            status: SessionStatus::Paused,
            tags: vec!["#or".to_string()],
            ..Default::default()
        };
        let processing_session = Session {
            id: "session-processing".to_string(),
            created_at: "2024-01-03T02:00:00Z".to_string(),
            updated_at: "2024-01-03T02:00:00Z".to_string(),
            status: SessionStatus::Processing,
            tags: vec!["#or".to_string()],
            ..Default::default()
        };

        super::upsert_session(&connection, &interrupted_recording)
            .expect("recording session should be stored");
        super::upsert_session(&connection, &interrupted_paused)
            .expect("paused session should be stored");
        super::upsert_session(&connection, &processing_session)
            .expect("processing session should be stored");
        drop(connection);

        let storage = Storage::open_existing(temp_dir.path().to_path_buf(), &db_path)
            .expect("existing db should open");

        let recovered_recording = storage
            .get_session("session-recording")
            .expect("recording session should load")
            .expect("recording session should exist");
        assert!(matches!(recovered_recording.status, SessionStatus::Stopped));
        assert_ne!(recovered_recording.updated_at, "2024-01-03T00:00:00Z");

        let recovered_paused = storage
            .get_session("session-paused")
            .expect("paused session should load")
            .expect("paused session should exist");
        assert!(matches!(recovered_paused.status, SessionStatus::Stopped));
        assert_ne!(recovered_paused.updated_at, "2024-01-03T01:00:00Z");

        let untouched_processing = storage
            .get_session("session-processing")
            .expect("processing session should load")
            .expect("processing session should exist");
        assert!(matches!(
            untouched_processing.status,
            SessionStatus::Processing
        ));
        assert_eq!(untouched_processing.updated_at, "2024-01-03T02:00:00Z");
    }
}
