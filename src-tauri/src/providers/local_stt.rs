use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::models::{AudioSegmentMeta, LocalSttEngine, TranscriptSegment};

const WORKER_SCRIPT_SOURCE: &str = include_str!("../../python/local_stt_worker.py");

#[derive(Debug, Clone)]
pub struct LocalSttConfig {
    pub python_path: Option<String>,
    pub venv_dir: Option<String>,
    pub model_cache_dir: Option<String>,
    pub engine: LocalSttEngine,
    pub whisper_model: String,
    pub sense_voice_model: String,
    pub language: String,
    pub diarization_enabled: bool,
    pub min_speakers: Option<u32>,
    pub max_speakers: Option<u32>,
    pub speaker_count_hint: Option<u32>,
    pub compute_device: String,
    pub vad_enabled: bool,
    pub chunk_seconds: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct WorkerSegmentMeta {
    path: String,
    start_ms: u64,
    end_ms: u64,
    duration_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct WorkerRequest {
    session_id: String,
    audio_paths: Vec<String>,
    segment_meta: Vec<WorkerSegmentMeta>,
    engine: LocalSttEngine,
    whisper_model: String,
    sense_voice_model: String,
    language: String,
    diarization_enabled: bool,
    min_speakers: Option<u32>,
    max_speakers: Option<u32>,
    speaker_count_hint: Option<u32>,
    compute_device: String,
    vad_enabled: bool,
    chunk_seconds: u64,
    model_cache_dir: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WorkerSegment {
    start_ms: u64,
    end_ms: u64,
    text: String,
    confidence: Option<f32>,
    speaker_id: Option<String>,
    speaker_label: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WorkerResponse {
    segments: Vec<WorkerSegment>,
    error: Option<String>,
    warning: Option<String>,
}

pub fn ensure_worker_script_on_disk() -> Result<PathBuf, String> {
    let worker_dir = std::env::temp_dir()
        .join("open-recorder-local-stt")
        .join("worker");
    fs::create_dir_all(&worker_dir)
        .map_err(|error| format!("failed to create local stt worker dir: {error}"))?;
    let worker_path = worker_dir.join("local_stt_worker.py");
    fs::write(&worker_path, WORKER_SCRIPT_SOURCE.as_bytes())
        .map_err(|error| format!("failed to write local stt worker script: {error}"))?;
    Ok(worker_path)
}

#[derive(Debug, Clone)]
pub(crate) struct PythonCommand {
    program: String,
    args: Vec<String>,
}

impl PythonCommand {
    fn new(program: impl Into<String>, args: Vec<String>) -> Self {
        Self {
            program: program.into(),
            args,
        }
    }

    pub(crate) fn display(&self) -> String {
        if self.args.is_empty() {
            self.program.clone()
        } else {
            format!("{} {}", self.program, self.args.join(" "))
        }
    }

    pub(crate) fn to_command(&self) -> Command {
        let mut command = Command::new(&self.program);
        command.args(&self.args);
        command
    }

    pub(crate) fn is_available(&self) -> bool {
        self.to_command()
            .arg("--version")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }
}

pub(crate) fn resolve_python_command(
    python_path: Option<&str>,
    venv_dir: Option<&Path>,
) -> PythonCommand {
    if let Some(path) = python_path.map(str::trim).filter(|value| !value.is_empty()) {
        return PythonCommand::new(path, vec![]);
    }

    if let Some(venv_dir) = venv_dir {
        let unix_python = venv_dir.join("bin").join("python3");
        if unix_python.is_file() {
            return PythonCommand::new(unix_python.to_string_lossy().to_string(), vec![]);
        }

        let windows_python = venv_dir.join("Scripts").join("python.exe");
        if windows_python.is_file() {
            return PythonCommand::new(windows_python.to_string_lossy().to_string(), vec![]);
        }
    }

    let default_commands = default_python_commands();
    for command in &default_commands {
        if command.is_available() {
            return command.clone();
        }
    }

    default_commands
        .into_iter()
        .next()
        .unwrap_or_else(|| PythonCommand::new("python", vec![]))
}

fn default_python_commands() -> Vec<PythonCommand> {
    #[cfg(target_os = "windows")]
    {
        vec![
            PythonCommand::new("python", vec![]),
            PythonCommand::new("py", vec!["-3".to_string()]),
        ]
    }

    #[cfg(not(target_os = "windows"))]
    {
        vec![
            PythonCommand::new("python3", vec![]),
            PythonCommand::new("python", vec![]),
        ]
    }
}

fn build_segment_meta(
    segment_paths: &[String],
    segment_meta: &[AudioSegmentMeta],
) -> Vec<WorkerSegmentMeta> {
    let mut cursor = 0_u64;
    segment_paths
        .iter()
        .enumerate()
        .map(|(index, path)| {
            let duration_ms = segment_meta
                .get(index)
                .map(|value| value.duration_ms)
                .filter(|value| *value > 0)
                .unwrap_or(600_000);
            let start_ms = cursor;
            let end_ms = start_ms.saturating_add(duration_ms);
            cursor = end_ms;
            WorkerSegmentMeta {
                path: path.clone(),
                start_ms,
                end_ms,
                duration_ms,
            }
        })
        .collect()
}

pub fn transcribe_with_local_stt(
    segment_paths: &[String],
    config: &LocalSttConfig,
    segment_meta: &[AudioSegmentMeta],
    session_id: &str,
    progress_callback: &dyn Fn(&str),
) -> Result<Vec<TranscriptSegment>, String> {
    let script_path = ensure_worker_script_on_disk()?;

    let worker_request = WorkerRequest {
        session_id: session_id.to_string(),
        audio_paths: segment_paths.to_vec(),
        segment_meta: build_segment_meta(segment_paths, segment_meta),
        engine: config.engine.clone(),
        whisper_model: config.whisper_model.clone(),
        sense_voice_model: config.sense_voice_model.clone(),
        language: config.language.clone(),
        diarization_enabled: config.diarization_enabled,
        min_speakers: config.min_speakers,
        max_speakers: config.max_speakers,
        speaker_count_hint: config.speaker_count_hint,
        compute_device: config.compute_device.clone(),
        vad_enabled: config.vad_enabled,
        chunk_seconds: config.chunk_seconds,
        model_cache_dir: config.model_cache_dir.clone(),
    };

    let tmp_dir = std::env::temp_dir().join("open-recorder-local-stt");
    fs::create_dir_all(&tmp_dir)
        .map_err(|error| format!("failed to create local stt temp dir: {error}"))?;
    let trace_id = Uuid::new_v4().to_string();
    let request_path = tmp_dir.join(format!("{trace_id}-request.json"));
    let response_path = tmp_dir.join(format!("{trace_id}-response.json"));
    let payload = serde_json::to_vec_pretty(&worker_request)
        .map_err(|error| format!("failed to encode local stt request: {error}"))?;
    fs::write(&request_path, payload)
        .map_err(|error| format!("failed to write local stt request file: {error}"))?;

    progress_callback("Running local STT model...");
    let python_command = resolve_python_command(
        config.python_path.as_deref(),
        config.venv_dir.as_deref().map(Path::new),
    );
    let python_executable = python_command.display();
    let command_output = python_command
        .to_command()
        .arg(&script_path)
        .arg("--request")
        .arg(&request_path)
        .arg("--response")
        .arg(&response_path)
        .output()
        .map_err(|error| {
            format!(
                "failed to run local stt worker via '{}': {error}",
                python_executable
            )
        })?;

    let response_from_file = fs::read_to_string(&response_path)
        .ok()
        .and_then(|raw| serde_json::from_str::<WorkerResponse>(&raw).ok());

    if !command_output.status.success() {
        if let Some(response) = response_from_file
            .as_ref()
            .and_then(|value| value.error.as_ref())
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
        {
            return Err(format!("local stt worker failed: {response}"));
        }

        let stderr = String::from_utf8_lossy(&command_output.stderr)
            .trim()
            .to_string();
        let stdout = String::from_utf8_lossy(&command_output.stdout)
            .trim()
            .to_string();
        return Err(format!(
            "local stt worker failed with status {}: {}{}; response_path={}",
            command_output.status,
            if stderr.is_empty() { "" } else { "stderr=" },
            if stderr.is_empty() {
                if stdout.is_empty() {
                    "no output".to_string()
                } else {
                    format!("stdout={stdout}")
                }
            } else if stdout.is_empty() {
                stderr
            } else {
                format!("{stderr}; stdout={stdout}")
            },
            response_path.display()
        ));
    }

    let response = match response_from_file {
        Some(value) => value,
        None => {
            let response_raw = fs::read_to_string(&response_path)
                .map_err(|error| format!("failed to read local stt response file: {error}"))?;
            serde_json::from_str::<WorkerResponse>(&response_raw)
                .map_err(|error| format!("failed to parse local stt response JSON: {error}"))?
        }
    };

    if let Some(error) = response.error.filter(|value| !value.trim().is_empty()) {
        return Err(error);
    }
    if let Some(warning) = response
        .warning
        .as_ref()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
    {
        let warning_msg = format!("Warning: {warning}");
        progress_callback(&warning_msg);
        eprintln!("[local-stt] {warning_msg}");
    }

    if response.segments.is_empty() {
        return Err("local stt worker returned empty transcript".to_string());
    }

    let transcript = response
        .segments
        .into_iter()
        .map(|segment| TranscriptSegment {
            start_ms: segment.start_ms,
            end_ms: segment.end_ms.max(segment.start_ms),
            text: segment.text,
            translation_text: None,
            translation_target_language: None,
            confidence: segment.confidence,
            speaker_id: segment.speaker_id,
            speaker_label: segment.speaker_label,
        })
        .collect();
    Ok(transcript)
}
