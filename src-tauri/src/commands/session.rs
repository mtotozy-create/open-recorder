use std::{io::ErrorKind, path::Path};

use chrono::Utc;
use tauri::State;
use uuid::Uuid;

use crate::{
    models::{
        merge_session_tags_into_catalog, validate_session_tags, AudioSegmentMeta,
        RecordingQualityPreset, Session, SessionStatus, SessionSummary, StartSessionResponse,
        SummaryResult, DEFAULT_IMPORTED_SESSION_TAG,
    },
    state::AppState,
};

fn now_iso() -> String {
    Utc::now().to_rfc3339()
}

fn infer_extension(file_name: &str, mime_type: Option<&str>) -> &'static str {
    let path = Path::new(file_name);
    if let Some(ext) = path.extension().and_then(|value| value.to_str()) {
        let normalized = ext.to_ascii_lowercase();
        if matches!(
            normalized.as_str(),
            "wav" | "m4a" | "mp3" | "aac" | "flac" | "ogg" | "opus" | "webm" | "mp4" | "m4b"
        ) {
            return match normalized.as_str() {
                "wav" => "wav",
                "m4a" => "m4a",
                "mp3" => "mp3",
                "aac" => "aac",
                "flac" => "flac",
                "ogg" => "ogg",
                "opus" => "opus",
                "webm" => "webm",
                "mp4" => "mp4",
                "m4b" => "m4b",
                _ => "bin",
            };
        }
    }

    match mime_type.unwrap_or("").trim().to_ascii_lowercase().as_str() {
        "audio/wav" | "audio/x-wav" | "audio/wave" => "wav",
        "audio/mpeg" => "mp3",
        "audio/mp4" => "m4a",
        "audio/aac" => "aac",
        "audio/flac" | "audio/x-flac" => "flac",
        "audio/ogg" => "ogg",
        "audio/opus" => "opus",
        "audio/webm" => "webm",
        _ => "bin",
    }
}

fn session_has_merged_audio(session: &Session) -> bool {
    [
        session.exported_m4a_path.as_deref(),
        session.exported_mp3_path.as_deref(),
        session.exported_wav_path.as_deref(),
    ]
    .into_iter()
    .flatten()
    .any(|path| !path.trim().is_empty())
}

fn ensure_single_segment_deletion_allowed(session: &Session) -> Result<(), String> {
    if matches!(session.status, SessionStatus::Processing) {
        return Err(
            "session is still processing segments; deleting audio segments is temporarily unavailable"
                .to_string(),
        );
    }
    Ok(())
}

fn ensure_all_segments_deletion_allowed(session: &Session) -> Result<(), String> {
    ensure_single_segment_deletion_allowed(session)?;
    if !session_has_merged_audio(session) {
        return Err("merged audio file is required before deleting original segments".to_string());
    }
    Ok(())
}

fn remove_audio_file_if_present(path: &str) -> Result<(), String> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!(
            "failed to delete audio segment file {path}: {error}"
        )),
    }
}

fn collect_session_audio_paths(session: &Session) -> Vec<String> {
    let mut paths: Vec<String> = Vec::new();
    for path in &session.audio_segments {
        let normalized = path.trim();
        if !normalized.is_empty() && !paths.iter().any(|existing| existing == normalized) {
            paths.push(normalized.to_string());
        }
    }
    for meta in &session.audio_segment_meta {
        let normalized = meta.path.trim();
        if !normalized.is_empty() && !paths.iter().any(|existing| existing == normalized) {
            paths.push(normalized.to_string());
        }
    }
    paths
}

fn remove_segment_references(session: &mut Session, segment_path: &str) -> bool {
    let before_segments = session.audio_segments.len();
    let before_meta = session.audio_segment_meta.len();
    session.audio_segments.retain(|path| path != segment_path);
    session
        .audio_segment_meta
        .retain(|meta| meta.path != segment_path);
    before_segments != session.audio_segments.len()
        || before_meta != session.audio_segment_meta.len()
}

#[tauri::command]
pub fn session_list(state: State<'_, AppState>) -> Result<Vec<SessionSummary>, String> {
    let storage = state
        .storage
        .lock()
        .map_err(|_| "failed to acquire storage lock".to_string())?;
    storage.list_sessions()
}

#[tauri::command]
pub fn session_get(session_id: String, state: State<'_, AppState>) -> Result<Session, String> {
    let storage = state
        .storage
        .lock()
        .map_err(|_| "failed to acquire storage lock".to_string())?;
    storage
        .get_session(&session_id)?
        .ok_or_else(|| "session not found".to_string())
}

#[tauri::command]
pub fn session_create_from_audio(
    file_name: String,
    audio_bytes: Vec<u8>,
    mime_type: Option<String>,
    duration_ms: Option<u64>,
    state: State<'_, AppState>,
) -> Result<StartSessionResponse, String> {
    if audio_bytes.is_empty() {
        return Err("audio file is empty".to_string());
    }

    let session_id = Uuid::new_v4().to_string();
    let now = now_iso();
    let extension = infer_extension(&file_name, mime_type.as_deref());
    let file_path = {
        let storage = state
            .storage
            .lock()
            .map_err(|_| "failed to acquire storage lock".to_string())?;
        let session_dir = storage.session_audio_dir(&session_id)?;
        let timestamp = Utc::now().format("%Y%m%d-%H%M%S");
        session_dir.join(format!("imported-{timestamp}.{extension}"))
    };

    std::fs::write(&file_path, &audio_bytes)
        .map_err(|error| format!("failed to write imported audio file: {error}"))?;

    let saved_path = file_path.to_string_lossy().to_string();
    let file_size = file_path
        .metadata()
        .map(|metadata| metadata.len())
        .unwrap_or(audio_bytes.len() as u64);
    let format = file_path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|value| value.to_ascii_lowercase())
        .unwrap_or_else(|| "bin".to_string());

    let mut storage = state
        .storage
        .lock()
        .map_err(|_| "failed to acquire storage lock".to_string())?;
    let mut settings = storage.get_settings()?;
    let default_tags = vec![DEFAULT_IMPORTED_SESSION_TAG.to_string()];
    let session = Session {
        id: session_id.clone(),
        name: None,
        discoverable: true,
        status: SessionStatus::Stopped,
        created_at: now.clone(),
        updated_at: now.clone(),
        input_device_id: None,
        audio_segments: vec![saved_path.clone()],
        audio_segment_meta: vec![AudioSegmentMeta {
            path: saved_path,
            sequence: 0,
            started_at: now.clone(),
            ended_at: now,
            duration_ms: duration_ms.unwrap_or(0),
            sample_rate: 0,
            channels: 0,
            format,
            file_size_bytes: file_size,
        }],
        quality_preset: RecordingQualityPreset::Standard,
        sample_rate: 0,
        channels: 0,
        elapsed_ms: duration_ms.unwrap_or(0),
        tags: default_tags.clone(),
        exported_wav_path: None,
        exported_wav_size: None,
        exported_wav_created_at: None,
        exported_mp3_path: None,
        exported_mp3_size: None,
        exported_mp3_created_at: None,
        exported_m4a_path: None,
        exported_m4a_size: None,
        exported_m4a_created_at: None,
        transcript: vec![],
        summary: None,
    };
    merge_session_tags_into_catalog(
        &mut settings.session_tag_catalog,
        &default_tags,
    );
    settings.normalize();
    storage.save_settings_and_session(&settings, &session)?;
    Ok(StartSessionResponse {
        session_id,
        input_device_id: None,
        input_device_name: None,
        fallback_from_input_device_id: None,
    })
}

#[tauri::command]
pub fn session_rename(
    session_id: String,
    name: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let storage = state
        .storage
        .lock()
        .map_err(|_| "failed to acquire storage lock".to_string())?;
    let mut session = storage
        .get_session(&session_id)?
        .ok_or_else(|| "session not found".to_string())?;
    let normalized = name.trim();
    session.name = if normalized.is_empty() {
        None
    } else {
        Some(normalized.to_string())
    };
    session.updated_at = now_iso();
    storage.upsert_session(&session)?;
    Ok(())
}

#[tauri::command]
pub fn session_delete(session_id: String, state: State<'_, AppState>) -> Result<(), String> {
    let mut storage = state
        .storage
        .lock()
        .map_err(|_| "failed to acquire storage lock".to_string())?;
    let session = storage
        .delete_session_and_jobs(&session_id)?
        .ok_or_else(|| "session not found".to_string())?;

    for path in session.audio_segments {
        let _ = std::fs::remove_file(&path);
    }
    Ok(())
}

#[tauri::command]
pub fn session_delete_segment(
    session_id: String,
    segment_path: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let target_path = segment_path.trim();
    if target_path.is_empty() {
        return Err("segment path is required".to_string());
    }

    let storage = state
        .storage
        .lock()
        .map_err(|_| "failed to acquire storage lock".to_string())?;
    let mut session = storage
        .get_session(&session_id)?
        .ok_or_else(|| "session not found".to_string())?;

    ensure_single_segment_deletion_allowed(&session)?;
    if !remove_segment_references(&mut session, target_path) {
        return Err("audio segment not found".to_string());
    }

    remove_audio_file_if_present(target_path)?;
    session.updated_at = now_iso();
    storage.upsert_session(&session)?;
    Ok(())
}

#[tauri::command]
pub fn session_delete_segments(
    session_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let storage = state
        .storage
        .lock()
        .map_err(|_| "failed to acquire storage lock".to_string())?;
    let mut session = storage
        .get_session(&session_id)?
        .ok_or_else(|| "session not found".to_string())?;

    ensure_all_segments_deletion_allowed(&session)?;

    let segment_paths = collect_session_audio_paths(&session);
    if segment_paths.is_empty() {
        return Ok(());
    }

    for path in &segment_paths {
        remove_audio_file_if_present(path)?;
    }

    session.audio_segments.clear();
    session.audio_segment_meta.clear();
    session.updated_at = now_iso();
    storage.upsert_session(&session)?;
    Ok(())
}

#[tauri::command]
pub fn session_set_tags(
    session_id: String,
    tags: Vec<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let normalized_tags = validate_session_tags(&tags)?;
    let mut storage = state
        .storage
        .lock()
        .map_err(|_| "failed to acquire storage lock".to_string())?;
    let mut session = storage
        .get_session(&session_id)?
        .ok_or_else(|| "session not found".to_string())?;
    let mut settings = storage.get_settings()?;
    session.tags = normalized_tags.clone();
    session.updated_at = now_iso();

    merge_session_tags_into_catalog(
        &mut settings.session_tag_catalog,
        &normalized_tags,
    );
    settings.normalize();
    storage.save_settings_and_session(&settings, &session)?;
    Ok(())
}

#[tauri::command]
pub fn session_set_discoverable(
    session_id: String,
    discoverable: bool,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let storage = state
        .storage
        .lock()
        .map_err(|_| "failed to acquire storage lock".to_string())?;
    let mut session = storage
        .get_session(&session_id)?
        .ok_or_else(|| "session not found".to_string())?;
    session.discoverable = discoverable;
    session.updated_at = now_iso();
    storage.upsert_session(&session)?;
    Ok(())
}

#[tauri::command]
pub fn session_update_summary_raw_markdown(
    session_id: String,
    raw_markdown: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let storage = state
        .storage
        .lock()
        .map_err(|_| "failed to acquire storage lock".to_string())?;
    let mut session = storage
        .get_session(&session_id)?
        .ok_or_else(|| "session not found".to_string())?;

    if let Some(summary) = session.summary.as_mut() {
        summary.raw_markdown = raw_markdown;
    } else {
        session.summary = Some(SummaryResult {
            title: "Manual Summary".to_string(),
            decisions: vec![],
            action_items: vec![],
            risks: vec![],
            timeline: vec![],
            raw_markdown,
        });
    }

    session.updated_at = now_iso();
    storage.upsert_session(&session)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        collect_session_audio_paths, ensure_all_segments_deletion_allowed,
        ensure_single_segment_deletion_allowed, remove_segment_references, session_has_merged_audio,
    };
    use crate::models::{AudioSegmentMeta, Session, SessionStatus};

    fn build_session() -> Session {
        Session {
            exported_m4a_path: Some("/tmp/merged.m4a".to_string()),
            audio_segments: vec![
                "/tmp/segment-a.m4a".to_string(),
                "/tmp/segment-b.m4a".to_string(),
            ],
            audio_segment_meta: vec![
                AudioSegmentMeta {
                    path: "/tmp/segment-a.m4a".to_string(),
                    sequence: 0,
                    ..Default::default()
                },
                AudioSegmentMeta {
                    path: "/tmp/segment-b.m4a".to_string(),
                    sequence: 1,
                    ..Default::default()
                },
            ],
            ..Default::default()
        }
    }

    #[test]
    fn merged_audio_is_detected_from_any_exported_file() {
        let mut session = Session::default();
        assert!(!session_has_merged_audio(&session));

        session.exported_mp3_path = Some("/tmp/test.mp3".to_string());
        assert!(session_has_merged_audio(&session));
    }

    #[test]
    fn deleting_all_segments_requires_merged_audio() {
        let session = Session::default();
        assert_eq!(
            ensure_all_segments_deletion_allowed(&session).unwrap_err(),
            "merged audio file is required before deleting original segments"
        );
    }

    #[test]
    fn deleting_single_segment_is_allowed_without_merged_audio() {
        let session = Session::default();
        assert!(ensure_single_segment_deletion_allowed(&session).is_ok());
    }

    #[test]
    fn segment_deletion_is_blocked_while_processing_for_single_delete() {
        let mut session = build_session();
        session.status = SessionStatus::Processing;
        assert_eq!(
            ensure_single_segment_deletion_allowed(&session).unwrap_err(),
            "session is still processing segments; deleting audio segments is temporarily unavailable"
        );
    }

    #[test]
    fn segment_deletion_is_blocked_while_processing_for_delete_all() {
        let mut session = build_session();
        session.status = SessionStatus::Processing;
        assert_eq!(
            ensure_all_segments_deletion_allowed(&session).unwrap_err(),
            "session is still processing segments; deleting audio segments is temporarily unavailable"
        );
    }

    #[test]
    fn remove_segment_references_clears_matching_segment_and_meta() {
        let mut session = build_session();
        let removed = remove_segment_references(&mut session, "/tmp/segment-a.m4a");

        assert!(removed);
        assert_eq!(
            session.audio_segments,
            vec!["/tmp/segment-b.m4a".to_string()]
        );
        assert_eq!(session.audio_segment_meta.len(), 1);
        assert_eq!(session.audio_segment_meta[0].path, "/tmp/segment-b.m4a");
    }

    #[test]
    fn collect_session_audio_paths_deduplicates_segment_paths() {
        let mut session = build_session();
        session
            .audio_segments
            .push("/tmp/segment-a.m4a".to_string());
        session.audio_segment_meta.push(AudioSegmentMeta {
            path: "/tmp/segment-b.m4a".to_string(),
            sequence: 2,
            ..Default::default()
        });

        assert_eq!(
            collect_session_audio_paths(&session),
            vec![
                "/tmp/segment-a.m4a".to_string(),
                "/tmp/segment-b.m4a".to_string()
            ]
        );
    }
}
