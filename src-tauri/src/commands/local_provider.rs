use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use serde::Serialize;
use tauri::State;

use crate::{
    models::{LocalSttProviderSettings, ProviderKind, Settings},
    providers::local_stt::ensure_worker_script_on_disk,
    state::AppState,
};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalProviderStatusResponse {
    pub python_ready: bool,
    pub venv_ready: bool,
    pub worker_script_ready: bool,
    pub python_executable: String,
    pub venv_dir: String,
    pub model_cache_dir: String,
    pub worker_script_path: String,
}

fn resolve_local_stt_settings(settings: &Settings) -> Option<&LocalSttProviderSettings> {
    settings
        .providers
        .iter()
        .find(|provider| provider.kind == ProviderKind::LocalStt)
        .and_then(|provider| provider.local_stt.as_ref())
}

fn resolve_python_executable(local_stt: &LocalSttProviderSettings, venv_dir: &Path) -> String {
    if let Some(path) = local_stt
        .python_path
        .as_ref()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
    {
        return path.to_string();
    }

    let unix_python = venv_dir.join("bin").join("python3");
    if unix_python.is_file() {
        return unix_python.to_string_lossy().to_string();
    }
    let windows_python = venv_dir.join("Scripts").join("python.exe");
    if windows_python.is_file() {
        return windows_python.to_string_lossy().to_string();
    }
    "python3".to_string()
}

fn check_python_ready(python_executable: &str) -> bool {
    Command::new(python_executable)
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn build_status(
    settings: &Settings,
    data_root: &Path,
) -> Result<LocalProviderStatusResponse, String> {
    let local_stt = resolve_local_stt_settings(settings)
        .cloned()
        .unwrap_or_default();
    let default_venv_dir = data_root.join("local-stt").join("venv");
    let venv_dir = local_stt
        .venv_dir
        .as_ref()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or(default_venv_dir);
    let default_model_cache_dir = data_root.join("local-stt").join("models");
    let model_cache_dir = local_stt
        .model_cache_dir
        .as_ref()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or(default_model_cache_dir);
    let worker_script_path = ensure_worker_script_on_disk()?;
    let python_executable = resolve_python_executable(&local_stt, &venv_dir);
    let venv_ready = venv_dir.join("bin").join("python3").is_file()
        || venv_dir.join("Scripts").join("python.exe").is_file();
    Ok(LocalProviderStatusResponse {
        python_ready: check_python_ready(&python_executable),
        venv_ready,
        worker_script_ready: true,
        python_executable,
        venv_dir: venv_dir.to_string_lossy().to_string(),
        model_cache_dir: model_cache_dir.to_string_lossy().to_string(),
        worker_script_path: worker_script_path.to_string_lossy().to_string(),
    })
}

fn run_command(program: &str, args: &[&str]) -> Result<(), String> {
    let output = Command::new(program)
        .args(args)
        .output()
        .map_err(|error| format!("failed to run '{}': {error}", program))?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Err(format!(
        "command failed: {} {:?}; stderr={}; stdout={}",
        program, args, stderr, stdout
    ))
}

#[tauri::command]
pub fn local_provider_status(
    state: State<'_, AppState>,
) -> Result<LocalProviderStatusResponse, String> {
    let mut storage = state
        .storage
        .lock()
        .map_err(|_| "failed to acquire storage lock".to_string())?;
    storage.data.settings.normalize();
    let data_root = storage.data_root_dir()?;
    build_status(&storage.data.settings, &data_root)
}

#[tauri::command]
pub fn local_provider_prepare(
    state: State<'_, AppState>,
) -> Result<LocalProviderStatusResponse, String> {
    let (bootstrap_python, venv_dir, model_cache_dir) = {
        let mut storage = state
            .storage
            .lock()
            .map_err(|_| "failed to acquire storage lock".to_string())?;
        storage.data.settings.normalize();
        let data_root = storage.data_root_dir()?;
        let local_stt = storage
            .data
            .settings
            .providers
            .iter_mut()
            .find(|provider| provider.kind == ProviderKind::LocalStt)
            .and_then(|provider| provider.local_stt.as_mut())
            .ok_or_else(|| "local_stt provider settings not found".to_string())?;

        let venv_dir = local_stt
            .venv_dir
            .as_ref()
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| data_root.join("local-stt").join("venv"));
        let model_cache_dir = local_stt
            .model_cache_dir
            .as_ref()
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| data_root.join("local-stt").join("models"));
        local_stt.venv_dir = Some(venv_dir.to_string_lossy().to_string());
        local_stt.model_cache_dir = Some(model_cache_dir.to_string_lossy().to_string());

        let bootstrap_python = local_stt
            .python_path
            .as_ref()
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            .unwrap_or("python3")
            .to_string();
        storage.save()?;
        (bootstrap_python, venv_dir, model_cache_dir)
    };

    if let Some(parent) = venv_dir.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create venv parent dir: {error}"))?;
    }
    fs::create_dir_all(&model_cache_dir)
        .map_err(|error| format!("failed to create model cache dir: {error}"))?;

    let venv_python_unix = venv_dir.join("bin").join("python3");
    let venv_python_windows = venv_dir.join("Scripts").join("python.exe");
    if !venv_python_unix.is_file() && !venv_python_windows.is_file() {
        run_command(
            &bootstrap_python,
            &["-m", "venv", venv_dir.to_string_lossy().as_ref()],
        )?;
    }

    let pip_unix = venv_dir.join("bin").join("pip");
    let pip_windows = venv_dir.join("Scripts").join("pip.exe");
    let pip_executable = if pip_unix.is_file() {
        pip_unix.to_string_lossy().to_string()
    } else if pip_windows.is_file() {
        pip_windows.to_string_lossy().to_string()
    } else {
        return Err("pip not found in local stt virtual environment".to_string());
    };

    run_command(&pip_executable, &["install", "--upgrade", "pip"])?;
    run_command(
        &pip_executable,
        &[
            "install",
            "faster-whisper",
            "funasr",
            "pyannote.audio",
            "torch",
        ],
    )?;

    let mut storage = state
        .storage
        .lock()
        .map_err(|_| "failed to acquire storage lock".to_string())?;
    storage.data.settings.normalize();
    let data_root = storage.data_root_dir()?;
    build_status(&storage.data.settings, &data_root)
}
