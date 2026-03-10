import { useEffect, useMemo, useRef } from "react";

import type { Translator } from "../i18n";
import type { RecordingQualityPreset, TranscriptSegment } from "../types/domain";

type RecorderTabProps = {
  statusMessage: string;
  canRecord: boolean;
  hasRecording: boolean;
  elapsedMs: number;
  waveformPoints: number[];
  realtimeEnabled: boolean;
  realtimeToggleDisabled: boolean;
  realtimeSourceLanguage: string;
  realtimeSourceLanguageDisabled: boolean;
  realtimeTranslationEnabled: boolean;
  realtimeTranslationToggleDisabled: boolean;
  realtimeTranslationTargetLanguage: string;
  realtimeTranslationTargetDisabled: boolean;
  realtimePreviewText: string;
  realtimeSegments: TranscriptSegment[];
  realtimeState: "idle" | "connecting" | "running" | "paused" | "stopping" | "error";
  realtimeLastError?: string;
  qualityPreset: RecordingQualityPreset;
  onQualityChange: (preset: RecordingQualityPreset) => void;
  onRealtimeToggle: (enabled: boolean) => void;
  onRealtimeSourceLanguageChange: (sourceLanguage: string) => void;
  onRealtimeTranslationToggle: (enabled: boolean) => void;
  onRealtimeTranslationTargetChange: (targetLanguage: string) => void;
  onStart: () => void;
  onPause: () => void;
  onResume: () => void;
  onStop: () => void;
  t: Translator;
};

type IconProps = {
  title: string;
};

function IconPlay({ title }: IconProps) {
  return (
    <svg viewBox="0 0 24 24" aria-hidden="true" focusable="false">
      <title>{title}</title>
      <path d="M8 5v14l11-7z" fill="currentColor" />
    </svg>
  );
}

function IconPause({ title }: IconProps) {
  return (
    <svg viewBox="0 0 24 24" aria-hidden="true" focusable="false">
      <title>{title}</title>
      <rect x="6" y="5" width="4" height="14" fill="currentColor" />
      <rect x="14" y="5" width="4" height="14" fill="currentColor" />
    </svg>
  );
}

function IconResume({ title }: IconProps) {
  return (
    <svg viewBox="0 0 24 24" aria-hidden="true" focusable="false">
      <title>{title}</title>
      <path d="M7 5v14l7-7z" fill="currentColor" />
      <rect x="16" y="5" width="2" height="14" fill="currentColor" />
    </svg>
  );
}

function IconStop({ title }: IconProps) {
  return (
    <svg viewBox="0 0 24 24" aria-hidden="true" focusable="false">
      <title>{title}</title>
      <rect x="6" y="6" width="12" height="12" fill="currentColor" />
    </svg>
  );
}

function formatDuration(ms: number): string {
  const totalSeconds = Math.max(0, Math.floor(ms / 1000));
  const hours = Math.floor(totalSeconds / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  const seconds = totalSeconds % 60;
  const hourPart = String(hours).padStart(2, "0");
  const minutePart = String(minutes).padStart(2, "0");
  const secondPart = String(seconds).padStart(2, "0");
  return `${hourPart}:${minutePart}:${secondPart}`;
}

function formatSegmentTime(ms: number): string {
  const totalSeconds = Math.floor(ms / 1000);
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;
  return `${String(minutes).padStart(2, "0")}:${String(seconds).padStart(2, "0")}`;
}

function RecorderTab({
  statusMessage,
  canRecord,
  hasRecording,
  elapsedMs,
  waveformPoints,
  realtimeEnabled,
  realtimeToggleDisabled,
  realtimeSourceLanguage,
  realtimeSourceLanguageDisabled,
  realtimeTranslationEnabled,
  realtimeTranslationToggleDisabled,
  realtimeTranslationTargetLanguage,
  realtimeTranslationTargetDisabled,
  realtimePreviewText,
  realtimeSegments,
  realtimeState,
  realtimeLastError,
  qualityPreset,
  onQualityChange,
  onRealtimeToggle,
  onRealtimeSourceLanguageChange,
  onRealtimeTranslationToggle,
  onRealtimeTranslationTargetChange,
  onStart,
  onPause,
  onResume,
  onStop,
  t
}: RecorderTabProps) {
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const durationLabel = useMemo(() => formatDuration(elapsedMs), [elapsedMs]);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) {
      return;
    }

    const context = canvas.getContext("2d");
    if (!context) {
      return;
    }

    const width = canvas.clientWidth;
    const height = canvas.clientHeight;
    const ratio = window.devicePixelRatio || 1;

    canvas.width = Math.max(1, Math.floor(width * ratio));
    canvas.height = Math.max(1, Math.floor(height * ratio));
    context.setTransform(ratio, 0, 0, ratio, 0, 0);

    context.clearRect(0, 0, width, height);

    context.fillStyle = "#f1f5f9";
    context.fillRect(0, 0, width, height);

    context.strokeStyle = "#cbd5e1";
    context.lineWidth = 1;
    context.beginPath();
    context.moveTo(0, height / 2);
    context.lineTo(width, height / 2);
    context.stroke();

    if (waveformPoints.length === 0) {
      return;
    }

    const step = width / Math.max(1, waveformPoints.length - 1);
    context.strokeStyle = "#2563eb";
    context.lineWidth = 2;
    context.beginPath();

    waveformPoints.forEach((point, index) => {
      const x = index * step;
      const normalized = Math.min(1, Math.max(0, point * 2.4));
      const y = height - normalized * height;
      if (index === 0) {
        context.moveTo(x, y);
      } else {
        context.lineTo(x, y);
      }
    });

    context.stroke();
  }, [waveformPoints]);

  const realtimeStateLabel = (() => {
    switch (realtimeState) {
      case "connecting":
        return t("recorder.realtime.state.connecting");
      case "running":
        return t("recorder.realtime.state.running");
      case "paused":
        return t("recorder.realtime.state.paused");
      case "stopping":
        return t("recorder.realtime.state.stopping");
      case "error":
        return t("recorder.realtime.state.error");
      default:
        return t("recorder.realtime.state.idle");
    }
  })();

  const realtimeStateClass = `realtime-state-badge state-${realtimeState}`;
  const hasRealtimeSegments = realtimeSegments.length > 0;

  return (
    <section className="panel recorder-panel">
      <header className="recorder-header">
        <div>
          <h2>{t("recorder.title")}</h2>
          <p>{t("recorder.subtitle")}</p>
        </div>
        <div className="recorder-controls-row" style={{ margin: 0 }}>
          <label className="quality-select">
            <span>{t("recorder.quality")}</span>
            <select
              value={qualityPreset}
              onChange={(event) => onQualityChange(event.target.value as RecordingQualityPreset)}
              disabled={hasRecording}
            >
              <option value="voice_low_storage">{t("recorder.quality.voiceLowStorage")}</option>
              <option value="legacy_compatible">{t("recorder.quality.legacyCompatible")}</option>
              <option value="standard">{t("recorder.quality.standard")}</option>
              <option value="hd">{t("recorder.quality.hd")}</option>
              <option value="hifi">{t("recorder.quality.hifi")}</option>
            </select>
          </label>
        </div>
      </header>

      <div className="recorder-timer-section">
        <div className="recorder-timer-row">
          <div className="recorder-duration">{durationLabel}</div>
          <div className="recorder-status-row">
            <span className="status-chip">{statusMessage}</span>
          </div>
        </div>
      </div>

      <div className="waveform-panel" role="img" aria-label={t("recorder.waveform")}>
        <div className="waveform-header">{t("recorder.waveform")}</div>
        <canvas ref={canvasRef} />
      </div>

      <div className="icon-button-grid">
        <button
          type="button"
          onClick={onStart}
          disabled={!canRecord}
          className="icon-button"
          aria-label={t("recorder.start")}
        >
          <IconPlay title={t("recorder.start")} />
          <span>{t("recorder.start")}</span>
        </button>

        <button
          type="button"
          onClick={onPause}
          disabled={!hasRecording}
          className="icon-button"
          aria-label={t("recorder.pause")}
        >
          <IconPause title={t("recorder.pause")} />
          <span>{t("recorder.pause")}</span>
        </button>

        <button
          type="button"
          onClick={onResume}
          disabled={!hasRecording}
          className="icon-button"
          aria-label={t("recorder.resume")}
        >
          <IconResume title={t("recorder.resume")} />
          <span>{t("recorder.resume")}</span>
        </button>

        <button
          type="button"
          onClick={onStop}
          disabled={!hasRecording}
          className="icon-button danger"
          aria-label={t("recorder.stop")}
        >
          <IconStop title={t("recorder.stop")} />
          <span>{t("recorder.stop")}</span>
        </button>
      </div>

      <div className="realtime-preview-panel">
        <div className="realtime-preview-top">
          <div className="realtime-preview-header">
            <span>{t("recorder.realtime.title")}</span>
            <strong className={realtimeStateClass}>{realtimeStateLabel}</strong>
          </div>
          <div className="realtime-controls">
            <label className="ios-switch-control with-label" aria-label={t("recorder.realtime.toggle")}>
              <span className="ios-switch-label">{t("recorder.realtime.toggle")}</span>
              <input
                type="checkbox"
                checked={realtimeEnabled}
                disabled={realtimeToggleDisabled}
                onChange={(event) => onRealtimeToggle(event.target.checked)}
              />
              <span className="ios-switch-track">
                <span className="ios-switch-thumb" />
              </span>
            </label>
            <label className="ios-switch-control with-label" aria-label={t("recorder.realtime.translationToggle")}>
              <span className="ios-switch-label">{t("recorder.realtime.translationToggle")}</span>
              <input
                type="checkbox"
                checked={realtimeTranslationEnabled}
                disabled={realtimeTranslationToggleDisabled}
                onChange={(event) => onRealtimeTranslationToggle(event.target.checked)}
              />
              <span className="ios-switch-track">
                <span className="ios-switch-thumb" />
              </span>
            </label>
            <label className="realtime-translation-target">
              <span>{t("recorder.realtime.sourceLanguage")}</span>
              <select
                value={realtimeSourceLanguage}
                disabled={realtimeSourceLanguageDisabled}
                onChange={(event) => onRealtimeSourceLanguageChange(event.target.value)}
              >
                <option value="cn">{t("recorder.realtime.sourceLanguage.cn")}</option>
                <option value="en">{t("recorder.realtime.sourceLanguage.en")}</option>
              </select>
            </label>
            <label className="realtime-translation-target">
              <span>{t("recorder.realtime.translationTarget")}</span>
              <select
                value={realtimeTranslationTargetLanguage}
                disabled={realtimeTranslationTargetDisabled}
                onChange={(event) => onRealtimeTranslationTargetChange(event.target.value)}
              >
                <option value="en">{t("recorder.realtime.translationTargetLanguage.en")}</option>
                <option value="cn">{t("recorder.realtime.translationTargetLanguage.zh")}</option>
                <option value="ja">{t("recorder.realtime.translationTargetLanguage.ja")}</option>
                <option value="ko">{t("recorder.realtime.translationTargetLanguage.ko")}</option>
                <option value="fr">{t("recorder.realtime.translationTargetLanguage.fr")}</option>
                <option value="de">{t("recorder.realtime.translationTargetLanguage.de")}</option>
                <option value="es">{t("recorder.realtime.translationTargetLanguage.es")}</option>
                <option value="ru">{t("recorder.realtime.translationTargetLanguage.ru")}</option>
              </select>
            </label>
          </div>
        </div>
        {realtimeLastError ? <p className="realtime-preview-error">{realtimeLastError}</p> : null}
        {!hasRealtimeSegments && !realtimePreviewText && (
          <p className="empty-hint">{t("recorder.realtime.empty")}</p>
        )}
        {hasRealtimeSegments && (
          <ul className="transcript-list realtime-transcript-list">
            {realtimeSegments.map((segment, index) => (
              <li
                key={`${index}-${segment.startMs}-${segment.endMs}`}
                className="realtime-transcript-item"
              >
                <span className="realtime-segment-time">
                  {formatSegmentTime(segment.startMs)} - {formatSegmentTime(segment.endMs)}
                </span>
                <span className="realtime-segment-content">
                  <span className="realtime-segment-text">{segment.text}</span>
                  {realtimeTranslationEnabled ? (
                    <span className="realtime-segment-translation">
                      {segment.translationText?.trim() || t("recorder.realtime.translationUnavailable")}
                    </span>
                  ) : null}
                </span>
              </li>
            ))}
          </ul>
        )}
        {!hasRealtimeSegments && realtimePreviewText && (
          <pre className="realtime-preview-fallback">{realtimePreviewText}</pre>
        )}
      </div>
    </section>
  );
}

export default RecorderTab;
