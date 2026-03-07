import { useEffect, useMemo, useState } from "react";

import RecorderTab from "./components/RecorderTab";
import SettingsTab from "./components/SettingsTab";
import SessionsTab from "./components/SessionsTab";
import TabNav, { type AppTab } from "./components/TabNav";
import {
  createSessionFromAudio,
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
  OssConfig,
  OssProviderKind,
  ProviderCapability,
  ProviderConfig,
  PromptTemplate,
  RecordingQualityPreset,
  SessionDetail,
  SessionSummary,
  Settings
} from "./types/domain";

const DEFAULT_BAILIAN_TRANSCRIPTION_PROVIDER_ID = "bailian-transcription-default";
const DEFAULT_ALIYUN_TRANSCRIPTION_PROVIDER_ID = "aliyun-transcription-default";
const DEFAULT_BAILIAN_SUMMARY_PROVIDER_ID = "bailian-summary-default";
const DEFAULT_OPENROUTER_SUMMARY_PROVIDER_ID = "openrouter-summary-default";
const DEFAULT_OSS_CONFIG_ID = "oss-aliyun-default";

function createDefaultOssConfig(kind: OssProviderKind = "aliyun"): OssConfig {
  return {
    id: kind === "r2" ? "oss-r2-default" : DEFAULT_OSS_CONFIG_ID,
    name: kind === "r2" ? "Cloudflare R2" : "Aliyun OSS",
    kind,
    accessKeyId: "",
    accessKeySecret: "",
    endpoint: "",
    bucket: "",
    pathPrefix: "open-recorder",
    signedUrlTtlSeconds: 1800
  };
}

function createDefaultOssConfigs(): OssConfig[] {
  return [createDefaultOssConfig("aliyun"), createDefaultOssConfig("r2")];
}

function createDefaultProviders(): ProviderConfig[] {
  return [
    {
      id: DEFAULT_BAILIAN_TRANSCRIPTION_PROVIDER_ID,
      name: "Bailian Transcription",
      kind: "bailian",
      capabilities: ["transcription"],
      enabled: true,
      bailian: {
        apiKey: "",
        baseUrl: "https://dashscope.aliyuncs.com",
        transcriptionModel: "paraformer-v2",
        summaryModel: "qwen-plus"
      }
    },
    {
      id: DEFAULT_ALIYUN_TRANSCRIPTION_PROVIDER_ID,
      name: "Aliyun Tingwu Transcription",
      kind: "aliyun_tingwu",
      capabilities: ["transcription"],
      enabled: true,
      aliyunTingwu: {
        accessKeyId: "",
        accessKeySecret: "",
        appKey: "",
        endpoint: "https://tingwu.cn-beijing.aliyuncs.com",
        sourceLanguage: "cn",
        fileUrlPrefix: "",
        languageHints: "",
        transcriptionNormalizationEnabled: true,
        transcriptionParagraphEnabled: true,
        transcriptionPunctuationPredictionEnabled: true,
        transcriptionDisfluencyRemovalEnabled: false,
        transcriptionSpeakerDiarizationEnabled: true,
        pollIntervalSeconds: 60,
        maxPollingMinutes: 180
      }
    },
    {
      id: DEFAULT_BAILIAN_SUMMARY_PROVIDER_ID,
      name: "Bailian Summary",
      kind: "bailian",
      capabilities: ["summary"],
      enabled: true,
      bailian: {
        apiKey: "",
        baseUrl: "https://dashscope.aliyuncs.com",
        transcriptionModel: "paraformer-v2",
        summaryModel: "qwen-plus"
      }
    },
    {
      id: DEFAULT_OPENROUTER_SUMMARY_PROVIDER_ID,
      name: "OpenRouter Summary",
      kind: "openrouter",
      capabilities: ["summary"],
      enabled: true,
      openrouter: {
        apiKey: "",
        baseUrl: "https://openrouter.ai/api/v1",
        summaryModel: "qwen/qwen-plus"
      }
    }
  ];
}

const emptySettings: Settings = {
  providers: createDefaultProviders(),
  ossConfigs: createDefaultOssConfigs(),
  selectedOssConfigId: DEFAULT_OSS_CONFIG_ID,
  selectedTranscriptionProviderId: DEFAULT_BAILIAN_TRANSCRIPTION_PROVIDER_ID,
  selectedSummaryProviderId: DEFAULT_BAILIAN_SUMMARY_PROVIDER_ID,
  defaultTemplateId: "meeting-default",
  templates: []
};
const initialSettings = normalizeSettings(emptySettings);

const WAVEFORM_CAPACITY = 220;

type StatusState = {
  key: TranslationKey;
  params?: TranslationParams;
};

type PollJobOptions = {
  runningStatusKey: "status.transcriptionRunning" | "status.summaryRunning";
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

function supportsCapability(
  provider: ProviderConfig,
  capability: ProviderCapability
): boolean {
  return provider.enabled && provider.capabilities.includes(capability);
}

function normalizeSettings(input: Settings): Settings {
  const ossConfigs = (input.ossConfigs.length > 0 ? input.ossConfigs : createDefaultOssConfigs()).map(
    (config, index): OssConfig => {
      const fallback = createDefaultOssConfig(config.kind ?? "aliyun");
      return {
        ...fallback,
        ...config,
        id: config.id?.trim() || `oss-${index + 1}`,
        name:
          config.name?.trim() ||
          (config.kind === "r2" ? "Cloudflare R2" : "Aliyun OSS"),
        kind: config.kind ?? "aliyun",
        signedUrlTtlSeconds: Number.isFinite(config.signedUrlTtlSeconds)
          ? Math.min(86400, Math.max(60, Math.floor(config.signedUrlTtlSeconds)))
          : fallback.signedUrlTtlSeconds
      };
    }
  );
  if (!ossConfigs.some((config) => config.kind === "aliyun")) {
    ossConfigs.push(createDefaultOssConfig("aliyun"));
  }
  if (!ossConfigs.some((config) => config.kind === "r2")) {
    ossConfigs.push(createDefaultOssConfig("r2"));
  }

  const selectedOssConfigId =
    ossConfigs.find((config) => config.id === input.selectedOssConfigId)?.id ??
    ossConfigs[0]?.id ??
    "";

  const templates = input.templates.length > 0 ? input.templates : [createDefaultTemplate()];
  const defaultExists = templates.some((template) => template.id === input.defaultTemplateId);
  const providers = (input.providers.length > 0 ? input.providers : createDefaultProviders()).map(
    (provider, index): ProviderConfig => {
      const base: ProviderConfig = {
        ...provider,
        id: provider.id?.trim() || `provider-${index + 1}`,
        name: provider.name?.trim() || provider.kind,
        capabilities:
          provider.capabilities.length > 0
            ? provider.capabilities
            : provider.kind === "aliyun_tingwu"
              ? ["transcription"]
              : provider.kind === "openrouter"
                ? ["summary"]
                : ["transcription", "summary"],
        enabled: provider.enabled ?? true
      };

      if (base.kind === "bailian") {
        const bailian = base.bailian ?? createDefaultProviders()[0].bailian!;
        return {
          ...base,
          bailian,
          aliyunTingwu: undefined,
          openrouter: undefined
        };
      }

      if (base.kind === "aliyun_tingwu") {
        const aliyun = base.aliyunTingwu ?? createDefaultProviders()[1].aliyunTingwu!;
        return {
          ...base,
          aliyunTingwu: {
            ...aliyun,
            pollIntervalSeconds: Number.isFinite(aliyun.pollIntervalSeconds)
              ? Math.min(300, Math.max(60, Math.floor(aliyun.pollIntervalSeconds)))
              : 60,
            maxPollingMinutes: Number.isFinite(aliyun.maxPollingMinutes)
              ? Math.min(720, Math.max(5, Math.floor(aliyun.maxPollingMinutes)))
              : 180
          },
          bailian: undefined,
          openrouter: undefined
        };
      }

      const openrouter = base.openrouter ?? createDefaultProviders()[3].openrouter!;
      return {
        ...base,
        openrouter,
        bailian: undefined,
        aliyunTingwu: undefined
      };
    }
  );

  const selectedTranscriptionProviderId =
    providers.find((provider) => provider.id === input.selectedTranscriptionProviderId && supportsCapability(provider, "transcription"))
      ?.id ??
    providers.find((provider) => supportsCapability(provider, "transcription"))?.id ??
    "";
  const selectedSummaryProviderId =
    providers.find((provider) => provider.id === input.selectedSummaryProviderId && supportsCapability(provider, "summary"))?.id ??
    providers.find((provider) => supportsCapability(provider, "summary"))?.id ??
    "";

  return {
    providers,
    ossConfigs,
    selectedOssConfigId,
    selectedTranscriptionProviderId,
    selectedSummaryProviderId,
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
  const [isCreatingSession, setIsCreatingSession] = useState(false);

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

  function upsertJob(previous: JobInfo[], nextJob: JobInfo): JobInfo[] {
    const index = previous.findIndex((job) => job.id === nextJob.id);
    if (index < 0) {
      return [...previous, nextJob];
    }
    const current = previous[index];
    if (
      current.status === nextJob.status &&
      current.updatedAt === nextJob.updatedAt &&
      current.progressMsg === nextJob.progressMsg &&
      current.error === nextJob.error
    ) {
      return previous;
    }
    const next = [...previous];
    next[index] = nextJob;
    return next;
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
    setStatus("status.transcriptionRunning", { elapsed: "" });

    try {
      // 后端立即返回 jobId，实际转写在后台线程执行
      const jobId = await enqueueTranscription(activeSessionId);

      // 立即刷新详情，让任务出现在列表中
      if (activeSessionId) {
        await refreshSessionDetail(activeSessionId);
      }

      // 轮询 job 状态直到完成或失败
      const pollResult = await pollJobUntilDone(jobId, {
        runningStatusKey: "status.transcriptionRunning"
      });

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
      setIsTranscribing(false);
    }
  }

  /**
   * 轮询 job 状态直到完成或失败
   * 每 3 秒查询一次，最长等待 180 分钟
   */
  async function pollJobUntilDone(
    jobId: string,
    options: PollJobOptions
  ): Promise<{ status: string; error?: string }> {
    const maxAttempts = 3600; // 3 * 3600 = 10800 秒 = 3 小时
    for (let attempt = 0; attempt < maxAttempts; attempt++) {
      await sleep(3000);
      try {
        const job = await getJob(jobId);
        // 更新任务列表状态（包含进度消息）
        setSessionJobs((previous) => upsertJob(previous, job));

        if (job.status === "completed") {
          return { status: "completed" };
        }
        if (job.status === "failed") {
          return { status: "failed", error: job.error ?? undefined };
        }
        if (job.status === "running") {
          setStatus(options.runningStatusKey, {
            elapsed: job.progressMsg ? ` ${job.progressMsg}` : ""
          });
        }
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
    setStatus("status.summaryRunning", { elapsed: "" });

    try {
      const jobId = await enqueueSummary(
        activeSessionId,
        summaryTemplateId || settings.defaultTemplateId
      );

      // 立即刷新详情，让任务出现在列表中
      if (activeSessionId) {
        await refreshSessionDetail(activeSessionId);
      }

      const pollResult = await pollJobUntilDone(jobId, {
        runningStatusKey: "status.summaryRunning"
      });

      if (pollResult.status === "completed") {
        setStatus("status.summaryFinished", { jobId });
      } else {
        setStatus("status.summaryFailed", {
          error: pollResult.error || "unknown error"
        });
      }

      await refreshSessionDetail(activeSessionId);
      await refreshSessions();
    } catch (error) {
      setStatus("status.summaryFailed", { error: String(error) });
    } finally {
      setIsSummarizing(false);
    }
  }

  async function readAudioDurationMs(file: File): Promise<number | undefined> {
    const objectUrl = URL.createObjectURL(file);
    try {
      const durationSeconds = await new Promise<number>((resolve, reject) => {
        const audio = new Audio();
        audio.preload = "metadata";
        audio.onloadedmetadata = () => {
          const duration = audio.duration;
          audio.src = "";
          if (Number.isFinite(duration) && duration > 0) {
            resolve(duration);
            return;
          }
          reject(new Error("invalid audio duration"));
        };
        audio.onerror = () => {
          audio.src = "";
          reject(new Error("failed to load audio metadata"));
        };
        audio.src = objectUrl;
      });
      return Math.round(durationSeconds * 1000);
    } catch {
      return undefined;
    } finally {
      URL.revokeObjectURL(objectUrl);
    }
  }

  async function onCreateSessionFromFile(file: File) {
    if (isCreatingSession) {
      return;
    }

    setIsCreatingSession(true);
    try {
      const audioBytes = Array.from(new Uint8Array(await file.arrayBuffer()));
      const durationMs = await readAudioDurationMs(file);
      const sessionId = await createSessionFromAudio(
        file.name,
        audioBytes,
        file.type || undefined,
        durationMs
      );
      await refreshSessions();
      setActiveSessionId(sessionId);
      await refreshSessionDetail(sessionId);
      setStatus("status.sessionCreateFinished", { fileName: file.name });
    } catch (error) {
      setStatus("status.sessionCreateFailed", { error: String(error) });
    } finally {
      setIsCreatingSession(false);
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
          isCreatingSession={isCreatingSession}
          onSummaryTemplateChange={setSummaryTemplateId}
          onCreateSessionFromFile={(file) => void onCreateSessionFromFile(file)}
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
