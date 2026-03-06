import { useEffect, useMemo, useState } from "react";

import RecorderTab from "./components/RecorderTab";
import SettingsTab from "./components/SettingsTab";
import SessionsTab from "./components/SessionsTab";
import TabNav, { type AppTab } from "./components/TabNav";
import {
  enqueueSummary,
  enqueueTranscription,
  exportRecording,
  getRecorderStatus,
  getSession,
  getSettings,
  listSessions,
  pauseRecording,
  renameSession,
  resumeRecording,
  startRecording,
  stopRecording,
  updateSettings
} from "./lib/api";
import {
  createTranslator,
  getInitialLocale,
  persistLocale,
  type TranslationParams
} from "./i18n";
import { type Locale, type TranslationKey } from "./i18n/messages";
import type {
  PromptTemplate,
  RecordingQualityPreset,
  SessionDetail,
  SessionSummary,
  Settings
} from "./types/domain";

const emptySettings: Settings = {
  transcriptionProvider: "bailian",
  bailianBaseUrl: "https://dashscope.aliyuncs.com",
  bailianTranscriptionModel: "paraformer-v2",
  bailianSummaryModel: "qwen-plus",
  bailianOssPathPrefix: "open-recorder",
  bailianOssSignedUrlTtlSeconds: 1800,
  aliyunEndpoint: "https://tingwu.cn-beijing.aliyuncs.com",
  aliyunSourceLanguage: "cn",
  aliyunTranscriptionNormalizationEnabled: true,
  aliyunTranscriptionParagraphEnabled: true,
  aliyunTranscriptionPunctuationPredictionEnabled: true,
  aliyunTranscriptionDisfluencyRemovalEnabled: false,
  aliyunTranscriptionSpeakerDiarizationEnabled: true,
  aliyunPollIntervalSeconds: 60,
  aliyunMaxPollingMinutes: 180,
  defaultTemplateId: "meeting-default",
  templates: [],
  bailianApiKey: ""
};
const initialSettings = normalizeSettings(emptySettings);

const WAVEFORM_CAPACITY = 220;

type StatusState = {
  key: TranslationKey;
  params?: TranslationParams;
};

function createDefaultTemplate(): PromptTemplate {
  return {
    id: "meeting-default",
    name: "Meeting Default",
    systemPrompt: "You are an assistant for writing concise meeting notes.",
    userPrompt: "Organize transcript into: conclusion, action items, risks, timeline.",
    variables: ["language", "audience"]
  };
}

function normalizeSettings(input: Settings): Settings {
  const templates = input.templates.length > 0 ? input.templates : [createDefaultTemplate()];
  const defaultExists = templates.some((template) => template.id === input.defaultTemplateId);
  const ttl = Number.isFinite(input.bailianOssSignedUrlTtlSeconds)
    ? Math.min(86400, Math.max(60, Math.floor(input.bailianOssSignedUrlTtlSeconds)))
    : 1800;
  const aliyunPollIntervalSeconds = Number.isFinite(input.aliyunPollIntervalSeconds)
    ? Math.min(300, Math.max(60, Math.floor(input.aliyunPollIntervalSeconds)))
    : 60;
  const aliyunMaxPollingMinutes = Number.isFinite(input.aliyunMaxPollingMinutes)
    ? Math.min(720, Math.max(5, Math.floor(input.aliyunMaxPollingMinutes)))
    : 180;

  return {
    ...input,
    bailianOssSignedUrlTtlSeconds: ttl,
    aliyunPollIntervalSeconds,
    aliyunMaxPollingMinutes,
    templates,
    defaultTemplateId: defaultExists ? input.defaultTemplateId : templates[0].id
  };
}

function App() {
  const [activeTab, setActiveTab] = useState<AppTab>("recorder");
  const [locale, setLocale] = useState<Locale>(getInitialLocale);
  const [sessions, setSessions] = useState<SessionSummary[]>([]);
  const [activeSessionId, setActiveSessionId] = useState<string>();
  const [activeSession, setActiveSession] = useState<SessionDetail>();
  const [currentRecording, setCurrentRecording] = useState<string>();
  const [recordingQuality, setRecordingQuality] = useState<RecordingQualityPreset>("standard");
  const [recordingElapsedMs, setRecordingElapsedMs] = useState<number>(0);
  const [waveformPoints, setWaveformPoints] = useState<number[]>([]);
  const [settings, setSettings] = useState<Settings>(initialSettings);
  const [summaryTemplateId, setSummaryTemplateId] = useState<string>(initialSettings.defaultTemplateId);
  const [statusState, setStatusState] = useState<StatusState>({ key: "status.ready" });

  const t = useMemo(() => createTranslator(locale), [locale]);
  const statusMessage = t(statusState.key, statusState.params);
  const canRecord = useMemo(() => !currentRecording, [currentRecording]);
  const canExport = useMemo(
    () => Boolean(activeSessionId && activeSession && activeSession.audioSegments.length > 0),
    [activeSession, activeSessionId]
  );

  function setStatus(key: TranslationKey, params?: TranslationParams) {
    setStatusState({ key, params });
  }

  async function refreshSessions() {
    const data = await listSessions();
    setSessions(data);
    if (data.length === 0) {
      setActiveSessionId(undefined);
      setActiveSession(undefined);
      return;
    }
    if (!activeSessionId || !data.some((session) => session.id === activeSessionId)) {
      setActiveSessionId(data[0].id);
    }
  }

  async function refreshSessionDetail(sessionId: string) {
    const detail = await getSession(sessionId);
    setActiveSession(detail);
  }

  useEffect(() => {
    void refreshSessions().catch((error) => {
      setStatus("status.sessionsLoadFailed", { error: String(error) });
    });

    void getSettings()
      .then((loaded) => {
        const normalized = normalizeSettings(loaded);
        setSettings(normalized);
        setSummaryTemplateId(normalized.defaultTemplateId);
      })
      .catch((error) => setStatus("status.settingsLoadFailed", { error: String(error) }));
  }, []);

  useEffect(() => {
    persistLocale(locale);
  }, [locale]);

  useEffect(() => {
    if (!activeSessionId) {
      setActiveSession(undefined);
      return;
    }
    void refreshSessionDetail(activeSessionId).catch((error) => {
      setStatus("status.sessionsLoadFailed", { error: String(error) });
    });
  }, [activeSessionId]);

  useEffect(() => {
    if (!settings.templates.some((template) => template.id === summaryTemplateId)) {
      setSummaryTemplateId(settings.defaultTemplateId);
    }
  }, [settings.defaultTemplateId, settings.templates, summaryTemplateId]);

  useEffect(() => {
    if (!currentRecording) {
      return;
    }

    let disposed = false;
    let failureNotified = false;

    const poll = async () => {
      try {
        const status = await getRecorderStatus(currentRecording);
        if (disposed) {
          return;
        }
        setRecordingElapsedMs(status.elapsedMs);
        setWaveformPoints((previous) => {
          const next = [...previous, status.rms];
          if (next.length > WAVEFORM_CAPACITY) {
            next.splice(0, next.length - WAVEFORM_CAPACITY);
          }
          return next;
        });
      } catch (error) {
        if (!disposed && !failureNotified) {
          failureNotified = true;
          setStatus("status.recorderStatusFailed", { error: String(error) });
        }
      }
    };

    void poll();
    const timer = window.setInterval(() => {
      void poll();
    }, 250);

    return () => {
      disposed = true;
      window.clearInterval(timer);
    };
  }, [currentRecording]);

  async function onStart() {
    try {
      const sessionId = await startRecording(undefined, recordingQuality);
      setCurrentRecording(sessionId);
      setActiveSessionId(sessionId);
      setRecordingElapsedMs(0);
      setWaveformPoints([]);
      setStatus("status.recordingSession", { sessionId });
      await refreshSessions();
      await refreshSessionDetail(sessionId);
    } catch (error) {
      setStatus("status.startRecordingFailed", { error: String(error) });
    }
  }

  async function onPause() {
    if (!currentRecording) return;
    try {
      await pauseRecording(currentRecording);
      setStatus("status.recordingPaused");
      await refreshSessionDetail(currentRecording);
      await refreshSessions();
    } catch (error) {
      setStatus("status.pauseFailed", { error: String(error) });
    }
  }

  async function onResume() {
    if (!currentRecording) return;
    try {
      await resumeRecording(currentRecording);
      setStatus("status.recordingResumed");
      await refreshSessionDetail(currentRecording);
      await refreshSessions();
    } catch (error) {
      setStatus("status.resumeFailed", { error: String(error) });
    }
  }

  async function onStop() {
    if (!currentRecording) {
      return;
    }

    try {
      await stopRecording(currentRecording);
      setStatus("status.recordingStopped");
      await refreshSessionDetail(currentRecording);
      await refreshSessions();
    } catch (error) {
      setStatus("status.stopRecordingFailed", { error: String(error) });
    } finally {
      setCurrentRecording(undefined);
      setWaveformPoints([]);
    }
  }

  async function onExport(format: "wav" | "mp3") {
    if (!activeSessionId) {
      return;
    }

    try {
      const path = await exportRecording(activeSessionId, format);
      setStatus("status.exportFinished", { path });
      await refreshSessionDetail(activeSessionId);
      await refreshSessions();
    } catch (error) {
      setStatus("status.exportFailed", { error: String(error) });
    }
  }

  async function onTranscribe() {
    if (!activeSessionId) return;

    try {
      const jobId = await enqueueTranscription(activeSessionId);
      setStatus("status.transcriptionFinished", { jobId });
      await refreshSessionDetail(activeSessionId);
      await refreshSessions();
    } catch (error) {
      setStatus("status.transcriptionFailed", { error: String(error) });
    }
  }

  async function onSummarize() {
    if (!activeSessionId) return;

    try {
      const jobId = await enqueueSummary(activeSessionId, summaryTemplateId || settings.defaultTemplateId);
      setStatus("status.summaryFinished", { jobId });
      await refreshSessionDetail(activeSessionId);
      await refreshSessions();
    } catch (error) {
      setStatus("status.summaryFailed", { error: String(error) });
    }
  }

  async function onRenameSession(sessionId: string, name: string) {
    const normalized = name.trim();
    const nextName = normalized.length > 0 ? normalized : undefined;
    setSessions((previous) =>
      previous.map((session) =>
        session.id === sessionId ? { ...session, name: nextName } : session
      )
    );
    setActiveSession((previous) =>
      previous && previous.id === sessionId ? { ...previous, name: nextName } : previous
    );

    try {
      await renameSession(sessionId, name);
      await refreshSessions();
      if (activeSessionId === sessionId) {
        await refreshSessionDetail(sessionId);
      }
      setStatus("status.sessionRenameFinished");
    } catch (error) {
      await refreshSessions();
      if (activeSessionId === sessionId) {
        await refreshSessionDetail(sessionId);
      }
      setStatus("status.sessionRenameFailed", { error: String(error) });
    }
  }

  async function onSaveSettings() {
    try {
      const updated = await updateSettings(settings);
      const normalized = normalizeSettings(updated);
      setSettings(normalized);
      setSummaryTemplateId((previous) =>
        normalized.templates.some((template) => template.id === previous)
          ? previous
          : normalized.defaultTemplateId
      );
      setStatus("status.settingsSaved");
    } catch (error) {
      setStatus("status.settingsSaveFailed", { error: String(error) });
    }
  }

  return (
    <main className="app-shell">
      <header className="app-header panel">
        <div className="app-header-top">
          <h1>{t("app.title")}</h1>
          <span className="status-badge">{statusMessage}</span>
        </div>
        <TabNav activeTab={activeTab} onChange={setActiveTab} t={t} />
      </header>

      {activeTab === "recorder" && (
        <RecorderTab
          statusMessage={statusMessage}
          canRecord={canRecord}
          hasRecording={Boolean(currentRecording)}
          canExport={canExport}
          elapsedMs={recordingElapsedMs}
          waveformPoints={waveformPoints}
          qualityPreset={recordingQuality}
          onQualityChange={setRecordingQuality}
          onStart={() => void onStart()}
          onPause={() => void onPause()}
          onResume={() => void onResume()}
          onStop={() => void onStop()}
          onExportWav={() => void onExport("wav")}
          onExportMp3={() => void onExport("mp3")}
          t={t}
        />
      )}

      {activeTab === "sessions" && (
        <SessionsTab
          sessions={sessions}
          templates={settings.templates}
          activeSessionId={activeSessionId}
          activeSession={activeSession}
          summaryTemplateId={summaryTemplateId}
          onSummaryTemplateChange={setSummaryTemplateId}
          onRefresh={() =>
            void refreshSessions().catch((error) => {
              setStatus("status.sessionsLoadFailed", { error: String(error) });
            })
          }
          onSelectSession={setActiveSessionId}
          onRenameSession={(sessionId, name) => void onRenameSession(sessionId, name)}
          onTranscribe={() => void onTranscribe()}
          onSummarize={() => void onSummarize()}
          t={t}
        />
      )}

      {activeTab === "settings" && (
        <SettingsTab
          locale={locale}
          settings={settings}
          onLocaleChange={setLocale}
          onSettingsChange={(patch) => {
            setSettings((previous) => normalizeSettings({ ...previous, ...patch }));
          }}
          onSave={() => void onSaveSettings()}
          t={t}
        />
      )}
    </main>
  );
}

export default App;
