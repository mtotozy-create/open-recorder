use std::{
    collections::HashSet,
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
};

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
    match fs::remove_file(path) {
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

#[derive(Debug, Clone)]
struct SelectedSessionSegment {
    source_path: String,
    meta: AudioSegmentMeta,
}

fn infer_audio_format_from_path(path: &str) -> String {
    Path::new(path)
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "bin".to_string())
}

fn normalize_segment_path_inputs(segment_paths: &[String]) -> Vec<String> {
    let mut normalized = Vec::new();
    let mut seen = HashSet::new();

    for raw_path in segment_paths {
        let path = raw_path.trim();
        if path.is_empty() || !seen.insert(path.to_string()) {
            continue;
        }
        normalized.push(path.to_string());
    }

    normalized
}

fn collect_selected_session_segments(
    session: &Session,
    requested_paths: &[String],
) -> Result<Vec<SelectedSessionSegment>, String> {
    let normalized_paths = normalize_segment_path_inputs(requested_paths);
    if normalized_paths.is_empty() {
        return Err("at least one audio segment must be selected".to_string());
    }

    let requested = normalized_paths
        .iter()
        .cloned()
        .collect::<HashSet<String>>();
    let mut matched = HashSet::new();
    let mut selected = Vec::new();
    let mut seen_source_paths = HashSet::new();

    for meta in &session.audio_segment_meta {
        let path = meta.path.trim();
        if path.is_empty() || !seen_source_paths.insert(path.to_string()) {
            continue;
        }
        if requested.contains(path) {
            matched.insert(path.to_string());
            selected.push(SelectedSessionSegment {
                source_path: path.to_string(),
                meta: meta.clone(),
            });
        }
    }

    for raw_path in &session.audio_segments {
        let path = raw_path.trim();
        if path.is_empty() || !seen_source_paths.insert(path.to_string()) {
            continue;
        }
        if requested.contains(path) {
            matched.insert(path.to_string());
            selected.push(SelectedSessionSegment {
                source_path: path.to_string(),
                meta: AudioSegmentMeta {
                    path: path.to_string(),
                    format: infer_audio_format_from_path(path),
                    ..Default::default()
                },
            });
        }
    }

    if matched.len() != requested.len() {
        let missing = normalized_paths
            .into_iter()
            .find(|path| !matched.contains(path))
            .unwrap_or_else(|| "unknown".to_string());
        return Err(format!(
            "audio segment not found in source session: {missing}"
        ));
    }

    Ok(selected)
}

fn resolve_selected_segment_extension(segment: &SelectedSessionSegment) -> String {
    let format = segment.meta.format.trim().to_ascii_lowercase();
    if !format.is_empty() {
        return format;
    }
    infer_audio_format_from_path(&segment.source_path)
}

fn copy_selected_segments_to_dir(
    session_dir: &Path,
    selected_segments: &[SelectedSessionSegment],
) -> Result<Vec<AudioSegmentMeta>, String> {
    fs::create_dir_all(session_dir).map_err(|error| {
        format!(
            "failed to create session audio dir {}: {error}",
            session_dir.display()
        )
    })?;

    let mut copied_segments = Vec::with_capacity(selected_segments.len());
    for (index, segment) in selected_segments.iter().enumerate() {
        let extension = resolve_selected_segment_extension(segment);
        let destination_path = session_dir.join(format!("segment-{index:04}.{extension}"));
        fs::copy(&segment.source_path, &destination_path).map_err(|error| {
            format!(
                "failed to copy audio segment {} to {}: {error}",
                segment.source_path,
                destination_path.display()
            )
        })?;

        let file_size_bytes = destination_path
            .metadata()
            .map(|metadata| metadata.len())
            .unwrap_or(segment.meta.file_size_bytes);
        let mut copied_meta = segment.meta.clone();
        copied_meta.path = destination_path.to_string_lossy().to_string();
        copied_meta.sequence = index as u32;
        copied_meta.format = extension;
        copied_meta.file_size_bytes = file_size_bytes;
        copied_segments.push(copied_meta);
    }

    Ok(copied_segments)
}

fn build_imported_session_from_segments(
    session_id: String,
    source_session: &Session,
    audio_segment_meta: Vec<AudioSegmentMeta>,
    now: &str,
) -> Session {
    let elapsed_ms = audio_segment_meta.iter().map(|meta| meta.duration_ms).sum();
    let sample_rate = audio_segment_meta
        .iter()
        .find_map(|meta| (meta.sample_rate > 0).then_some(meta.sample_rate))
        .unwrap_or(source_session.sample_rate);
    let channels = audio_segment_meta
        .iter()
        .find_map(|meta| (meta.channels > 0).then_some(meta.channels))
        .unwrap_or(source_session.channels);
    let audio_segments = audio_segment_meta
        .iter()
        .map(|meta| meta.path.clone())
        .collect::<Vec<String>>();

    Session {
        id: session_id,
        name: None,
        discoverable: true,
        status: SessionStatus::Stopped,
        created_at: now.to_string(),
        updated_at: now.to_string(),
        input_device_id: None,
        audio_segments,
        audio_segment_meta,
        quality_preset: source_session.quality_preset.clone(),
        sample_rate,
        channels,
        elapsed_ms,
        tags: vec![DEFAULT_IMPORTED_SESSION_TAG.to_string()],
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
    }
}

fn remove_dir_if_present(path: &Path) {
    match fs::remove_dir_all(path) {
        Ok(()) => {}
        Err(error) if error.kind() == ErrorKind::NotFound => {}
        Err(error) => {
            eprintln!(
                "[session] failed to clean up temporary session dir {}: {error}",
                path.display()
            );
        }
    }
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
    merge_session_tags_into_catalog(&mut settings.session_tag_catalog, &default_tags);
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
pub fn session_create_from_segments(
    session_id: String,
    segment_paths: Vec<String>,
    state: State<'_, AppState>,
) -> Result<StartSessionResponse, String> {
    let new_session_id = Uuid::new_v4().to_string();
    let now = now_iso();
    let (source_session, session_dir, cleanup_dir): (Session, PathBuf, PathBuf) = {
        let storage = state
            .storage
            .lock()
            .map_err(|_| "failed to acquire storage lock".to_string())?;
        let source_session = storage
            .get_session(&session_id)?
            .ok_or_else(|| "session not found".to_string())?;
        let session_dir = storage.session_audio_dir(&new_session_id)?;
        let cleanup_dir = session_dir
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| session_dir.clone());
        (source_session, session_dir, cleanup_dir)
    };

    let selected_segments = match collect_selected_session_segments(&source_session, &segment_paths)
    {
        Ok(selected_segments) => selected_segments,
        Err(error) => {
            remove_dir_if_present(&cleanup_dir);
            return Err(error);
        }
    };
    let copied_segments = match copy_selected_segments_to_dir(&session_dir, &selected_segments) {
        Ok(copied_segments) => copied_segments,
        Err(error) => {
            remove_dir_if_present(&cleanup_dir);
            return Err(error);
        }
    };
    let session = build_imported_session_from_segments(
        new_session_id.clone(),
        &source_session,
        copied_segments,
        &now,
    );

    let save_result = {
        let mut storage = state
            .storage
            .lock()
            .map_err(|_| "failed to acquire storage lock".to_string())?;
        let mut settings = storage.get_settings()?;
        merge_session_tags_into_catalog(&mut settings.session_tag_catalog, &session.tags);
        settings.normalize();
        storage.save_settings_and_session(&settings, &session)
    };
    if let Err(error) = save_result {
        remove_dir_if_present(&cleanup_dir);
        return Err(error);
    }

    Ok(StartSessionResponse {
        session_id: new_session_id,
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

    merge_session_tags_into_catalog(&mut settings.session_tag_catalog, &normalized_tags);
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
        build_imported_session_from_segments, collect_selected_session_segments,
        collect_session_audio_paths, copy_selected_segments_to_dir,
        ensure_all_segments_deletion_allowed, ensure_single_segment_deletion_allowed,
        remove_segment_references, session_has_merged_audio,
    };
    use crate::models::{
        AudioSegmentMeta, Session, SessionStatus, SummaryResult, TranscriptSegment,
    };
    use std::{
        fs,
        path::{Path, PathBuf},
    };
    use uuid::Uuid;

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

    #[test]
    fn selected_segments_follow_source_session_order_and_ignore_duplicate_requests() {
        let session = build_session();

        let selected = collect_selected_session_segments(
            &session,
            &[
                "/tmp/segment-b.m4a".to_string(),
                "/tmp/segment-a.m4a".to_string(),
                "/tmp/segment-b.m4a".to_string(),
            ],
        )
        .expect("selected segments should be collected");

        assert_eq!(selected.len(), 2);
        assert_eq!(selected[0].source_path, "/tmp/segment-a.m4a");
        assert_eq!(selected[1].source_path, "/tmp/segment-b.m4a");
    }

    #[test]
    fn selecting_unknown_segment_path_returns_error() {
        let session = build_session();

        assert_eq!(
            collect_selected_session_segments(&session, &["/tmp/missing.m4a".to_string()])
                .unwrap_err(),
            "audio segment not found in source session: /tmp/missing.m4a"
        );
    }

    #[test]
    fn copied_segments_are_reindexed_and_written_to_destination() {
        let source_dir = create_test_dir("source");
        let target_dir = create_test_dir("target");
        let source_path_a = source_dir.join("segment-a.m4a");
        let source_path_b = source_dir.join("segment-b.m4a");
        fs::write(&source_path_a, b"aaa").expect("segment a should be written");
        fs::write(&source_path_b, b"bbbb").expect("segment b should be written");

        let session = Session {
            audio_segments: vec![
                source_path_a.to_string_lossy().to_string(),
                source_path_b.to_string_lossy().to_string(),
            ],
            audio_segment_meta: vec![
                AudioSegmentMeta {
                    path: source_path_a.to_string_lossy().to_string(),
                    sequence: 4,
                    duration_ms: 1200,
                    sample_rate: 44100,
                    channels: 2,
                    format: "m4a".to_string(),
                    file_size_bytes: 3,
                    ..Default::default()
                },
                AudioSegmentMeta {
                    path: source_path_b.to_string_lossy().to_string(),
                    sequence: 8,
                    duration_ms: 2400,
                    sample_rate: 44100,
                    channels: 2,
                    format: "m4a".to_string(),
                    file_size_bytes: 4,
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        let selected = collect_selected_session_segments(&session, &session.audio_segments)
            .expect("selection should work");

        let copied = copy_selected_segments_to_dir(&target_dir, &selected)
            .expect("selected segments should be copied");

        assert_eq!(copied.len(), 2);
        assert_eq!(copied[0].sequence, 0);
        assert_eq!(copied[1].sequence, 1);
        assert!(Path::new(&copied[0].path).exists());
        assert!(Path::new(&copied[1].path).exists());
        assert_eq!(fs::read(&copied[0].path).expect("copied segment a"), b"aaa");
        assert_eq!(
            fs::read(&copied[1].path).expect("copied segment b"),
            b"bbbb"
        );

        remove_test_dir(&source_dir);
        remove_test_dir(&target_dir);
    }

    #[test]
    fn imported_session_from_segments_resets_transcript_summary_and_exports() {
        let source_session = Session {
            quality_preset: crate::models::RecordingQualityPreset::Hd,
            sample_rate: 48000,
            channels: 1,
            transcript: vec![TranscriptSegment {
                start_ms: 0,
                end_ms: 1000,
                text: "hello".to_string(),
                translation_text: None,
                translation_target_language: None,
                confidence: None,
                speaker_id: None,
                speaker_label: None,
            }],
            summary: Some(SummaryResult {
                title: "Summary".to_string(),
                decisions: vec!["Decision".to_string()],
                action_items: vec![],
                risks: vec![],
                timeline: vec![],
                raw_markdown: "Summary".to_string(),
            }),
            ..Default::default()
        };
        let copied_segments = vec![
            AudioSegmentMeta {
                path: "/tmp/copied-a.m4a".to_string(),
                sequence: 0,
                duration_ms: 1000,
                sample_rate: 44100,
                channels: 2,
                format: "m4a".to_string(),
                ..Default::default()
            },
            AudioSegmentMeta {
                path: "/tmp/copied-b.m4a".to_string(),
                sequence: 1,
                duration_ms: 2500,
                sample_rate: 44100,
                channels: 2,
                format: "m4a".to_string(),
                ..Default::default()
            },
        ];

        let session = build_imported_session_from_segments(
            "derived-session".to_string(),
            &source_session,
            copied_segments,
            "2026-04-13T12:00:00Z",
        );

        assert_eq!(session.id, "derived-session");
        assert!(matches!(session.status, SessionStatus::Stopped));
        assert_eq!(session.elapsed_ms, 3500);
        assert_eq!(session.sample_rate, 44100);
        assert_eq!(session.channels, 2);
        assert_eq!(session.tags, vec!["#imported".to_string()]);
        assert!(session.transcript.is_empty());
        assert!(session.summary.is_none());
        assert!(session.exported_m4a_path.is_none());
        assert!(session.exported_mp3_path.is_none());
        assert!(session.exported_wav_path.is_none());
    }

    fn create_test_dir(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "open-recorder-session-tests-{label}-{}",
            Uuid::new_v4()
        ));
        fs::create_dir_all(&dir).expect("test dir should be created");
        dir
    }

    fn remove_test_dir(path: &Path) {
        let _ = fs::remove_dir_all(path);
    }
}
