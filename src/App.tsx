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
  getSessionJobs,
  getJob,
  listSessions,
  deleteSession,
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
  JobInfo,
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
  const [sessionJobs, setSessionJobs] = useState<JobInfo[]>([]);
  const [currentRecording, setCurrentRecording] = useState<string>();
  const [recordingQuality, setRecordingQuality] = useState<RecordingQualityPreset>("standard");
  const [recordingElapsedMs, setRecordingElapsedMs] = useState<number>(0);
  const [waveformPoints, setWaveformPoints] = useState<number[]>([]);
  const [settings, setSettings] = useState<Settings>(initialSettings);
  const [summaryTemplateId, setSummaryTemplateId] = useState<string>(initialSettings.defaultTemplateId);
  const [statusState, setStatusState] = useState<StatusState>({ key: "status.ready" });
  const [isTranscribing, setIsTranscribing] = useState(false);
  const [isSummarizing, setIsSummarizing] = useState(false);

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

  /**
   * 格式化秒数为 mm:ss 字符串
   */
  function formatElapsed(seconds: number): string {
    const m = Math.floor(seconds / 60);
    const s = seconds % 60;
    return `${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}`;
  }

  /**
   * 启动状态栏计时器，每秒更新显示已用时间
   * 返回清理函数
   */
  function startElapsedTimer(statusKey: TranslationKey): () => void {
    const startedAt = Date.now();
    const update = () => {
      const elapsed = Math.floor((Date.now() - startedAt) / 1000);
      setStatus(statusKey, { elapsed: formatElapsed(elapsed) });
    };
    update();
    const id = window.setInterval(update, 1000);
    return () => window.clearInterval(id);
  }

  async function refreshSessions() {
    const data = await listSessions();
    setSessions(data);
    if (data.length === 0) {
      setActiveSessionId(undefined);
      setActiveSession(undefined);
      setSessionJobs([]);
      return;
    }
    if (!activeSessionId || !data.some((session) => session.id === activeSessionId)) {
      setActiveSessionId(data[0].id);
    }
  }

  async function refreshSessionDetail(sessionId: string) {
    const detail = await getSession(sessionId);
    setActiveSession(detail);
    const jobs = await getSessionJobs(sessionId);
    setSessionJobs(jobs);
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
      setSessionJobs([]);
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

  async function onExport(format: "m4a" | "mp3") {
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
    if (!activeSessionId || isTranscribing) return;

    setIsTranscribing(true);
    const stopTimer = startElapsedTimer("status.transcriptionRunning");

    try {
      // 后端立即返回 jobId，实际转写在后台线程执行
      const jobId = await enqueueTranscription(activeSessionId);

      // 轮询 job 状态直到完成或失败
      const pollResult = await pollJobUntilDone(jobId);

      if (pollResult.status === "completed") {
        setStatus("status.transcriptionFinished", { jobId });
      } else {
        setStatus("status.transcriptionFailed", {
          error: pollResult.error || "unknown error"
        });
      }

      await refreshSessionDetail(activeSessionId);
      await refreshSessions();
    } catch (error) {
      setStatus("status.transcriptionFailed", { error: String(error) });
    } finally {
      stopTimer();
      setIsTranscribing(false);
    }
  }

  /**
   * 轮询 job 状态直到完成或失败
   * 每 3 秒查询一次，最长等待 180 分钟
   */
  async function pollJobUntilDone(
    jobId: string
  ): Promise<{ status: string; error?: string }> {
    const maxAttempts = 3600; // 3 * 3600 = 10800 秒 = 3 小时
    for (let attempt = 0; attempt < maxAttempts; attempt++) {
      await sleep(3000);
      try {
        const job = await getJob(jobId);
        if (job.status === "completed") {
          return { status: "completed" };
        }
        if (job.status === "failed") {
          return { status: "failed", error: job.error ?? undefined };
        }
        // 仍在运行，继续轮询
      } catch {
        // 获取状态失败，继续重试
      }
    }
    return { status: "failed", error: "polling timed out" };
  }

  function sleep(ms: number): Promise<void> {
    return new Promise((resolve) => setTimeout(resolve, ms));
  }

  async function onSummarize() {
    if (!activeSessionId || isSummarizing) return;

    setIsSummarizing(true);
    const stopTimer = startElapsedTimer("status.summaryRunning");

    try {
      const jobId = await enqueueSummary(activeSessionId, summaryTemplateId || settings.defaultTemplateId);
      setStatus("status.summaryFinished", { jobId });
      await refreshSessionDetail(activeSessionId);
      await refreshSessions();
    } catch (error) {
      setStatus("status.summaryFailed", { error: String(error) });
    } finally {
      stopTimer();
      setIsSummarizing(false);
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

  async function handleDeleteSession(sessionId: string) {
    try {
      await deleteSession(sessionId);
      if (activeSessionId === sessionId) {
        setActiveSessionId(undefined);
        setActiveSession(undefined);
        setSessionJobs([]);
      }
      await refreshSessions();
      setStatus("status.sessionDeleteFinished");
    } catch (error) {
      setStatus("status.sessionDeleteFailed", { error: String(error) });
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
          onExportM4a={() => void onExport("m4a")}
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
          sessionJobs={sessionJobs}
          summaryTemplateId={summaryTemplateId}
          isTranscribing={isTranscribing}
          isSummarizing={isSummarizing}
          onSummaryTemplateChange={setSummaryTemplateId}
          onRefresh={() =>
            void refreshSessions().catch((error) => {
              setStatus("status.sessionsLoadFailed", { error: String(error) });
            })
          }
          onSelectSession={setActiveSessionId}
          onRenameSession={(sessionId, name) => void onRenameSession(sessionId, name)}
          onDeleteSession={(sessionId) => void handleDeleteSession(sessionId)}
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
