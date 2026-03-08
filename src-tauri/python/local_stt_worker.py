#!/usr/bin/env python3

import argparse
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import traceback
from dataclasses import dataclass
from typing import Any, Dict, Iterator, List, Optional, Tuple


@dataclass
class SegmentMeta:
    path: str
    start_ms: int
    end_ms: int
    duration_ms: int


@dataclass
class WhisperChunk:
    start_sec: float
    end_sec: float
    text: str
    confidence: Optional[float]


@dataclass
class DiarizationTurn:
    start_sec: float
    end_sec: float
    speaker_id: str


WHISPER_MODEL_PROFILES: Dict[str, Dict[str, str]] = {
    "small": {
        "faster_whisper": "small",
        "faster_whisper_hf_repo": "Systran/faster-whisper-small",
        "mlx_whisper": "mlx-community/whisper-small-mlx",
    },
    "medium": {
        "faster_whisper": "medium",
        "faster_whisper_hf_repo": "Systran/faster-whisper-medium",
        "mlx_whisper": "mlx-community/whisper-medium-mlx",
    },
    "large-v3": {
        "faster_whisper": "large-v3",
        "faster_whisper_hf_repo": "Systran/faster-whisper-large-v3",
        "mlx_whisper": "mlx-community/whisper-large-v3-mlx",
    },
}

MERGE_SAME_SPEAKER_MAX_GAP_MS = 600
PYANNOTE_PIPELINE_MODEL_ID = "pyannote/speaker-diarization-3.1"
PYANNOTE_DEPENDENCY_MODEL_IDS = (
    "pyannote/segmentation-3.0",
    "pyannote/wespeaker-voxceleb-resnet34-LM",
)

_T2S_CONVERTER: Any = None
_T2S_CONVERTER_READY = False
_T2S_WARNED = False


def _read_request(path: str) -> Dict[str, Any]:
    with open(path, "r", encoding="utf-8") as f:
        return json.load(f)


def _write_response(path: str, payload: Dict[str, Any]) -> None:
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w", encoding="utf-8") as f:
        json.dump(payload, f, ensure_ascii=False)


def _extract_segment_meta(raw: List[Dict[str, Any]]) -> List[SegmentMeta]:
    items: List[SegmentMeta] = []
    for item in raw:
        items.append(
            SegmentMeta(
                path=str(item.get("path", "")),
                start_ms=int(item.get("startMs", 0)),
                end_ms=int(item.get("endMs", 0)),
                duration_ms=int(item.get("durationMs", 0)),
            )
        )
    return items


def _normalize_device(device: str) -> str:
    normalized = (device or "").strip().lower()
    if not normalized or normalized == "auto":
        return "cpu"
    return normalized


def _resolve_whisper_model_names(model_name: str) -> Tuple[str, str, str]:
    normalized_model = (model_name or "").strip().lower()
    profile = WHISPER_MODEL_PROFILES.get(normalized_model)
    if profile:
        return (
            profile["faster_whisper"],
            profile["faster_whisper_hf_repo"],
            profile["mlx_whisper"],
        )
    model = (model_name or "").strip() or "small"
    return model, model, model


def _resolve_local_hf_snapshot(model_id_or_path: str) -> str:
    candidate = (model_id_or_path or "").strip()
    if not candidate:
        raise RuntimeError("empty model id/path")
    if os.path.exists(candidate):
        return candidate

    from huggingface_hub import snapshot_download  # type: ignore

    try:
        return snapshot_download(candidate, local_files_only=True)
    except Exception as error:
        raise RuntimeError(
            f"offline model cache missing for '{candidate}', run local provider prepare or pre-download models: {error}"
        ) from error


def _whisper_compute_type(device: str) -> str:
    return "float16" if device in ("cuda", "mps") else "int8"


def _should_fallback_to_cpu(error: Exception, device: str) -> bool:
    normalized_device = (device or "").strip().lower()
    if normalized_device == "cpu":
        return False
    message = str(error).strip().lower()
    if not message:
        return False

    fallback_markers = (
        "unsupported device",
        "not available",
        "not compiled",
        "not implemented",
        "invalid device",
    )
    if normalized_device in message and any(marker in message for marker in fallback_markers):
        return True
    if normalized_device == "cuda" and "cuda" in message and "driver" in message:
        return True
    return False


def _load_whisper_model(
    whisper_model_name: str,
    device: str,
    model_cache_dir: Any,
) -> Any:
    from faster_whisper import WhisperModel  # type: ignore

    return WhisperModel(
        whisper_model_name,
        device=device,
        compute_type=_whisper_compute_type(device),
        download_root=model_cache_dir,
    )


def _resolve_faster_whisper_model_source(
    faster_whisper_model_name: str,
    faster_whisper_hf_repo: str,
) -> str:
    model_candidate = (faster_whisper_model_name or "").strip()
    if model_candidate and os.path.exists(model_candidate):
        return model_candidate
    repo_candidate = (faster_whisper_hf_repo or "").strip() or model_candidate
    return _resolve_local_hf_snapshot(repo_candidate)


def _load_sensevoice_model(
    sense_voice_model_name: str,
    device: str,
) -> Any:
    from funasr import AutoModel  # type: ignore

    return AutoModel(
        model=sense_voice_model_name,
        trust_remote_code=True,
        device=device,
        disable_update=True,
    )


def _build_diarization_kwargs(
    speaker_count_hint: Optional[int],
    min_speakers: Optional[int],
    max_speakers: Optional[int],
) -> Dict[str, Any]:
    kwargs: Dict[str, Any] = {}
    if speaker_count_hint:
        kwargs["num_speakers"] = speaker_count_hint
    else:
        if min_speakers:
            kwargs["min_speakers"] = min_speakers
        if max_speakers:
            kwargs["max_speakers"] = max_speakers
    return kwargs


def _iter_diarization_tracks(diarization_output: Any) -> Iterator[Tuple[Any, Any, Any]]:
    annotation = diarization_output
    speaker_diarization = getattr(diarization_output, "speaker_diarization", None)
    if speaker_diarization is not None:
        annotation = speaker_diarization
    itertracks = getattr(annotation, "itertracks", None)
    if callable(itertracks):
        yield from itertracks(yield_label=True)


def _resolve_ffmpeg_executable() -> Optional[str]:
    ffmpeg = shutil.which("ffmpeg")
    if ffmpeg:
        return ffmpeg
    for candidate in (
        "/opt/homebrew/bin/ffmpeg",
        "/usr/local/bin/ffmpeg",
        "/opt/local/bin/ffmpeg",
        "/usr/bin/ffmpeg",
    ):
        if os.path.isfile(candidate) and os.access(candidate, os.X_OK):
            return candidate
    return None


def _maybe_convert_to_wav(audio_path: str) -> Optional[str]:
    ext = os.path.splitext(audio_path)[1].lower()
    if ext not in {".m4a", ".mp4", ".aac", ".mp3"}:
        return None
    ffmpeg = _resolve_ffmpeg_executable()
    if not ffmpeg:
        return None
    with tempfile.NamedTemporaryFile(suffix=".wav", delete=False) as temp_file:
        wav_path = temp_file.name
    try:
        subprocess.run(
            [
                ffmpeg,
                "-y",
                "-i",
                audio_path,
                "-ar",
                "16000",
                "-ac",
                "1",
                wav_path,
            ],
            check=True,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        return wav_path
    except Exception:
        if os.path.exists(wav_path):
            os.remove(wav_path)
        return None


def _run_diarization(
    audio_path: str,
    diarization_pipeline: Any,
    speaker_count_hint: Optional[int],
    min_speakers: Optional[int],
    max_speakers: Optional[int],
) -> List[DiarizationTurn]:
    kwargs = _build_diarization_kwargs(speaker_count_hint, min_speakers, max_speakers)
    temporary_wav: Optional[str] = None
    try:
        try:
            diarization = diarization_pipeline(audio_path, **kwargs)
        except Exception:
            temporary_wav = _maybe_convert_to_wav(audio_path)
            if temporary_wav:
                diarization = diarization_pipeline(temporary_wav, **kwargs)
            else:
                raise

        turns: List[DiarizationTurn] = []
        for turn, _, speaker in _iter_diarization_tracks(diarization):
            start_sec = float(getattr(turn, "start", 0.0) or 0.0)
            end_sec = float(getattr(turn, "end", start_sec) or start_sec)
            if end_sec <= start_sec:
                continue
            speaker_id = str(speaker)
            if not speaker_id:
                continue
            turns.append(
                DiarizationTurn(
                    start_sec=start_sec,
                    end_sec=end_sec,
                    speaker_id=speaker_id,
                )
            )
        return turns
    except Exception as error:
        print(
            f"[local-stt] diarization failed for '{audio_path}': {error}",
            file=sys.stderr,
        )
        return []
    finally:
        if temporary_wav and os.path.exists(temporary_wav):
            os.remove(temporary_wav)


def _assign_speaker(
    diarization_turns: List[DiarizationTurn],
    start_sec: float,
    end_sec: float,
) -> Tuple[Optional[str], Optional[str]]:
    if not diarization_turns:
        return None, None
    end_sec = max(end_sec, start_sec)
    duration_by_speaker: Dict[str, float] = {}
    for turn in diarization_turns:
        overlap = min(end_sec, turn.end_sec) - max(start_sec, turn.start_sec)
        if overlap > 0:
            duration_by_speaker[turn.speaker_id] = (
                duration_by_speaker.get(turn.speaker_id, 0.0) + overlap
            )
    if not duration_by_speaker:
        return None, None
    speaker_id = max(duration_by_speaker, key=duration_by_speaker.get)
    return speaker_id, f"Speaker {speaker_id}"


def _load_pyannote_pipeline(model_cache_dir: Optional[str]) -> Any:
    if model_cache_dir:
        os.environ.setdefault("HF_HOME", model_cache_dir)
        os.environ.setdefault("TRANSFORMERS_CACHE", model_cache_dir)
    from pyannote.audio import Pipeline  # type: ignore

    local_snapshot = _resolve_local_hf_snapshot(PYANNOTE_PIPELINE_MODEL_ID)
    config_path = os.path.join(local_snapshot, "config.yaml")
    if not os.path.isfile(config_path):
        raise RuntimeError(f"pyannote config not found: {config_path}")

    with open(config_path, "r", encoding="utf-8") as file:
        raw_config = file.read()

    patched_config = raw_config
    for dependency_model_id in PYANNOTE_DEPENDENCY_MODEL_IDS:
        local_dependency_snapshot = _resolve_local_hf_snapshot(dependency_model_id)
        patched_config = patched_config.replace(
            dependency_model_id, local_dependency_snapshot
        )

    temp_config_path: Optional[str] = None
    config_source: str = config_path
    if patched_config != raw_config:
        with tempfile.NamedTemporaryFile(
            mode="w",
            suffix="-pyannote-config.yaml",
            delete=False,
            encoding="utf-8",
        ) as temp_config:
            temp_config.write(patched_config)
            temp_config_path = temp_config.name
        config_source = temp_config_path

    try:
        pipeline = Pipeline.from_pretrained(config_source)
    finally:
        if temp_config_path and os.path.exists(temp_config_path):
            os.remove(temp_config_path)

    print(
        f"[local-stt] loaded diarization pipeline from local snapshot: {local_snapshot}",
        file=sys.stderr,
    )
    return pipeline


def _transcribe_whisper(
    audio_path: str,
    model: Any,
    language: str,
    vad_enabled: bool,
) -> List[WhisperChunk]:
    language_arg = None if language == "auto" else language
    segments, _ = model.transcribe(
        audio_path,
        language=language_arg,
        vad_filter=bool(vad_enabled),
    )
    chunks: List[WhisperChunk] = []
    for segment in segments:
        text = str(getattr(segment, "text", "")).strip()
        if not text:
            continue
        start_sec = float(getattr(segment, "start", 0.0) or 0.0)
        end_sec = float(getattr(segment, "end", start_sec) or start_sec)
        end_sec = max(end_sec, start_sec)
        avg_logprob = getattr(segment, "avg_logprob", None)
        confidence = None
        if isinstance(avg_logprob, (int, float)):
            confidence = max(0.0, min(1.0, 1.0 + float(avg_logprob)))
        chunks.append(
            WhisperChunk(
                start_sec=start_sec,
                end_sec=end_sec,
                text=text,
                confidence=confidence,
            )
        )
    return chunks


def _transcribe_whisper_mlx(
    audio_path: str,
    model_repo: str,
    language: str,
    ffmpeg_path: str,
) -> List[WhisperChunk]:
    child_script = r"""
import json
import os
import sys

audio_path, model_repo, language, output_path, ffmpeg_path = sys.argv[1:6]
payload = {"result": None, "error": None, "defaultDevice": None}
try:
    if ffmpeg_path:
        ffmpeg_dir = os.path.dirname(ffmpeg_path)
        current_path = os.environ.get("PATH", "")
        os.environ["PATH"] = (
            f"{ffmpeg_dir}:{current_path}" if current_path else ffmpeg_dir
        )

    import mlx.core as mx  # type: ignore
    if hasattr(mx, "set_default_device") and hasattr(mx, "gpu"):
        mx.set_default_device(mx.gpu)
    payload["defaultDevice"] = str(mx.default_device())

    import mlx_whisper  # type: ignore
    kwargs = {"path_or_hf_repo": model_repo}
    if language != "auto":
        kwargs["language"] = language
    payload["result"] = mlx_whisper.transcribe(audio_path, **kwargs)
except Exception as error:
    payload["error"] = str(error)

with open(output_path, "w", encoding="utf-8") as file:
    json.dump(payload, file, ensure_ascii=False)
"""

    with tempfile.NamedTemporaryFile(suffix=".json", delete=False) as temp_result_file:
        result_path = temp_result_file.name

    try:
        completed = subprocess.run(
            [
                sys.executable,
                "-c",
                child_script,
                audio_path,
                model_repo,
                language,
                result_path,
                ffmpeg_path,
            ],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            check=False,
        )
        if completed.returncode != 0:
            stderr_text = completed.stderr.strip()
            stdout_text = completed.stdout.strip()
            detail = stderr_text or stdout_text or "no output"
            raise RuntimeError(
                f"mlx-whisper subprocess failed (status={completed.returncode}): {detail}"
            )

        with open(result_path, "r", encoding="utf-8") as file:
            payload = json.load(file)
    finally:
        if os.path.exists(result_path):
            os.remove(result_path)

    error_text = str(payload.get("error", "") or "").strip()
    if error_text:
        raise RuntimeError(f"mlx-whisper failed: {error_text}")

    default_device = str(payload.get("defaultDevice", "") or "").lower()
    if "gpu" not in default_device:
        raise RuntimeError(
            f"mlx default device is '{default_device or 'unknown'}', cannot enforce mps execution"
        )

    result = payload.get("result")
    if not isinstance(result, dict):
        return []

    chunks: List[WhisperChunk] = []
    segments = result.get("segments")
    if isinstance(segments, list):
        for item in segments:
            if not isinstance(item, dict):
                continue
            text = str(item.get("text", "")).strip()
            if not text:
                continue
            start_sec = float(item.get("start", 0.0) or 0.0)
            end_sec = float(item.get("end", start_sec) or start_sec)
            end_sec = max(end_sec, start_sec)
            avg_logprob = item.get("avg_logprob")
            confidence = None
            if isinstance(avg_logprob, (int, float)):
                confidence = max(0.0, min(1.0, 1.0 + float(avg_logprob)))
            chunks.append(
                WhisperChunk(
                    start_sec=start_sec,
                    end_sec=end_sec,
                    text=text,
                    confidence=confidence,
                )
            )

    if chunks:
        return chunks

    text = ""
    if isinstance(result, dict):
        text = str(result.get("text", "")).strip()
    if text:
        return [WhisperChunk(start_sec=0.0, end_sec=0.0, text=text, confidence=None)]
    return []


def _extract_text_from_obj(value: Any) -> Optional[str]:
    if isinstance(value, dict):
        if "text" in value and isinstance(value["text"], str):
            return value["text"]
        for item in value.values():
            extracted = _extract_text_from_obj(item)
            if extracted:
                return extracted
        return None
    if isinstance(value, list):
        for item in value:
            extracted = _extract_text_from_obj(item)
            if extracted:
                return extracted
        return None
    return None


def _clean_sensevoice_text(text: str) -> str:
    cleaned = text.strip()
    if not cleaned:
        return ""
    try:
        from funasr.utils.postprocess_utils import (  # type: ignore
            rich_transcription_postprocess,
        )

        cleaned = str(rich_transcription_postprocess(cleaned))
    except Exception:
        pass
    cleaned = re.sub(r"<\|[^|>]+\|>", "", cleaned)
    cleaned = re.sub(r"\s+", " ", cleaned).strip()
    return cleaned


def _transcribe_sensevoice(
    audio_path: str,
    model: Any,
    language: str,
) -> Tuple[str, Optional[float]]:
    language_arg = None if language == "auto" else language
    result = model.generate(input=audio_path, language=language_arg, use_itn=True)
    text = _extract_text_from_obj(result) or ""
    return _clean_sensevoice_text(text), None


def _merge_diarization_turns(diarization_turns: List[DiarizationTurn]) -> List[DiarizationTurn]:
    if not diarization_turns:
        return []
    merged: List[DiarizationTurn] = []
    sorted_turns = sorted(
        diarization_turns,
        key=lambda item: (item.start_sec, item.end_sec, item.speaker_id),
    )
    for turn in sorted_turns:
        if not merged:
            merged.append(
                DiarizationTurn(
                    start_sec=turn.start_sec,
                    end_sec=turn.end_sec,
                    speaker_id=turn.speaker_id,
                )
            )
            continue
        last = merged[-1]
        same_speaker = turn.speaker_id == last.speaker_id
        nearly_adjacent = turn.start_sec <= (last.end_sec + 0.35)
        if same_speaker and nearly_adjacent:
            last.end_sec = max(last.end_sec, turn.end_sec)
        else:
            merged.append(
                DiarizationTurn(
                    start_sec=turn.start_sec,
                    end_sec=turn.end_sec,
                    speaker_id=turn.speaker_id,
                )
            )
    return merged


def _build_sensevoice_segments_from_diarization(
    text: str,
    meta: SegmentMeta,
    confidence: Optional[float],
    diarization_turns: List[DiarizationTurn],
) -> List[Dict[str, Any]]:
    normalized_text = text.strip()
    merged_turns = _merge_diarization_turns(diarization_turns)
    if not normalized_text or not merged_turns:
        return []

    duration_weights = [max(0.001, turn.end_sec - turn.start_sec) for turn in merged_turns]
    total_weight = sum(duration_weights)
    if total_weight <= 0:
        return []

    total_chars = len(normalized_text)
    running_weight = 0.0
    previous_idx = 0
    output: List[Dict[str, Any]] = []
    for index, turn in enumerate(merged_turns):
        running_weight += duration_weights[index]
        if index == len(merged_turns) - 1:
            end_idx = total_chars
        else:
            end_idx = int(round((running_weight / total_weight) * total_chars))
        end_idx = min(total_chars, max(previous_idx, end_idx))
        chunk_text = normalized_text[previous_idx:end_idx].strip()
        previous_idx = end_idx
        if not chunk_text:
            continue
        start_ms = meta.start_ms + _to_ms(turn.start_sec)
        end_ms = meta.start_ms + _to_ms(turn.end_sec)
        end_ms = max(start_ms, min(end_ms, max(meta.end_ms, meta.start_ms)))
        output.append(
            {
                "startMs": int(start_ms),
                "endMs": int(end_ms),
                "text": chunk_text,
                "confidence": confidence,
                "speakerId": turn.speaker_id,
                "speakerLabel": f"Speaker {turn.speaker_id}",
            }
        )

    return output


def _to_ms(value_sec: float) -> int:
    return max(0, int(round(value_sec * 1000.0)))


def _warn_t2s_once(message: str) -> None:
    global _T2S_WARNED
    if _T2S_WARNED:
        return
    _T2S_WARNED = True
    print(f"[local-stt] t2s normalize disabled: {message}", file=sys.stderr)


def _get_t2s_converter() -> Any:
    global _T2S_CONVERTER, _T2S_CONVERTER_READY
    if _T2S_CONVERTER_READY:
        return _T2S_CONVERTER

    try:
        from opencc import OpenCC  # type: ignore

        _T2S_CONVERTER = OpenCC("t2s")
    except Exception as error:
        _T2S_CONVERTER = None
        _warn_t2s_once(str(error))

    _T2S_CONVERTER_READY = True
    return _T2S_CONVERTER


def _normalize_text_to_simplified(text: str) -> str:
    converter = _get_t2s_converter()
    if converter is None:
        return text

    try:
        return str(converter.convert(text))
    except Exception as error:
        _warn_t2s_once(str(error))
        return text


def _normalize_segments_to_simplified(
    segments: List[Dict[str, Any]],
) -> List[Dict[str, Any]]:
    normalized: List[Dict[str, Any]] = []
    for item in segments:
        text = str(item.get("text", ""))
        updated = dict(item)
        updated["text"] = _normalize_text_to_simplified(text)
        normalized.append(updated)
    return normalized


def _merge_confidence(prev: Any, curr: Any) -> Optional[float]:
    prev_value = float(prev) if isinstance(prev, (int, float)) else None
    curr_value = float(curr) if isinstance(curr, (int, float)) else None
    if prev_value is None:
        return curr_value
    if curr_value is None:
        return prev_value
    return (prev_value + curr_value) / 2.0


def _merge_segments_by_speaker(
    segments: List[Dict[str, Any]],
    max_gap_ms: int,
) -> List[Dict[str, Any]]:
    if not segments:
        return segments

    ordered = sorted(
        segments,
        key=lambda item: (int(item.get("startMs", 0)), int(item.get("endMs", 0))),
    )
    merged: List[Dict[str, Any]] = []
    for item in ordered:
        start_ms = int(item.get("startMs", 0))
        end_ms = int(item.get("endMs", start_ms))
        end_ms = max(start_ms, end_ms)
        text = re.sub(r"\s+", " ", str(item.get("text", "")).strip())
        normalized = {
            "startMs": start_ms,
            "endMs": end_ms,
            "text": text,
            "confidence": item.get("confidence"),
            "speakerId": item.get("speakerId"),
            "speakerLabel": item.get("speakerLabel"),
        }

        if not merged:
            merged.append(normalized)
            continue

        previous = merged[-1]
        same_speaker = (
            previous.get("speakerId") == normalized.get("speakerId")
            and previous.get("speakerLabel") == normalized.get("speakerLabel")
        )
        previous_end = int(previous.get("endMs", 0))
        gap_ms = start_ms - previous_end
        can_merge = (
            same_speaker
            and bool(str(previous.get("text", "")).strip())
            and bool(text)
            and gap_ms <= max_gap_ms
        )
        if can_merge:
            previous_text = str(previous.get("text", "")).strip()
            previous["text"] = f"{previous_text} {text}".strip()
            previous["endMs"] = max(previous_end, end_ms)
            previous["confidence"] = _merge_confidence(
                previous.get("confidence"),
                normalized.get("confidence"),
            )
            continue

        merged.append(normalized)

    return merged


def _short_error_message(value: Optional[str], limit: int = 160) -> str:
    if not value:
        return ""
    text = re.sub(r"\s+", " ", str(value)).strip()
    if len(text) <= limit:
        return text
    return f"{text[: limit - 3]}..."


def _build_diarization_warning(
    diarization_enabled: bool,
    segments: List[Dict[str, Any]],
    diarization_load_error: Optional[str],
) -> Optional[str]:
    if not diarization_enabled:
        return None
    has_speaker = any(
        bool(item.get("speakerId")) or bool(item.get("speakerLabel")) for item in segments
    )
    if has_speaker:
        return None
    compact_error = _short_error_message(diarization_load_error)
    if compact_error:
        return (
            "已开启说话人分离，但本次未生成说话人标识。"
            f"分离模型不可用：{compact_error}"
        )
    return "已开启说话人分离，但本次未生成说话人标识（分离结果为空或质量不足）。"


def run_worker(request: Dict[str, Any]) -> Dict[str, Any]:
    segment_meta = _extract_segment_meta(request.get("segmentMeta", []))
    audio_paths = [str(path) for path in request.get("audioPaths", [])]
    if not audio_paths:
        return {"segments": [], "error": "audioPaths is empty"}

    if not segment_meta:
        cursor = 0
        fallback_meta: List[SegmentMeta] = []
        for path in audio_paths:
            duration_ms = 600000
            fallback_meta.append(
                SegmentMeta(
                    path=path,
                    start_ms=cursor,
                    end_ms=cursor + duration_ms,
                    duration_ms=duration_ms,
                )
            )
            cursor += duration_ms
        segment_meta = fallback_meta

    engine = str(request.get("engine", "whisper"))
    language = str(request.get("language", "auto"))
    device = _normalize_device(str(request.get("computeDevice", "auto")))
    diarization_enabled = bool(request.get("diarizationEnabled", False))
    model_cache_dir = request.get("modelCacheDir")
    model_cache_dir_str = (
        str(model_cache_dir).strip() if isinstance(model_cache_dir, str) else ""
    )
    whisper_model_name = str(request.get("whisperModel", "small"))
    (
        faster_whisper_model_name,
        faster_whisper_hf_repo,
        mlx_whisper_model_name,
    ) = _resolve_whisper_model_names(whisper_model_name)
    sense_voice_model_name = str(request.get("senseVoiceModel", "iic/SenseVoiceSmall"))
    speaker_count_hint = request.get("speakerCountHint")
    min_speakers = request.get("minSpeakers")
    max_speakers = request.get("maxSpeakers")

    if model_cache_dir_str:
        os.environ.setdefault("HF_HOME", model_cache_dir_str)
        os.environ.setdefault("TRANSFORMERS_CACHE", model_cache_dir_str)
        os.environ.setdefault("MODELSCOPE_CACHE", model_cache_dir_str)
    elif os.environ.get("HF_HOME"):
        os.environ.setdefault("MODELSCOPE_CACHE", os.environ["HF_HOME"])

    # Enforce offline mode for deterministic local-only execution.
    os.environ["HF_HUB_OFFLINE"] = "1"
    os.environ["TRANSFORMERS_OFFLINE"] = "1"
    os.environ["HF_DATASETS_OFFLINE"] = "1"
    os.environ["MODELSCOPE_OFFLINE"] = "1"

    diarization_pipeline = None
    diarization_load_error: Optional[str] = None
    if diarization_enabled:
        try:
            diarization_pipeline = _load_pyannote_pipeline(model_cache_dir)
        except Exception as error:
            diarization_load_error = str(error)
            print(
                f"[local-stt] failed to load diarization pipeline, fallback to plain transcription: {error}",
                file=sys.stderr,
            )
            diarization_pipeline = None

    segments_out: List[Dict[str, Any]] = []
    if engine == "whisper":
        if device == "mps":
            ffmpeg_path = _resolve_ffmpeg_executable()
            if not ffmpeg_path:
                return {
                    "segments": [],
                    "error": "ffmpeg not found; install ffmpeg and ensure it is available in PATH",
                }
            mlx_model_source = _resolve_local_hf_snapshot(mlx_whisper_model_name)
            for index, path in enumerate(audio_paths):
                meta = segment_meta[index] if index < len(segment_meta) else segment_meta[-1]
                diarization_turns: List[DiarizationTurn] = []
                if diarization_pipeline is not None:
                    diarization_turns = _run_diarization(
                        path,
                        diarization_pipeline,
                        int(speaker_count_hint) if speaker_count_hint else None,
                        int(min_speakers) if min_speakers else None,
                        int(max_speakers) if max_speakers else None,
                    )

                chunks = _transcribe_whisper_mlx(
                    path,
                    mlx_model_source,
                    language,
                    ffmpeg_path,
                )
                if not chunks:
                    speaker_id, speaker_label = _assign_speaker(
                        diarization_turns,
                        0.0,
                        float(max(0, meta.duration_ms)) / 1000.0,
                    )
                    segments_out.append(
                        {
                            "startMs": int(meta.start_ms),
                            "endMs": int(max(meta.end_ms, meta.start_ms)),
                            "text": "",
                            "confidence": None,
                            "speakerId": speaker_id,
                            "speakerLabel": speaker_label,
                        }
                    )
                    continue

                for chunk in chunks:
                    speaker_id, speaker_label = _assign_speaker(
                        diarization_turns,
                        chunk.start_sec,
                        chunk.end_sec,
                    )
                    chunk_start_ms = meta.start_ms + _to_ms(chunk.start_sec)
                    chunk_end_ms = max(chunk_start_ms, meta.start_ms + _to_ms(chunk.end_sec))
                    segments_out.append(
                        {
                            "startMs": int(chunk_start_ms),
                            "endMs": int(chunk_end_ms),
                            "text": chunk.text,
                            "confidence": chunk.confidence,
                            "speakerId": speaker_id,
                            "speakerLabel": speaker_label,
                        }
                    )
            segments_out.sort(
                key=lambda item: (int(item.get("startMs", 0)), int(item.get("endMs", 0)))
            )
            segments_out = _normalize_segments_to_simplified(segments_out)
            segments_out = _merge_segments_by_speaker(
                segments_out,
                MERGE_SAME_SPEAKER_MAX_GAP_MS,
            )
            warning = _build_diarization_warning(
                diarization_enabled,
                segments_out,
                diarization_load_error,
            )
            return {"segments": segments_out, "error": None, "warning": warning}

        active_device = device
        faster_whisper_source = _resolve_faster_whisper_model_source(
            faster_whisper_model_name,
            faster_whisper_hf_repo,
        )
        try:
            model = _load_whisper_model(
                faster_whisper_source,
                active_device,
                model_cache_dir,
            )
        except Exception as error:
            if _should_fallback_to_cpu(error, active_device):
                print(
                    f"[local-stt] whisper model init failed on '{active_device}', fallback to 'cpu': {error}",
                    file=sys.stderr,
                )
                active_device = "cpu"
                model = _load_whisper_model(
                    faster_whisper_source,
                    active_device,
                    model_cache_dir,
                )
            else:
                raise

        for index, path in enumerate(audio_paths):
            meta = segment_meta[index] if index < len(segment_meta) else segment_meta[-1]
            diarization_turns: List[DiarizationTurn] = []
            if diarization_pipeline is not None:
                diarization_turns = _run_diarization(
                    path,
                    diarization_pipeline,
                    int(speaker_count_hint) if speaker_count_hint else None,
                    int(min_speakers) if min_speakers else None,
                    int(max_speakers) if max_speakers else None,
                )

            try:
                chunks = _transcribe_whisper(
                    path,
                    model,
                    language,
                    bool(request.get("vadEnabled", True)),
                )
            except Exception as error:
                if _should_fallback_to_cpu(error, active_device):
                    print(
                        f"[local-stt] whisper transcribe failed on '{active_device}', fallback to 'cpu': {error}",
                        file=sys.stderr,
                    )
                    active_device = "cpu"
                    model = _load_whisper_model(
                        faster_whisper_source,
                        active_device,
                        model_cache_dir,
                    )
                    chunks = _transcribe_whisper(
                        path,
                        model,
                        language,
                        bool(request.get("vadEnabled", True)),
                    )
                else:
                    raise

            if not chunks:
                speaker_id, speaker_label = _assign_speaker(
                    diarization_turns,
                    0.0,
                    float(max(0, meta.duration_ms)) / 1000.0,
                )
                segments_out.append(
                    {
                        "startMs": int(meta.start_ms),
                        "endMs": int(max(meta.end_ms, meta.start_ms)),
                        "text": "",
                        "confidence": None,
                        "speakerId": speaker_id,
                        "speakerLabel": speaker_label,
                    }
                )
                continue

            for chunk in chunks:
                speaker_id, speaker_label = _assign_speaker(
                    diarization_turns,
                    chunk.start_sec,
                    chunk.end_sec,
                )
                chunk_start_ms = meta.start_ms + _to_ms(chunk.start_sec)
                chunk_end_ms = max(chunk_start_ms, meta.start_ms + _to_ms(chunk.end_sec))
                segments_out.append(
                    {
                        "startMs": int(chunk_start_ms),
                        "endMs": int(chunk_end_ms),
                        "text": chunk.text,
                        "confidence": chunk.confidence,
                        "speakerId": speaker_id,
                        "speakerLabel": speaker_label,
                    }
                )
    elif engine == "sensevoice_small":
        active_device = device
        try:
            model = _load_sensevoice_model(sense_voice_model_name, active_device)
        except Exception as error:
            if _should_fallback_to_cpu(error, active_device):
                print(
                    f"[local-stt] sensevoice model init failed on '{active_device}', fallback to 'cpu': {error}",
                    file=sys.stderr,
                )
                active_device = "cpu"
                model = _load_sensevoice_model(sense_voice_model_name, active_device)
            else:
                raise

        for index, path in enumerate(audio_paths):
            meta = segment_meta[index] if index < len(segment_meta) else segment_meta[-1]
            try:
                text, confidence = _transcribe_sensevoice(path, model, language)
            except Exception as error:
                if _should_fallback_to_cpu(error, active_device):
                    print(
                        f"[local-stt] sensevoice transcribe failed on '{active_device}', fallback to 'cpu': {error}",
                        file=sys.stderr,
                    )
                    active_device = "cpu"
                    model = _load_sensevoice_model(sense_voice_model_name, active_device)
                    text, confidence = _transcribe_sensevoice(path, model, language)
                else:
                    raise

            speaker_id = None
            speaker_label = None
            diarization_turns: List[DiarizationTurn] = []
            if diarization_pipeline is not None:
                diarization_turns = _run_diarization(
                    path,
                    diarization_pipeline,
                    int(speaker_count_hint) if speaker_count_hint else None,
                    int(min_speakers) if min_speakers else None,
                    int(max_speakers) if max_speakers else None,
                )
                diarized_segments = _build_sensevoice_segments_from_diarization(
                    text,
                    meta,
                    confidence,
                    diarization_turns,
                )
                if diarized_segments:
                    segments_out.extend(diarized_segments)
                    continue
                speaker_id, speaker_label = _assign_speaker(
                    diarization_turns,
                    0.0,
                    float(max(0, meta.duration_ms)) / 1000.0,
                )
            segments_out.append(
                {
                    "startMs": int(meta.start_ms),
                    "endMs": int(max(meta.end_ms, meta.start_ms)),
                    "text": text,
                    "confidence": confidence,
                    "speakerId": speaker_id,
                    "speakerLabel": speaker_label,
                }
            )
    else:
        return {"segments": [], "error": f"unsupported local stt engine: {engine}"}

    segments_out.sort(key=lambda item: (int(item.get("startMs", 0)), int(item.get("endMs", 0))))
    segments_out = _normalize_segments_to_simplified(segments_out)
    segments_out = _merge_segments_by_speaker(
        segments_out,
        MERGE_SAME_SPEAKER_MAX_GAP_MS,
    )
    warning = _build_diarization_warning(
        diarization_enabled,
        segments_out,
        diarization_load_error,
    )
    return {"segments": segments_out, "error": None, "warning": warning}


def main() -> int:
    parser = argparse.ArgumentParser(description="Open Recorder Local STT worker")
    parser.add_argument("--request", required=True, help="request JSON file path")
    parser.add_argument("--response", required=True, help="response JSON file path")
    args = parser.parse_args()

    try:
        request = _read_request(args.request)
        response = run_worker(request)
        _write_response(args.response, response)
        if response.get("error"):
            return 1
        return 0
    except Exception as exc:  # noqa: BLE001
        traceback.print_exc()
        error_response = {"segments": [], "error": str(exc)}
        try:
            _write_response(args.response, error_response)
        except Exception:
            pass
        return 1


if __name__ == "__main__":
    sys.exit(main())
