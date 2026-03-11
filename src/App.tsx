import { useEffect, useMemo, useRef, useState } from "react";

import RecorderTab from "./components/RecorderTab";
import SettingsTab from "./components/SettingsTab";
import SessionsTab from "./components/SessionsTab";
import TabNav, { type AppTab } from "./components/TabNav";
import {
  createSessionFromAudio,
  enqueueSummary,
  enqueueTranscription,
  exportRecording,
  listInputDevices,
  getRecorderStatus,
  getSession,
  getSettings,
  getSessionJobs,
  getJob,
  listSessions,
  deleteSession,
  pauseRecording,
  prepareTranscriptionAudio,
  renameSession,
  resumeRecording,
  setRealtimeSourceLanguage as setRealtimeSourceLanguageApi,
  setSessionTags as saveSessionTags,
  startRecording,
  setRealtimeTranslationTargetLanguage as setRealtimeTranslationTargetLanguageApi,
  stopRecording,
  toggleRealtimeTranscription,
  toggleRealtimeTranslation,
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
  RecorderInputDevice,
  RecorderPhase,
  RecordingQualityPreset,
  SessionDetail,
  SessionSummary,
  TranscriptSegment,
  Settings
} from "./types/domain";

const DEFAULT_BAILIAN_PROVIDER_ID = "bailian-default";
const DEFAULT_ALIYUN_PROVIDER_ID = "aliyun-tingwu-default";
const DEFAULT_OPENROUTER_PROVIDER_ID = "openrouter-default";
const DEFAULT_LOCAL_STT_PROVIDER_ID = "local-stt-default";
const DEFAULT_OSS_CONFIG_ID = "oss-aliyun-default";
const DEFAULT_RECORDING_SEGMENT_SECONDS = 120;
const MIN_RECORDING_SEGMENT_SECONDS = 10;
const MAX_RECORDING_SEGMENT_SECONDS = 1800;
const DEFAULT_SESSION_TAG_CATALOG = [
  "#or",
  "#会议",
  "#电话",
  "#导入",
  "#总经理会",
  "#军工规划会",
  "#625项目"
];

function normalizeSessionTag(rawTag: string): string | undefined {
  const trimmed = rawTag.trim();
  if (!trimmed) {
    return undefined;
  }
  const body = trimmed.replace(/^#+/, "").trim();
  if (!body) {
    return undefined;
  }
  return `#${body.toLowerCase()}`;
}

function normalizeSessionTagCatalog(input: string[]): string[] {
  const result: string[] = [];
  const seen = new Set<string>();
  for (const rawTag of input) {
    const normalized = normalizeSessionTag(rawTag);
    if (!normalized || seen.has(normalized)) {
      continue;
    }
    seen.add(normalized);
    result.push(normalized);
  }
  return result;
}

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

function createDefaultProvider(kind: ProviderConfig["kind"]): ProviderConfig {
  if (kind === "bailian") {
    return {
      id: DEFAULT_BAILIAN_PROVIDER_ID,
      name: "Bailian",
      kind: "bailian",
      capabilities: ["transcription", "summary"],
      enabled: true,
      bailian: {
        apiKey: "",
        baseUrl: "https://dashscope.aliyuncs.com",
        transcriptionModel: "paraformer-v2",
        summaryModel: "qwen-plus"
      }
    };
  }

  if (kind === "aliyun_tingwu") {
    return {
      id: DEFAULT_ALIYUN_PROVIDER_ID,
      name: "Aliyun Tingwu",
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
        realtimeEnabledByDefault: false,
        realtimeFormat: "pcm",
        realtimeSampleRate: 16000,
        realtimeSourceLanguage: "cn",
        realtimeLanguageHints: "",
        realtimeTaskKey: "",
        realtimeProgressiveCallbacksEnabled: false,
        realtimeTranscodingTargetAudioFormat: undefined,
        realtimeTranscriptionOutputLevel: 1,
        realtimeTranscriptionDiarizationEnabled: false,
        realtimeTranscriptionDiarizationSpeakerCount: undefined,
        realtimeTranscriptionPhraseId: "",
        realtimeTranslationEnabled: false,
        realtimeTranslationOutputLevel: 1,
        realtimeTranslationTargetLanguages: "en",
        realtimeAutoChaptersEnabled: false,
        realtimeMeetingAssistanceEnabled: false,
        realtimeSummarizationEnabled: false,
        realtimeSummarizationTypes: "",
        realtimeTextPolishEnabled: false,
        realtimeServiceInspectionEnabled: false,
        realtimeServiceInspection: undefined,
        realtimeCustomPromptEnabled: false,
        realtimeCustomPrompt: undefined,
        pollIntervalSeconds: 60,
        maxPollingMinutes: 180
      }
    };
  }

  if (kind === "openrouter") {
    return {
      id: DEFAULT_OPENROUTER_PROVIDER_ID,
      name: "OpenRouter",
      kind: "openrouter",
      capabilities: ["summary"],
      enabled: true,
      openrouter: {
        apiKey: "",
        baseUrl: "https://openrouter.ai/api/v1",
        summaryModel: "qwen/qwen-plus"
      }
    };
  }

  return {
    id: DEFAULT_LOCAL_STT_PROVIDER_ID,
    name: "Local STT",
    kind: "local_stt",
    capabilities: ["transcription"],
    enabled: true,
    localStt: {
      pythonPath: "",
      venvDir: "",
      modelCacheDir: "",
      engine: "whisper",
      whisperModel: "small",
      senseVoiceModel: "iic/SenseVoiceSmall",
      language: "auto",
      diarizationEnabled: true,
      minSpeakers: undefined,
      maxSpeakers: undefined,
      speakerCountHint: undefined,
      computeDevice: "auto",
      vadEnabled: true,
      chunkSeconds: 30
    }
  };
}

function createDefaultProviders(): ProviderConfig[] {
  return [
    createDefaultProvider("bailian"),
    createDefaultProvider("aliyun_tingwu"),
    createDefaultProvider("openrouter"),
    createDefaultProvider("local_stt")
  ];
}

const emptySettings: Settings = {
  providers: createDefaultProviders(),
  ossConfigs: createDefaultOssConfigs(),
  selectedOssConfigId: DEFAULT_OSS_CONFIG_ID,
  selectedTranscriptionProviderId: DEFAULT_BAILIAN_PROVIDER_ID,
  selectedSummaryProviderId: DEFAULT_BAILIAN_PROVIDER_ID,
  recordingSegmentSeconds: DEFAULT_RECORDING_SEGMENT_SECONDS,
  recordingInputDeviceId: "",
  sessionTagCatalog: DEFAULT_SESSION_TAG_CATALOG,
  defaultTemplateId: "meeting-default",
  templates: []
};
const initialSettings = normalizeSettings(emptySettings);

const WAVEFORM_CAPACITY = 220;
const DEFAULT_REALTIME_SOURCE_LANGUAGE = "cn";
const DEFAULT_REALTIME_TRANSLATION_TARGET_LANGUAGE = "en";

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

function normalizeAliasProviderId(providerId: string): string {
  const value = providerId.trim();
  if (!value) {
    return value;
  }
  if (value === "bailian-transcription-default" || value === "bailian-summary-default") {
    return DEFAULT_BAILIAN_PROVIDER_ID;
  }
  if (value === "aliyun-transcription-default") {
    return DEFAULT_ALIYUN_PROVIDER_ID;
  }
  if (value === "openrouter-summary-default") {
    return DEFAULT_OPENROUTER_PROVIDER_ID;
  }
  return value;
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
  const recordingSegmentSeconds = Number.isFinite(input.recordingSegmentSeconds)
    ? Math.min(
        MAX_RECORDING_SEGMENT_SECONDS,
        Math.max(MIN_RECORDING_SEGMENT_SECONDS, Math.floor(input.recordingSegmentSeconds))
      )
    : DEFAULT_RECORDING_SEGMENT_SECONDS;
  const recordingInputDeviceId =
    typeof input.recordingInputDeviceId === "string" ? input.recordingInputDeviceId.trim() : "";
  const sessionTagCatalog = normalizeSessionTagCatalog([
    ...DEFAULT_SESSION_TAG_CATALOG,
    ...(input.sessionTagCatalog ?? [])
  ]);

  const templates = input.templates.length > 0 ? input.templates : [createDefaultTemplate()];
  const defaultExists = templates.some((template) => template.id === input.defaultTemplateId);
  const providerSource = input.providers.length > 0 ? input.providers : createDefaultProviders();
  const grouped = new Map<ProviderConfig["kind"], ProviderConfig[]>();
  for (const provider of providerSource) {
    const list = grouped.get(provider.kind) ?? [];
    list.push(provider);
    grouped.set(provider.kind, list);
  }

  const orderedKinds: ProviderConfig["kind"][] = [
    "bailian",
    "aliyun_tingwu",
    "openrouter",
    "local_stt"
  ];

  const providers = orderedKinds.map((kind): ProviderConfig => {
    const defaults = createDefaultProvider(kind);
    const candidates = grouped.get(kind) ?? [];
    const mergedEnabled =
      candidates.length === 0 ? defaults.enabled : candidates.some((item) => item.enabled !== false);
    const mergedName =
      candidates
        .map((item) => item.name?.trim())
        .find((value) => Boolean(value)) ?? defaults.name;

    if (kind === "bailian") {
      const bailianConfig = candidates.map((item) => item.bailian).find(Boolean) ?? defaults.bailian!;
      return {
        ...defaults,
        name: mergedName,
        enabled: mergedEnabled,
        bailian: {
          ...defaults.bailian!,
          ...bailianConfig
        },
        aliyunTingwu: undefined,
        openrouter: undefined,
        localStt: undefined
      };
    }

    if (kind === "aliyun_tingwu") {
      const aliyunConfig =
        candidates.map((item) => item.aliyunTingwu).find(Boolean) ?? defaults.aliyunTingwu!;
      const legacyRealtimeOutputLevel = (
        aliyunConfig as { realtimeOutputLevel?: number } | undefined
      )?.realtimeOutputLevel;
      const realtimeSourceLanguage =
        aliyunConfig.realtimeSourceLanguage === "cn" ||
        aliyunConfig.realtimeSourceLanguage === "en" ||
        aliyunConfig.realtimeSourceLanguage === "yue" ||
        aliyunConfig.realtimeSourceLanguage === "ja" ||
        aliyunConfig.realtimeSourceLanguage === "ko" ||
        aliyunConfig.realtimeSourceLanguage === "multilingual"
          ? aliyunConfig.realtimeSourceLanguage
          : defaults.aliyunTingwu!.realtimeSourceLanguage;
      const realtimeFormat =
        aliyunConfig.realtimeFormat === "pcm" ||
        aliyunConfig.realtimeFormat === "opus" ||
        aliyunConfig.realtimeFormat === "aac" ||
        aliyunConfig.realtimeFormat === "speex" ||
        aliyunConfig.realtimeFormat === "mp3"
          ? aliyunConfig.realtimeFormat
          : defaults.aliyunTingwu!.realtimeFormat;
      const realtimeSampleRate =
        aliyunConfig.realtimeSampleRate === 8000 || aliyunConfig.realtimeSampleRate === 16000
          ? aliyunConfig.realtimeSampleRate
          : defaults.aliyunTingwu!.realtimeSampleRate;
      const realtimeTranscodingTargetAudioFormat =
        aliyunConfig.realtimeTranscodingTargetAudioFormat === "mp3" ? "mp3" : undefined;
      const realtimeLanguageHints =
        realtimeSourceLanguage === "multilingual"
          ? parseCsvList(aliyunConfig.realtimeLanguageHints).join(",")
          : parseCsvList(aliyunConfig.realtimeLanguageHints).join(",");
      const realtimeTranslationTargetLanguages = normalizeRealtimeTranslationTargetLanguagesCsv(
        aliyunConfig.realtimeTranslationTargetLanguages
      );
      const realtimeSummarizationTypes = normalizeSummarizationTypesCsv(
        aliyunConfig.realtimeSummarizationTypes
      );
      const transcriptionOutputLevel =
        aliyunConfig.realtimeTranscriptionOutputLevel === 2
          ? 2
          : legacyRealtimeOutputLevel === 2
            ? 2
            : defaults.aliyunTingwu!.realtimeTranscriptionOutputLevel;
      const translationOutputLevel =
        aliyunConfig.realtimeTranslationOutputLevel === 2
          ? 2
          : legacyRealtimeOutputLevel === 2
            ? 2
            : defaults.aliyunTingwu!.realtimeTranslationOutputLevel;
      const diarizationSpeakerCount = Number.isFinite(
        aliyunConfig.realtimeTranscriptionDiarizationSpeakerCount
      )
        ? Math.min(
            64,
            Math.max(
              0,
              Math.floor(aliyunConfig.realtimeTranscriptionDiarizationSpeakerCount as number)
            )
          )
        : undefined;
      return {
        ...defaults,
        name: mergedName,
        enabled: mergedEnabled,
        aliyunTingwu: {
          ...defaults.aliyunTingwu!,
          ...aliyunConfig,
          realtimeEnabledByDefault:
            typeof aliyunConfig.realtimeEnabledByDefault === "boolean"
              ? aliyunConfig.realtimeEnabledByDefault
              : defaults.aliyunTingwu!.realtimeEnabledByDefault,
          realtimeFormat,
          realtimeSampleRate,
          realtimeSourceLanguage,
          realtimeLanguageHints,
          realtimeTranscodingTargetAudioFormat,
          realtimeTranscriptionOutputLevel: transcriptionOutputLevel,
          realtimeTranslationOutputLevel: translationOutputLevel,
          realtimeTranscriptionDiarizationSpeakerCount: diarizationSpeakerCount,
          realtimeTranslationTargetLanguages,
          realtimeSummarizationTypes,
          pollIntervalSeconds: Number.isFinite(aliyunConfig.pollIntervalSeconds)
            ? Math.min(300, Math.max(60, Math.floor(aliyunConfig.pollIntervalSeconds)))
            : defaults.aliyunTingwu!.pollIntervalSeconds,
          maxPollingMinutes: Number.isFinite(aliyunConfig.maxPollingMinutes)
            ? Math.min(720, Math.max(5, Math.floor(aliyunConfig.maxPollingMinutes)))
            : defaults.aliyunTingwu!.maxPollingMinutes
        },
        bailian: undefined,
        openrouter: undefined,
        localStt: undefined
      };
    }

    if (kind === "openrouter") {
      const openrouterConfig =
        candidates.map((item) => item.openrouter).find(Boolean) ?? defaults.openrouter!;
      return {
        ...defaults,
        name: mergedName,
        enabled: mergedEnabled,
        openrouter: {
          ...defaults.openrouter!,
          ...openrouterConfig
        },
        bailian: undefined,
        aliyunTingwu: undefined,
        localStt: undefined
      };
    }

    const localSttConfig = candidates.map((item) => item.localStt).find(Boolean) ?? defaults.localStt!;
    const chunkSeconds = Number.isFinite(localSttConfig.chunkSeconds)
      ? Math.min(180, Math.max(5, Math.floor(localSttConfig.chunkSeconds)))
      : defaults.localStt!.chunkSeconds;
    return {
      ...defaults,
      name: mergedName,
      enabled: mergedEnabled,
      localStt: {
        ...defaults.localStt!,
        ...localSttConfig,
        chunkSeconds
      },
      bailian: undefined,
      aliyunTingwu: undefined,
      openrouter: undefined
    };
  });

  const aliasedTranscriptionSelection = normalizeAliasProviderId(input.selectedTranscriptionProviderId);
  const aliasedSummarySelection = normalizeAliasProviderId(input.selectedSummaryProviderId);
  const selectedTranscriptionProviderId =
    providers.find((provider) => provider.id === aliasedTranscriptionSelection && supportsCapability(provider, "transcription"))
      ?.id ??
    providers.find((provider) => supportsCapability(provider, "transcription"))?.id ??
    "";
  const selectedSummaryProviderId =
    providers.find((provider) => provider.id === aliasedSummarySelection && supportsCapability(provider, "summary"))?.id ??
    providers.find((provider) => supportsCapability(provider, "summary"))?.id ??
    "";

  return {
    providers,
    ossConfigs,
    selectedOssConfigId,
    selectedTranscriptionProviderId,
    selectedSummaryProviderId,
    recordingSegmentSeconds,
    recordingInputDeviceId,
    sessionTagCatalog,
    templates,
    defaultTemplateId: defaultExists ? input.defaultTemplateId : templates[0].id
  };
}

type AliyunRealtimeDefaults = {
  enabled: boolean;
  sourceLanguage: string;
  translationEnabled: boolean;
  translationTargetLanguage: string;
};

function parseCsvList(raw: string | undefined): string[] {
  return (raw ?? "")
    .replaceAll("，", ",")
    .split(",")
    .map((item) => item.trim().toLowerCase())
    .filter(Boolean);
}

function normalizeRealtimeSourceLanguage(raw: string | undefined): string {
  const value = (raw ?? "").trim().toLowerCase();
  if (value === "zh" || value === "zh-cn") {
    return "cn";
  }
  if (
    value === "cn" ||
    value === "en" ||
    value === "yue" ||
    value === "ja" ||
    value === "ko" ||
    value === "multilingual"
  ) {
    return value;
  }
  return DEFAULT_REALTIME_SOURCE_LANGUAGE;
}

function normalizeRealtimeTranslationTargetLanguage(raw: string | undefined): string {
  const value = (raw ?? "").trim().toLowerCase().replaceAll("_", "-");
  const canonical = value === "zh" || value === "zh-cn" ? "cn" : value;
  const allowed = new Set(["cn", "en", "ja", "ko", "de", "fr", "ru"]);
  return allowed.has(canonical) ? canonical : DEFAULT_REALTIME_TRANSLATION_TARGET_LANGUAGE;
}

function normalizeRealtimeTranslationTargetLanguagesCsv(raw: string | undefined): string {
  const result = parseCsvList(raw)
    .map((item) => normalizeRealtimeTranslationTargetLanguage(item))
    .filter((item, index, list) => list.indexOf(item) === index);
  return result.join(",");
}

function normalizeSummarizationTypesCsv(raw: string | undefined): string {
  const alias: Record<string, string> = {
    paragraph: "Paragraph",
    conversational: "Conversational",
    questionsanswering: "QuestionsAnswering",
    mindmap: "MindMap"
  };
  const result = (raw ?? "")
    .replaceAll("，", ",")
    .split(",")
    .map((item) => item.trim())
    .filter(Boolean)
    .map((item) => alias[item.toLowerCase().replace(/\s+/g, "")])
    .filter((item): item is string => Boolean(item))
    .filter((item, index, list) => list.indexOf(item) === index);
  return result.join(",");
}

function getAliyunRealtimeDefaults(settings: Settings): AliyunRealtimeDefaults {
  const provider = settings.providers.find((item) => item.kind === "aliyun_tingwu");
  const aliyun = provider?.aliyunTingwu;
  if (!aliyun) {
    return {
      enabled: false,
      sourceLanguage: DEFAULT_REALTIME_SOURCE_LANGUAGE,
      translationEnabled: false,
      translationTargetLanguage: DEFAULT_REALTIME_TRANSLATION_TARGET_LANGUAGE
    };
  }
  const targetLanguages = parseCsvList(aliyun.realtimeTranslationTargetLanguages);
  return {
    enabled: aliyun.realtimeEnabledByDefault,
    sourceLanguage: normalizeRealtimeSourceLanguage(aliyun.realtimeSourceLanguage),
    translationEnabled: aliyun.realtimeTranslationEnabled,
    translationTargetLanguage: normalizeRealtimeTranslationTargetLanguage(targetLanguages[0])
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
  const [recorderPhase, setRecorderPhase] = useState<RecorderPhase>("idle");
  const [recordingQuality, setRecordingQuality] = useState<RecordingQualityPreset>("standard");
  const [recordingElapsedMs, setRecordingElapsedMs] = useState<number>(0);
  const [waveformPoints, setWaveformPoints] = useState<number[]>([]);
  const initialRealtimeDefaults = getAliyunRealtimeDefaults(initialSettings);
  const [realtimeTranscriptionEnabled, setRealtimeTranscriptionEnabled] = useState<boolean>(
    initialRealtimeDefaults.enabled
  );
  const [realtimeSourceLanguage, setRealtimeSourceLanguage] = useState<string>(
    initialRealtimeDefaults.sourceLanguage
  );
  const [realtimeTranslationEnabled, setRealtimeTranslationEnabled] = useState<boolean>(
    initialRealtimeDefaults.translationEnabled
  );
  const [realtimeTranslationTargetLanguage, setRealtimeTranslationTargetLanguage] = useState<string>(
    initialRealtimeDefaults.translationTargetLanguage
  );
  const [realtimePreviewText, setRealtimePreviewText] = useState<string>("");
  const [realtimeSegments, setRealtimeSegments] = useState<TranscriptSegment[]>([]);
  const [realtimeTranscriptionState, setRealtimeTranscriptionState] = useState<
    "idle" | "connecting" | "running" | "paused" | "stopping" | "error"
  >("idle");
  const [realtimeLastError, setRealtimeLastError] = useState<string>();
  const [inputDevices, setInputDevices] = useState<RecorderInputDevice[]>([]);
  const [settings, setSettings] = useState<Settings>(initialSettings);
  const [runtimeRecordingInputDeviceId, setRuntimeRecordingInputDeviceId] = useState<string>(
    typeof initialSettings.recordingInputDeviceId === "string"
      ? initialSettings.recordingInputDeviceId
      : ""
  );
  const [summaryTemplateId, setSummaryTemplateId] = useState<string>(initialSettings.defaultTemplateId);
  const [statusState, setStatusState] = useState<StatusState>({ key: "status.ready" });
  const [isTranscribing, setIsTranscribing] = useState(false);
  const [isSummarizing, setIsSummarizing] = useState(false);
  const [isCreatingSession, setIsCreatingSession] = useState(false);
  const persistedSegmentCountRef = useRef(0);

  const t = useMemo(() => createTranslator(locale), [locale]);
  const statusMessage = t(statusState.key, statusState.params);
  const canRecord = useMemo(() => !currentRecording, [currentRecording]);
  const hasRecording = useMemo(
    () => Boolean(currentRecording && (recorderPhase === "recording" || recorderPhase === "paused")),
    [currentRecording, recorderPhase]
  );
  const canToggleRealtime = useMemo(
    () => !currentRecording || recorderPhase === "recording" || recorderPhase === "paused",
    [currentRecording, recorderPhase]
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

  async function refreshInputDevices() {
    const devices = await listInputDevices();
    setInputDevices(devices);
  }

  useEffect(() => {
    void refreshSessions().catch((error) => {
      setStatus("status.sessionsLoadFailed", { error: String(error) });
    });

    void getSettings()
      .then((loaded) => {
        const normalized = normalizeSettings(loaded);
        const realtimeDefaults = getAliyunRealtimeDefaults(normalized);
        setSettings(normalized);
        setRuntimeRecordingInputDeviceId(
          typeof normalized.recordingInputDeviceId === "string"
            ? normalized.recordingInputDeviceId
            : ""
        );
        setSummaryTemplateId(normalized.defaultTemplateId);
        setRealtimeTranscriptionEnabled(realtimeDefaults.enabled);
        setRealtimeSourceLanguage(realtimeDefaults.sourceLanguage);
        setRealtimeTranslationEnabled(realtimeDefaults.translationEnabled);
        setRealtimeTranslationTargetLanguage(realtimeDefaults.translationTargetLanguage);
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
    if (activeTab !== "settings" && activeTab !== "recorder") {
      return;
    }
    void refreshInputDevices().catch((error) => {
      setStatus("status.inputDeviceListFailed", { error: String(error) });
    });
  }, [activeTab]);

  useEffect(() => {
    if (!currentRecording) {
      persistedSegmentCountRef.current = 0;
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
        setRecorderPhase(status.phase);
        setRealtimeTranscriptionEnabled(status.realtime.enabled);
        setRealtimeSourceLanguage(status.realtime.sourceLanguage);
        setRealtimeTranslationEnabled(status.realtime.translationEnabled);
        setRealtimeTranslationTargetLanguage(status.realtime.translationTargetLanguage);
        setRealtimePreviewText(status.realtime.previewText);
        setRealtimeSegments(status.realtime.segments);
        setRealtimeTranscriptionState(status.realtime.state);
        setRealtimeLastError(status.realtime.lastError);
        if (status.realtime.lastError) {
          setStatus("status.realtimeRuntimeError", { error: status.realtime.lastError });
        }
        if (status.phase === "recording") {
          setWaveformPoints((previous) => {
            const next = [...previous, status.rms];
            if (next.length > WAVEFORM_CAPACITY) {
              next.splice(0, next.length - WAVEFORM_CAPACITY);
            }
            return next;
          });
        }
        if (status.persistedSegmentCount > persistedSegmentCountRef.current) {
          persistedSegmentCountRef.current = status.persistedSegmentCount;
          await refreshSessionDetail(currentRecording);
          await refreshSessions();
        }
        if (status.phase === "processing") {
          setStatus("status.recordingProcessing", { pending: String(status.pendingJobs) });
        }
        if (status.phase === "error" && status.lastProcessingError) {
          setStatus("status.recorderError", { error: status.lastProcessingError });
        }
        if (status.phase === "idle") {
          const realtimeDefaults = getAliyunRealtimeDefaults(settings);
          setCurrentRecording(undefined);
          setWaveformPoints([]);
          setRecorderPhase("idle");
          setRealtimePreviewText("");
          setRealtimeSegments([]);
          setRealtimeTranscriptionState("idle");
          setRealtimeLastError(undefined);
          setRealtimeSourceLanguage(realtimeDefaults.sourceLanguage);
          setRealtimeTranslationEnabled(realtimeDefaults.translationEnabled);
          setRealtimeTranslationTargetLanguage(realtimeDefaults.translationTargetLanguage);
          setRealtimeTranscriptionEnabled(realtimeDefaults.enabled);
          setStatus("status.recordingPostProcessDone");
          await refreshSessionDetail(currentRecording);
          await refreshSessions();
        }
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
  }, [currentRecording, settings]);

  async function onStart() {
    try {
      const preferredInputDeviceId =
        runtimeRecordingInputDeviceId.trim().length > 0
          ? runtimeRecordingInputDeviceId.trim()
          : undefined;
      const response = await startRecording(
        preferredInputDeviceId,
        recordingQuality,
        realtimeTranscriptionEnabled,
        realtimeSourceLanguage,
        realtimeTranslationEnabled,
        realtimeTranslationTargetLanguage
      );
      const sessionId = response.sessionId;
      persistedSegmentCountRef.current = 0;
      setCurrentRecording(sessionId);
      setRecorderPhase("recording");
      setActiveSessionId(sessionId);
      setRecordingElapsedMs(0);
      setWaveformPoints([]);
      setRealtimePreviewText("");
      setRealtimeSegments([]);
      setRealtimeTranscriptionState(realtimeTranscriptionEnabled ? "connecting" : "idle");
      setRealtimeTranslationEnabled(
        realtimeTranscriptionEnabled && realtimeTranslationEnabled
      );
      setRealtimeLastError(undefined);
      if (response.fallbackFromInputDeviceId) {
        setStatus("status.recordingFallbackInputDevice", {
          deviceName: response.inputDeviceName || response.inputDeviceId || "default input"
        });
      } else {
        setStatus("status.recordingSession", { sessionId });
      }
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
      setRecorderPhase("paused");
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
      setRecorderPhase("recording");
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
      setRecorderPhase("processing");
      setStatus("status.recordingProcessing", { pending: "..." });
      await refreshSessionDetail(currentRecording);
      await refreshSessions();
    } catch (error) {
      setStatus("status.stopRecordingFailed", { error: String(error) });
    }
  }

  async function onToggleRealtime(enabled: boolean) {
    const previous = realtimeTranscriptionEnabled;
    const previousTranslationEnabled = realtimeTranslationEnabled;
    setRealtimeTranscriptionEnabled(enabled);
    if (!enabled) {
      setRealtimeTranslationEnabled(false);
    }
    if (!currentRecording) {
      return;
    }
    try {
      await toggleRealtimeTranscription(currentRecording, enabled);
      if (enabled) {
        setRealtimeTranscriptionState("connecting");
        setRealtimeLastError(undefined);
      } else {
        setRealtimeTranscriptionState("idle");
      }
    } catch (error) {
      setRealtimeTranscriptionEnabled(previous);
      setRealtimeTranslationEnabled(previousTranslationEnabled);
      setStatus("status.realtimeToggleFailed", { error: String(error) });
    }
  }

  async function onToggleRealtimeTranslation(enabled: boolean) {
    if (!realtimeTranscriptionEnabled) {
      setRealtimeTranslationEnabled(false);
      return;
    }
    const previous = realtimeTranslationEnabled;
    setRealtimeTranslationEnabled(enabled);
    if (!currentRecording) {
      return;
    }
    try {
      await toggleRealtimeTranslation(currentRecording, enabled);
    } catch (error) {
      setRealtimeTranslationEnabled(previous);
      setStatus("status.realtimeToggleFailed", { error: String(error) });
    }
  }

  async function onChangeRealtimeTranslationTargetLanguage(targetLanguage: string) {
    const previous = realtimeTranslationTargetLanguage;
    setRealtimeTranslationTargetLanguage(targetLanguage);
    if (!currentRecording) {
      return;
    }
    try {
      await setRealtimeTranslationTargetLanguageApi(currentRecording, targetLanguage);
    } catch (error) {
      setRealtimeTranslationTargetLanguage(previous);
      setStatus("status.realtimeToggleFailed", { error: String(error) });
    }
  }

  async function onChangeRealtimeSourceLanguage(sourceLanguage: string) {
    const previous = realtimeSourceLanguage;
    setRealtimeSourceLanguage(sourceLanguage);
    if (!currentRecording) {
      return;
    }
    try {
      await setRealtimeSourceLanguageApi(currentRecording, sourceLanguage);
    } catch (error) {
      setRealtimeSourceLanguage(previous);
      setStatus("status.realtimeToggleFailed", { error: String(error) });
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

  async function onPrepareSessionPlaybackAudio(): Promise<string> {
    if (!activeSessionId) {
      throw new Error("no active session selected");
    }

    try {
      console.info("[playback-debug] request prepareTranscriptionAudio", {
        activeSessionId,
        audioSegmentCount: activeSession?.audioSegments.length ?? 0,
        exportedM4aPath: activeSession?.exportedM4aPath,
        exportedMp3Path: activeSession?.exportedMp3Path,
        exportedWavPath: activeSession?.exportedWavPath
      });
      const response = await prepareTranscriptionAudio(activeSessionId);
      console.info("[playback-debug] prepareTranscriptionAudio response", {
        activeSessionId,
        path: response.path,
        format: response.format,
        merged: response.merged
      });
      await refreshSessionDetail(activeSessionId);
      await refreshSessions();
      return response.path;
    } catch (error) {
      console.error("[playback-debug] prepareTranscriptionAudio failed", {
        activeSessionId,
        error: String(error)
      });
      setStatus("status.preparePlaybackAudioFailed", { error: String(error) });
      throw error;
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

  async function onSetSessionTags(sessionId: string, tags: string[]) {
    try {
      await saveSessionTags(sessionId, tags);
      await refreshSessions();
      if (activeSessionId === sessionId) {
        await refreshSessionDetail(sessionId);
      }
      const latestSettings = normalizeSettings(await getSettings());
      setSettings(latestSettings);
      setStatus("status.sessionTagsUpdated");
    } catch (error) {
      setStatus("status.sessionTagsUpdateFailed", { error: String(error) });
      throw error;
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
      const realtimeDefaults = getAliyunRealtimeDefaults(normalized);
      setSettings(normalized);
      setSummaryTemplateId((previous) =>
        normalized.templates.some((template) => template.id === previous)
          ? previous
          : normalized.defaultTemplateId
      );
      if (!currentRecording) {
        setRealtimeTranscriptionEnabled(realtimeDefaults.enabled);
        setRealtimeSourceLanguage(realtimeDefaults.sourceLanguage);
        setRealtimeTranslationEnabled(realtimeDefaults.translationEnabled);
        setRealtimeTranslationTargetLanguage(realtimeDefaults.translationTargetLanguage);
      }
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
          hasRecording={hasRecording}
          elapsedMs={recordingElapsedMs}
          waveformPoints={waveformPoints}
          inputDevices={inputDevices}
          recordingInputDeviceId={runtimeRecordingInputDeviceId}
          recordingInputDeviceDisabled={hasRecording}
          realtimeEnabled={realtimeTranscriptionEnabled}
          realtimeToggleDisabled={!canToggleRealtime}
          realtimeSourceLanguage={realtimeSourceLanguage}
          realtimeSourceLanguageDisabled={!canToggleRealtime || !realtimeTranscriptionEnabled}
          realtimeTranslationEnabled={realtimeTranslationEnabled}
          realtimeTranslationToggleDisabled={!canToggleRealtime || !realtimeTranscriptionEnabled}
          realtimeTranslationTargetLanguage={realtimeTranslationTargetLanguage}
          realtimeTranslationTargetDisabled={!canToggleRealtime || !realtimeTranscriptionEnabled}
          realtimePreviewText={realtimePreviewText}
          realtimeSegments={realtimeSegments}
          realtimeState={realtimeTranscriptionState}
          realtimeLastError={realtimeLastError}
          qualityPreset={recordingQuality}
          onRecordingInputDeviceChange={setRuntimeRecordingInputDeviceId}
          onQualityChange={setRecordingQuality}
          onRealtimeToggle={(enabled) => void onToggleRealtime(enabled)}
          onRealtimeSourceLanguageChange={(sourceLanguage) =>
            void onChangeRealtimeSourceLanguage(sourceLanguage)
          }
          onRealtimeTranslationToggle={(enabled) => void onToggleRealtimeTranslation(enabled)}
          onRealtimeTranslationTargetChange={(targetLanguage) =>
            void onChangeRealtimeTranslationTargetLanguage(targetLanguage)
          }
          onStart={() => void onStart()}
          onPause={() => void onPause()}
          onResume={() => void onResume()}
          onStop={() => void onStop()}
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
          tagCatalog={settings.sessionTagCatalog}
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
          onSetSessionTags={(sessionId, tags) => void onSetSessionTags(sessionId, tags)}
          onDeleteSession={(sessionId) => void handleDeleteSession(sessionId)}
          onPreparePlaybackAudio={() => onPrepareSessionPlaybackAudio()}
          onExportM4a={() => void onExport("m4a")}
          onExportMp3={() => void onExport("mp3")}
          onTranscribe={() => void onTranscribe()}
          onSummarize={() => void onSummarize()}
          t={t}
        />
      )}

      {activeTab === "settings" && (
        <SettingsTab
          locale={locale}
          settings={settings}
          inputDevices={inputDevices}
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
