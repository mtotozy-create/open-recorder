use std::path::Path;

use chrono::Utc;
use tauri::State;
use uuid::Uuid;

use crate::{
    models::{
        merge_session_tags_into_catalog, validate_session_tags, AudioSegmentMeta,
        RecordingQualityPreset, Session, SessionStatus, SessionSummary, StartSessionResponse,
        DEFAULT_IMPORTED_SESSION_TAG,
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
    let default_tags = vec![DEFAULT_IMPORTED_SESSION_TAG.to_string()];

    storage.data.sessions.insert(
        session_id.clone(),
        Session {
            id: session_id.clone(),
            name: None,
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
        },
    );
    merge_session_tags_into_catalog(
        &mut storage.data.settings.session_tag_catalog,
        &default_tags,
    );
    storage.data.settings.normalize();

    storage.save()?;
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
    storage
        .data
        .jobs
        .retain(|_, job| job.session_id != session_id);

    storage.save()?;
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

    {
        let session = storage
            .data
            .sessions
            .get_mut(&session_id)
            .ok_or_else(|| "session not found".to_string())?;
        session.tags = normalized_tags.clone();
        session.updated_at = now_iso();
    }

    merge_session_tags_into_catalog(
        &mut storage.data.settings.session_tag_catalog,
        &normalized_tags,
    );
    storage.data.settings.normalize();
    storage.save()?;
    Ok(())
}
