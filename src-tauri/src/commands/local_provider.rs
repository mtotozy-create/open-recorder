use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use serde::Serialize;
use tauri::State;

use crate::{
    models::{LocalSttEngine, LocalSttProviderSettings, ProviderKind, Settings},
    providers::local_stt::{ensure_worker_script_on_disk, resolve_python_command, PythonCommand},
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
    resolve_python_command(local_stt.python_path.as_deref(), Some(venv_dir)).display()
}

fn resolve_python_command_for_settings(
    local_stt: &LocalSttProviderSettings,
    venv_dir: &Path,
) -> PythonCommand {
    resolve_python_command(local_stt.python_path.as_deref(), Some(venv_dir))
}

fn check_python_ready(python_command: &PythonCommand) -> bool {
    python_command.is_available()
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
    let python_command = resolve_python_command_for_settings(&local_stt, &venv_dir);
    let python_executable = resolve_python_executable(&local_stt, &venv_dir);
    let venv_ready = venv_dir.join("bin").join("python3").is_file()
        || venv_dir.join("Scripts").join("python.exe").is_file();
    Ok(LocalProviderStatusResponse {
        python_ready: check_python_ready(&python_command),
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

fn run_python_command(python_command: &PythonCommand, args: &[&str]) -> Result<(), String> {
    let output = python_command
        .to_command()
        .args(args)
        .output()
        .map_err(|error| format!("failed to run '{}': {error}", python_command.display()))?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Err(format!(
        "command failed: {} {:?}; stderr={}; stdout={}",
        python_command.display(),
        args,
        stderr,
        stdout
    ))
}

fn resolve_whisper_repos(whisper_model: &str) -> (String, String) {
    match whisper_model.trim().to_ascii_lowercase().as_str() {
        "small" => (
            "Systran/faster-whisper-small".to_string(),
            "mlx-community/whisper-small-mlx".to_string(),
        ),
        "medium" => (
            "Systran/faster-whisper-medium".to_string(),
            "mlx-community/whisper-medium-mlx".to_string(),
        ),
        "large-v3" => (
            "Systran/faster-whisper-large-v3".to_string(),
            "mlx-community/whisper-large-v3-mlx".to_string(),
        ),
        _ => {
            let value = whisper_model.trim().to_string();
            (value.clone(), value)
        }
    }
}

fn maybe_insert_hf_model(model_ids: &mut BTreeSet<String>, model_id_or_path: &str) {
    let value = model_id_or_path.trim();
    if value.is_empty() || Path::new(value).exists() {
        return;
    }
    model_ids.insert(value.to_string());
}

fn collect_offline_snapshot_models(local_stt: &LocalSttProviderSettings) -> Vec<String> {
    let mut model_ids: BTreeSet<String> = BTreeSet::new();

    if matches!(local_stt.engine, LocalSttEngine::Whisper) {
        let (faster_repo, mlx_repo) = resolve_whisper_repos(&local_stt.whisper_model);
        maybe_insert_hf_model(&mut model_ids, &faster_repo);
        if cfg!(target_os = "macos") {
            maybe_insert_hf_model(&mut model_ids, &mlx_repo);
        }
    }

    if local_stt.diarization_enabled {
        maybe_insert_hf_model(&mut model_ids, "pyannote/speaker-diarization-3.1");
        maybe_insert_hf_model(&mut model_ids, "pyannote/segmentation-3.0");
        maybe_insert_hf_model(&mut model_ids, "pyannote/wespeaker-voxceleb-resnet34-LM");
    }

    model_ids.into_iter().collect()
}

fn preload_hf_snapshots(
    python_executable: &str,
    model_cache_dir: &Path,
    model_ids: &[String],
) -> Result<(), String> {
    if model_ids.is_empty() {
        return Ok(());
    }

    let preload_script = r#"
import sys
from huggingface_hub import snapshot_download

cache_dir = sys.argv[1]
for model_id in sys.argv[2:]:
    if not model_id:
        continue
    snapshot_download(repo_id=model_id, cache_dir=cache_dir)
"#;

    let cache_dir = model_cache_dir.to_string_lossy().to_string();
    let mut command = Command::new(python_executable);
    command
        .arg("-c")
        .arg(preload_script)
        .arg(&cache_dir)
        .env("HF_HOME", &cache_dir)
        .env("TRANSFORMERS_CACHE", &cache_dir)
        .env("HUGGINGFACE_HUB_CACHE", &cache_dir);
    for model_id in model_ids {
        command.arg(model_id);
    }

    let output = command.output().map_err(|error| {
        format!(
            "failed to preload offline model snapshots via '{}': {error}",
            python_executable
        )
    })?;

    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Err(format!(
        "failed to preload offline model snapshots {:?}; stderr={}; stdout={}",
        model_ids, stderr, stdout
    ))
}

#[tauri::command]
pub fn local_provider_status(
    state: State<'_, AppState>,
) -> Result<LocalProviderStatusResponse, String> {
    let storage = state
        .storage
        .lock()
        .map_err(|_| "failed to acquire storage lock".to_string())?;
    let settings = storage.get_settings()?;
    let data_root = storage.data_root_dir()?;
    build_status(&settings, &data_root)
}

#[tauri::command]
pub fn local_provider_prepare(
    state: State<'_, AppState>,
) -> Result<LocalProviderStatusResponse, String> {
    let (bootstrap_python, venv_dir, model_cache_dir, offline_snapshot_models) = {
        let storage = state
            .storage
            .lock()
            .map_err(|_| "failed to acquire storage lock".to_string())?;
        let mut settings = storage.get_settings()?;
        let data_root = storage.data_root_dir()?;
        let local_stt = settings
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

        let bootstrap_python = resolve_python_command(local_stt.python_path.as_deref(), None);
        let offline_snapshot_models = collect_offline_snapshot_models(local_stt);
        settings.normalize();
        storage.save_settings(&settings)?;
        (
            bootstrap_python,
            venv_dir,
            model_cache_dir,
            offline_snapshot_models,
        )
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
        run_python_command(
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
            "opencc-python-reimplemented",
        ],
    )?;
    if cfg!(target_os = "macos") {
        run_command(&pip_executable, &["install", "mlx-whisper"]).map_err(|error| {
            format!("failed to install mlx-whisper (required for whisper+mps execution): {error}")
        })?;
    }

    let python_executable = if venv_python_unix.is_file() {
        venv_python_unix.to_string_lossy().to_string()
    } else if venv_python_windows.is_file() {
        venv_python_windows.to_string_lossy().to_string()
    } else {
        return Err("python not found in local stt virtual environment".to_string());
    };
    preload_hf_snapshots(
        &python_executable,
        &model_cache_dir,
        &offline_snapshot_models,
    )?;

    let storage = state
        .storage
        .lock()
        .map_err(|_| "failed to acquire storage lock".to_string())?;
    let settings = storage.get_settings()?;
    let data_root = storage.data_root_dir()?;
    build_status(&settings, &data_root)
}
