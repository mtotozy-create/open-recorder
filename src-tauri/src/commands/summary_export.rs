use std::{
    collections::HashMap,
    env, fs,
    path::{Path, PathBuf},
};

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::{models::Session, state::AppState};

const SUMMARY_EXPORT_PROGRESS_EVENT: &str = "summary-export-progress";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SummaryMarkdownExportProgress {
    pub total_sessions: usize,
    pub processed_sessions: usize,
    pub exported_count: usize,
    pub skipped_existing_count: usize,
    pub skipped_empty_count: usize,
    pub current_session_name: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SummaryMarkdownExportResult {
    pub total_sessions: usize,
    pub summary_sessions: usize,
    pub exported_count: usize,
    pub skipped_existing_count: usize,
    pub skipped_empty_count: usize,
    pub folder_path: String,
}

#[derive(Debug, Clone)]
struct ExportCounters {
    total_sessions: usize,
    processed_sessions: usize,
    exported_count: usize,
    skipped_existing_count: usize,
    skipped_empty_count: usize,
}

impl ExportCounters {
    fn to_progress(&self, current_session_name: Option<String>) -> SummaryMarkdownExportProgress {
        SummaryMarkdownExportProgress {
            total_sessions: self.total_sessions,
            processed_sessions: self.processed_sessions,
            exported_count: self.exported_count,
            skipped_existing_count: self.skipped_existing_count,
            skipped_empty_count: self.skipped_empty_count,
            current_session_name,
        }
    }
}

#[tauri::command]
pub fn summary_export_all_markdown(
    folder_path: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<SummaryMarkdownExportResult, String> {
    let sessions = {
        let storage = state
            .storage
            .lock()
            .map_err(|_| "failed to acquire storage lock".to_string())?;
        storage.list_all_sessions()?
    };

    export_sessions_to_markdown(sessions, &folder_path, |progress| {
        let _ = app.emit(SUMMARY_EXPORT_PROGRESS_EVENT, progress);
    })
}

fn export_sessions_to_markdown<F>(
    mut sessions: Vec<Session>,
    folder_path: &str,
    emit_progress: F,
) -> Result<SummaryMarkdownExportResult, String>
where
    F: Fn(&SummaryMarkdownExportProgress),
{
    let folder = resolve_export_folder(folder_path)?;
    ensure_export_folder(&folder)?;

    sessions.sort_by(|left, right| {
        left.created_at
            .cmp(&right.created_at)
            .then_with(|| left.id.cmp(&right.id))
    });

    let mut counters = ExportCounters {
        total_sessions: sessions.len(),
        processed_sessions: 0,
        exported_count: 0,
        skipped_existing_count: 0,
        skipped_empty_count: 0,
    };
    emit_progress(&counters.to_progress(None));

    let mut base_name_counts: HashMap<String, usize> = HashMap::new();

    for session in sessions {
        let current_session_name = display_session_name(&session);
        let raw_markdown = session
            .summary
            .as_ref()
            .map(|summary| summary.raw_markdown.as_str())
            .unwrap_or("");
        let markdown = normalize_summary_markdown(raw_markdown);

        if markdown.trim().is_empty() {
            counters.skipped_empty_count += 1;
            counters.processed_sessions += 1;
            emit_progress(&counters.to_progress(Some(current_session_name)));
            continue;
        }

        let file_name = resolve_markdown_file_name(&session, &mut base_name_counts);
        let file_path = folder.join(file_name);
        if file_path.exists() {
            counters.skipped_existing_count += 1;
            counters.processed_sessions += 1;
            emit_progress(&counters.to_progress(Some(current_session_name)));
            continue;
        }

        fs::write(&file_path, markdown.as_bytes()).map_err(|error| {
            format!(
                "failed to write summary markdown {}: {error}",
                file_path.display()
            )
        })?;
        counters.exported_count += 1;
        counters.processed_sessions += 1;
        emit_progress(&counters.to_progress(Some(current_session_name)));
    }

    Ok(SummaryMarkdownExportResult {
        total_sessions: counters.total_sessions,
        summary_sessions: counters
            .total_sessions
            .saturating_sub(counters.skipped_empty_count),
        exported_count: counters.exported_count,
        skipped_existing_count: counters.skipped_existing_count,
        skipped_empty_count: counters.skipped_empty_count,
        folder_path: folder.to_string_lossy().to_string(),
    })
}

fn resolve_export_folder(raw_path: &str) -> Result<PathBuf, String> {
    let trimmed = raw_path.trim();
    if trimmed.is_empty() {
        return Err("summary export folder is required".to_string());
    }

    if trimmed == "~" {
        return resolve_home_dir();
    }

    if let Some(rest) = trimmed.strip_prefix("~/") {
        return Ok(resolve_home_dir()?.join(rest));
    }

    Ok(PathBuf::from(trimmed))
}

fn resolve_home_dir() -> Result<PathBuf, String> {
    env::var_os("HOME")
        .or_else(|| env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
        .ok_or_else(|| "failed to resolve home directory".to_string())
}

fn ensure_export_folder(folder: &Path) -> Result<(), String> {
    if folder.exists() {
        if folder.is_dir() {
            return Ok(());
        }
        return Err(format!(
            "summary export path is not a folder: {}",
            folder.display()
        ));
    }

    let parent = folder.parent().filter(|path| !path.as_os_str().is_empty());
    if let Some(parent) = parent {
        if !parent.is_dir() {
            return Err(format!(
                "summary export parent folder does not exist: {}",
                parent.display()
            ));
        }
    }

    fs::create_dir(folder).map_err(|error| {
        format!(
            "failed to create summary export folder {}: {error}",
            folder.display()
        )
    })
}

fn display_session_name(session: &Session) -> String {
    session
        .name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| format!("session-{}", short_session_id(&session.id)))
}

fn resolve_markdown_file_name(
    session: &Session,
    base_name_counts: &mut HashMap<String, usize>,
) -> String {
    let base_name = sanitize_file_name_segment(&display_session_name(session))
        .unwrap_or_else(|| format!("session-{}", short_session_id(&session.id)));
    let count = base_name_counts.entry(base_name.clone()).or_insert(0);
    let file_stem = if *count == 0 {
        base_name.clone()
    } else {
        format!("{}-{}", base_name, short_session_id(&session.id))
    };
    *count += 1;
    format!("{file_stem}.md")
}

fn short_session_id(session_id: &str) -> String {
    let trimmed = session_id.trim();
    if trimmed.is_empty() {
        return "unknown".to_string();
    }
    trimmed.chars().take(8).collect()
}

fn sanitize_file_name_segment(input: &str) -> Option<String> {
    let mut output = String::new();
    let mut previous_was_space = false;

    for character in input.chars() {
        let invalid = matches!(
            character,
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*'
        ) || character.is_control();
        let next = if invalid { ' ' } else { character };
        if next.is_whitespace() {
            if !previous_was_space {
                output.push(' ');
                previous_was_space = true;
            }
        } else {
            output.push(next);
            previous_was_space = false;
        }
    }

    let mut sanitized = output.trim().trim_matches('.').trim().to_string();
    if sanitized.to_ascii_lowercase().ends_with(".md") {
        let next_len = sanitized.len().saturating_sub(3);
        sanitized.truncate(next_len);
        sanitized = sanitized.trim().trim_matches('.').trim().to_string();
    }

    if sanitized.chars().count() > 80 {
        sanitized = sanitized
            .chars()
            .take(80)
            .collect::<String>()
            .trim()
            .to_string();
    }

    if sanitized.is_empty() {
        None
    } else {
        Some(sanitized)
    }
}

fn normalize_summary_markdown(raw: &str) -> String {
    let without_placeholder_lines = raw
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            trimmed != "*"
        })
        .collect::<Vec<_>>()
        .join("\n");

    let mut normalized = String::with_capacity(without_placeholder_lines.len());
    let mut newline_count = 0usize;
    for character in without_placeholder_lines.chars() {
        if character == '\n' {
            newline_count += 1;
            if newline_count <= 2 {
                normalized.push(character);
            }
        } else {
            newline_count = 0;
            normalized.push(character);
        }
    }
    normalized
}

#[cfg(test)]
mod tests {
    use super::{
        export_sessions_to_markdown, normalize_summary_markdown, sanitize_file_name_segment,
    };
    use crate::models::{Session, SummaryResult};
    use std::{env, fs, path::Path};
    use uuid::Uuid;

    fn session(id: &str, name: Option<&str>, raw_markdown: &str, created_at: &str) -> Session {
        Session {
            id: id.to_string(),
            name: name.map(str::to_string),
            created_at: created_at.to_string(),
            summary: Some(SummaryResult {
                title: String::new(),
                decisions: vec![],
                action_items: vec![],
                risks: vec![],
                timeline: vec![],
                raw_markdown: raw_markdown.to_string(),
            }),
            ..Default::default()
        }
    }

    #[test]
    fn normalizes_summary_markdown_like_readable_view() {
        assert_eq!(
            normalize_summary_markdown("# Title\n\n\n*\n\nBody"),
            "# Title\n\nBody"
        );
    }

    #[test]
    fn sanitizes_file_name_segments() {
        assert_eq!(
            sanitize_file_name_segment(" Team/Meeting: Notes.md "),
            Some("Team Meeting Notes".to_string())
        );
        assert_eq!(sanitize_file_name_segment("..."), None);
    }

    #[test]
    fn exports_markdown_and_skips_existing_files() {
        let temp_dir = env::temp_dir().join(format!(
            "open-recorder-summary-export-test-{}",
            Uuid::new_v4()
        ));
        fs::create_dir_all(&temp_dir).expect("temp dir should be created");

        let sessions = vec![
            session(
                "11111111-aaaa",
                Some("Daily Sync"),
                "# First\n\n\nBody",
                "2026-01-01T00:00:00Z",
            ),
            session(
                "22222222-bbbb",
                Some("Daily Sync"),
                "# Second",
                "2026-01-02T00:00:00Z",
            ),
            session(
                "33333333-cccc",
                Some("Empty"),
                "   ",
                "2026-01-03T00:00:00Z",
            ),
        ];

        let result = export_sessions_to_markdown(
            sessions.clone(),
            temp_dir.to_string_lossy().as_ref(),
            |_| {},
        )
        .expect("export should succeed");
        assert_eq!(result.exported_count, 2);
        assert_eq!(result.skipped_empty_count, 1);
        assert_eq!(
            fs::read_to_string(temp_dir.join("Daily Sync.md")).unwrap(),
            "# First\n\nBody"
        );
        assert!(Path::new(&temp_dir.join("Daily Sync-22222222.md")).exists());

        let result =
            export_sessions_to_markdown(sessions, temp_dir.to_string_lossy().as_ref(), |_| {})
                .expect("second export should succeed");
        assert_eq!(result.exported_count, 0);
        assert_eq!(result.skipped_existing_count, 2);

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
