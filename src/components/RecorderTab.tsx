import { useEffect, useMemo, useRef } from "react";

import type { Translator } from "../i18n";
import type { RecordingQualityPreset } from "../types/domain";

type RecorderTabProps = {
  statusMessage: string;
  canRecord: boolean;
  hasRecording: boolean;
  canExport: boolean;
  elapsedMs: number;
  waveformPoints: number[];
  qualityPreset: RecordingQualityPreset;
  onQualityChange: (preset: RecordingQualityPreset) => void;
  onStart: () => void;
  onPause: () => void;
  onResume: () => void;
  onStop: () => void;
  onExportM4a: () => void;
  onExportMp3: () => void;
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

function IconDownload({ title }: IconProps) {
  return (
    <svg viewBox="0 0 24 24" aria-hidden="true" focusable="false">
      <title>{title}</title>
      <path d="M11 4h2v8h3l-4 5-4-5h3z" fill="currentColor" />
      <rect x="5" y="19" width="14" height="2" fill="currentColor" />
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

function RecorderTab({
  statusMessage,
  canRecord,
  hasRecording,
  canExport,
  elapsedMs,
  waveformPoints,
  qualityPreset,
  onQualityChange,
  onStart,
  onPause,
  onResume,
  onStop,
  onExportM4a,
  onExportMp3,
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
      const normalized = Math.min(1, Math.max(0, point));
      const y = height - normalized * height;
      if (index === 0) {
        context.moveTo(x, y);
      } else {
        context.lineTo(x, y);
      }
    });

    context.stroke();
  }, [waveformPoints]);

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
              <option value="standard">{t("recorder.quality.standard")}</option>
              <option value="hd">{t("recorder.quality.hd")}</option>
              <option value="hifi">{t("recorder.quality.hifi")}</option>
            </select>
          </label>
          <p className="segment-hint">{t("recorder.segmentLength")}</p>
        </div>
      </header>

      <div className="recorder-timer-section">
        <div className="recorder-duration">{durationLabel}</div>
        <div className="recorder-status-row">
          <span className="status-chip">{statusMessage}</span>
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

      <div className="export-grid">
        <button
          type="button"
          onClick={onExportM4a}
          disabled={!canExport}
          className="icon-button secondary"
          aria-label={t("recorder.exportM4a")}
        >
          <IconDownload title={t("recorder.exportM4a")} />
          <span>{t("recorder.exportM4a")}</span>
        </button>

        <button
          type="button"
          onClick={onExportMp3}
          disabled={!canExport}
          className="icon-button secondary"
          aria-label={t("recorder.exportMp3")}
        >
          <IconDownload title={t("recorder.exportMp3")} />
          <span>{t("recorder.exportMp3")}</span>
        </button>
      </div>
    </section>
  );
}

export default RecorderTab;
