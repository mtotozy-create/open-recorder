#!/usr/bin/env python3

import argparse
import json
import os
import sys
import traceback
from dataclasses import dataclass
from typing import Any, Dict, List, Optional, Tuple


@dataclass
class SegmentMeta:
    path: str
    start_ms: int
    end_ms: int
    duration_ms: int


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
    if not device or device == "auto":
        return "cpu"
    return device


def _dominant_speaker_label(
    audio_path: str,
    diarization_pipeline: Any,
    speaker_count_hint: Optional[int],
    min_speakers: Optional[int],
    max_speakers: Optional[int],
) -> Tuple[Optional[str], Optional[str]]:
    kwargs: Dict[str, Any] = {}
    if speaker_count_hint:
        kwargs["num_speakers"] = speaker_count_hint
    else:
        if min_speakers:
            kwargs["min_speakers"] = min_speakers
        if max_speakers:
            kwargs["max_speakers"] = max_speakers

    diarization = diarization_pipeline(audio_path, **kwargs)
    duration_by_speaker: Dict[str, float] = {}
    for turn, _, speaker in diarization.itertracks(yield_label=True):
        duration_by_speaker[speaker] = duration_by_speaker.get(speaker, 0.0) + (
            float(turn.end) - float(turn.start)
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

    return Pipeline.from_pretrained("pyannote/speaker-diarization-3.1")


def _transcribe_whisper(
    audio_path: str,
    model: Any,
    language: str,
    vad_enabled: bool,
) -> Tuple[str, Optional[float]]:
    language_arg = None if language == "auto" else language
    segments, _ = model.transcribe(
        audio_path,
        language=language_arg,
        vad_filter=bool(vad_enabled),
    )
    text_parts: List[str] = []
    confidence_values: List[float] = []
    for segment in segments:
        text = str(getattr(segment, "text", "")).strip()
        if text:
            text_parts.append(text)
        avg_logprob = getattr(segment, "avg_logprob", None)
        if isinstance(avg_logprob, (int, float)):
            confidence_values.append(float(avg_logprob))
    text = " ".join(text_parts).strip()
    confidence = None
    if confidence_values:
        confidence = max(0.0, min(1.0, 1.0 + sum(confidence_values) / len(confidence_values)))
    return text, confidence


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


def _transcribe_sensevoice(
    audio_path: str,
    model: Any,
    language: str,
) -> Tuple[str, Optional[float]]:
    language_arg = None if language == "auto" else language
    result = model.generate(input=audio_path, language=language_arg, use_itn=True)
    text = _extract_text_from_obj(result) or ""
    return text.strip(), None


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
    whisper_model_name = str(request.get("whisperModel", "small"))
    sense_voice_model_name = str(request.get("senseVoiceModel", "iic/SenseVoiceSmall"))
    speaker_count_hint = request.get("speakerCountHint")
    min_speakers = request.get("minSpeakers")
    max_speakers = request.get("maxSpeakers")

    if model_cache_dir:
        os.environ.setdefault("HF_HOME", str(model_cache_dir))
        os.environ.setdefault("TRANSFORMERS_CACHE", str(model_cache_dir))

    diarization_pipeline = None
    if diarization_enabled:
        diarization_pipeline = _load_pyannote_pipeline(model_cache_dir)

    segments_out: List[Dict[str, Any]] = []
    if engine == "whisper":
        from faster_whisper import WhisperModel  # type: ignore

        model = WhisperModel(
            whisper_model_name,
            device=device,
            compute_type="float16" if device in ("cuda", "mps") else "int8",
            download_root=model_cache_dir,
        )
        for index, path in enumerate(audio_paths):
            meta = segment_meta[index] if index < len(segment_meta) else segment_meta[-1]
            text, confidence = _transcribe_whisper(
                path,
                model,
                language,
                bool(request.get("vadEnabled", True)),
            )
            speaker_id = None
            speaker_label = None
            if diarization_pipeline is not None:
                speaker_id, speaker_label = _dominant_speaker_label(
                    path,
                    diarization_pipeline,
                    int(speaker_count_hint) if speaker_count_hint else None,
                    int(min_speakers) if min_speakers else None,
                    int(max_speakers) if max_speakers else None,
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
    elif engine == "sensevoice_small":
        from funasr import AutoModel  # type: ignore

        model = AutoModel(
            model=sense_voice_model_name,
            trust_remote_code=True,
            device=device,
        )
        for index, path in enumerate(audio_paths):
            meta = segment_meta[index] if index < len(segment_meta) else segment_meta[-1]
            text, confidence = _transcribe_sensevoice(path, model, language)
            speaker_id = None
            speaker_label = None
            if diarization_pipeline is not None:
                speaker_id, speaker_label = _dominant_speaker_label(
                    path,
                    diarization_pipeline,
                    int(speaker_count_hint) if speaker_count_hint else None,
                    int(min_speakers) if min_speakers else None,
                    int(max_speakers) if max_speakers else None,
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

    return {"segments": segments_out, "error": None}


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
