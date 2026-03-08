# Local STT Whisper MPS Execution Design (2026-03-08)

## Objective
1. Keep existing faster-whisper path for non-MPS devices.
2. When `computeDevice = mps`, run Whisper through `mlx-whisper` so execution is truly on Apple Metal.
3. Show actual model mapping in settings UI to avoid ambiguous model selection.

## Model Mapping Strategy
Use a shared logical model id (`small`, `medium`, `large-v3`) and map each id to:
1. faster-whisper model name
2. mlx-whisper model repo name

Profiles:
1. `small` -> `small` / `mlx-community/whisper-small-mlx`
2. `medium` -> `medium` / `mlx-community/whisper-medium-mlx`
3. `large-v3` -> `large-v3` / `mlx-community/whisper-large-v3-mlx`

## Runtime Routing
In Python worker (`engine=whisper`):
1. If `computeDevice == mps`: route to `mlx-whisper` path.
2. Otherwise: keep current faster-whisper path and existing fallback behavior.

## Stability Guard
`mlx` can terminate process at native layer in some environments.
Use subprocess isolation for mlx transcription:
1. spawn child python process for mlx inference
2. parse JSON output back in parent
3. if child crashes, parent returns deterministic error instead of hanging/crashing

## Setup Changes
In local provider prepare (`src-tauri/src/commands/local_provider.rs`):
1. continue installing existing dependencies
2. on macOS, additionally install `mlx-whisper`

## UX Changes
In settings page Whisper model dropdown:
1. show user-facing label
2. show actual model names for both backends (`faster-whisper` and `mlx-whisper`)
3. keep saved value as stable logical id

## Validation
1. `python3 -m py_compile src-tauri/python/local_stt_worker.py`
2. `npm run build`
3. `cargo check`
4. verify patched runtime worker in Application Support is synchronized
