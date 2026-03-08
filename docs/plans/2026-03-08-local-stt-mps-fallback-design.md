# Local STT MPS Fallback Design (2026-03-08)

## Problem
When `computeDevice` is set to `mps`, local transcription can fail with runtime errors like `unsupported device mps`, causing the whole local STT job to fail.

## Goal
1. Keep `mps` as first attempt when user explicitly selects it.
2. If `mps` fails due device support/runtime constraints, automatically fallback to `cpu`.
3. Apply fallback consistently to both engines: `whisper` and `sensevoice_small`.

## Chosen Approach
Use runtime retry on device-related failures:
1. Normalize and preserve requested device.
2. Attempt model initialization on requested device.
3. If failure matches device-unsupported patterns, log warning and retry model init with `cpu`.
4. If transcription call itself fails for the same reason, reinitialize model on `cpu` and retry current segment.

## Scope
- File: `src-tauri/python/local_stt_worker.py`
- Add helpers:
  - `_should_fallback_to_cpu(...)`
  - `_load_whisper_model(...)`
  - `_load_sensevoice_model(...)`
- Apply fallback in both engine branches.

## Non-goals
1. No UI change to device selector.
2. No hardware probing matrix for every backend.
3. No change to diarization pipeline behavior.

## Validation
1. Python syntax check (`py_compile`) passes.
2. Simulated runtime checks confirm `mps` failure triggers fallback and returns successful response.

## Risks
1. Error message matching may miss rare backend-specific strings.
2. First failed attempt adds a small startup overhead before CPU fallback.
