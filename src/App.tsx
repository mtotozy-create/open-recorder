import { useEffect, useMemo, useRef, useState } from "react";

import DiscoverTab from "./components/DiscoverTab";
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
  setSessionDiscoverable,
  setSessionTags as saveSessionTags,
  updateSessionSummaryRawMarkdown as updateSessionSummaryRawMarkdownApi,
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
const DEFAULT_OLLAMA_PROVIDER_ID = "ollama-default";
const DEFAULT_LOCAL_STT_PROVIDER_ID = "local-stt-default";
const DEFAULT_OSS_CONFIG_ID = "oss-aliyun-default";
const DEFAULT_RECORDING_SEGMENT_SECONDS = 120;
const MIN_RECORDING_SEGMENT_SECONDS = 10;
const MAX_RECORDING_SEGMENT_SECONDS = 1800;
const DEFAULT_SESSION_TAG_CATALOG = [
  "#or",
  "#meeting",
  "#call",
  "#imported"
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

function sanitizeFileNameSegment(input: string): string {
  return input
    .replace(/[<>:"/\\|?*\u0000-\u001F]/g, " ")
    .replace(/\s+/g, " ")
    .trim()
    .slice(0, 80);
}

function estimateTextDisplayUnits(input: string): number {
  const normalized = input.replace(/\s+/g, " ").trim();
  if (!normalized) {
    return 0;
  }
  let units = 0;
  for (const char of normalized) {
    if (char === " ") {
      units += 0.5;
      continue;
    }
    const code = char.charCodeAt(0);
    const isWideGlyph =
      (code >= 0x2e80 && code <= 0x9fff) ||
      (code >= 0xac00 && code <= 0xd7af) ||
      (code >= 0x3040 && code <= 0x30ff) ||
      (code >= 0xff01 && code <= 0xff60);
    units += isWideGlyph ? 2 : 1;
  }
  return units;
}

function normalizeColumnPercentages(
  rawPercentages: number[],
  minPercent: number,
  maxPercent: number
): number[] {
  const adjusted = rawPercentages.map((value) =>
    Math.min(maxPercent, Math.max(minPercent, value))
  );
  const tolerance = 0.01;
  for (let round = 0; round < 24; round += 1) {
    const sum = adjusted.reduce((acc, value) => acc + value, 0);
    const diff = 100 - sum;
    if (Math.abs(diff) <= tolerance) {
      break;
    }
    if (diff > 0) {
      const candidates = adjusted
        .map((value, index) => ({ index, capacity: maxPercent - value }))
        .filter((item) => item.capacity > tolerance);
      const capacitySum = candidates.reduce((acc, item) => acc + item.capacity, 0);
      if (capacitySum <= tolerance) {
        break;
      }
      for (const item of candidates) {
        adjusted[item.index] = Math.min(
          maxPercent,
          adjusted[item.index] + (diff * item.capacity) / capacitySum
        );
      }
    } else {
      const need = -diff;
      const candidates = adjusted
        .map((value, index) => ({ index, capacity: value - minPercent }))
        .filter((item) => item.capacity > tolerance);
      const capacitySum = candidates.reduce((acc, item) => acc + item.capacity, 0);
      if (capacitySum <= tolerance) {
        break;
      }
      for (const item of candidates) {
        adjusted[item.index] = Math.max(
          minPercent,
          adjusted[item.index] - (need * item.capacity) / capacitySum
        );
      }
    }
  }
  const finalSum = adjusted.reduce((acc, value) => acc + value, 0);
  if (adjusted.length > 0 && Math.abs(100 - finalSum) > tolerance) {
    adjusted[0] += 100 - finalSum;
  }
  return adjusted;
}

function applyAdaptiveTableColumnWidths(table: HTMLTableElement): void {
  const rows = Array.from(table.querySelectorAll("tr"));
  const columnCount = rows.reduce((max, row) => {
    const cells = Array.from(row.children).filter((child) => {
      const tag = child.tagName.toUpperCase();
      return tag === "TH" || tag === "TD";
    });
    return Math.max(max, cells.length);
  }, 0);
  if (columnCount <= 0) {
    return;
  }

  const columnScores = new Array<number>(columnCount).fill(4);
  rows.forEach((row, rowIndex) => {
    const cells = Array.from(row.children).filter((child) => {
      const tag = child.tagName.toUpperCase();
      return tag === "TH" || tag === "TD";
    }) as HTMLElement[];
    cells.forEach((cell, index) => {
      const text = cell.innerText || cell.textContent || "";
      const units = Math.max(1, Math.min(160, estimateTextDisplayUnits(text)));
      const weighted = rowIndex === 0 ? units * 1.15 : units;
      columnScores[index] = Math.max(columnScores[index], weighted);
    });
  });

  const effectiveScores = columnScores.map((score) => Math.sqrt(score) + 2);
  const totalEffective = effectiveScores.reduce((acc, value) => acc + value, 0);
  const rawPercentages = effectiveScores.map((value) => (value / totalEffective) * 100);

  const suggestedMin =
    columnCount >= 6 ? 7 : columnCount === 5 ? 9 : columnCount === 4 ? 11 : columnCount === 3 ? 15 : 22;
  const minPercent = Math.max(4, Math.min(suggestedMin, Math.floor(100 / columnCount) - 1));
  const maxPercent = columnCount <= 2 ? 78 : columnCount === 3 ? 60 : columnCount === 4 ? 48 : 42;
  const normalizedPercentages = normalizeColumnPercentages(rawPercentages, minPercent, maxPercent);

  table.querySelectorAll("colgroup[data-or-pdf-colgroup='1']").forEach((node) => node.remove());
  const colgroup = document.createElement("colgroup");
  colgroup.setAttribute("data-or-pdf-colgroup", "1");
  for (const percentage of normalizedPercentages) {
    const col = document.createElement("col");
    col.style.width = `${percentage.toFixed(2)}%`;
    colgroup.appendChild(col);
  }
  table.insertBefore(colgroup, table.firstChild);
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
        summaryModel: "qwen/qwen-plus",
        discoverModel: "qwen/qwen-plus"
      }
    };
  }

  if (kind === "ollama") {
    return {
      id: DEFAULT_OLLAMA_PROVIDER_ID,
      name: "Ollama",
      kind: "ollama",
      capabilities: ["summary"],
      enabled: true,
      ollama: {
        apiKey: "",
        baseUrl: "http://127.0.0.1:11434/v1",
        summaryModel: "qwen2.5:7b-instruct"
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
    createDefaultProvider("ollama"),
    createDefaultProvider("local_stt")
  ];
}

const emptySettings: Settings = {
  providers: createDefaultProviders(),
  ossConfigs: createDefaultOssConfigs(),
  selectedOssConfigId: DEFAULT_OSS_CONFIG_ID,
  selectedTranscriptionProviderId: DEFAULT_BAILIAN_PROVIDER_ID,
  selectedSummaryProviderId: DEFAULT_OLLAMA_PROVIDER_ID,
  selectedDiscoverProviderId: DEFAULT_OLLAMA_PROVIDER_ID,
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
  if (value === "ollama-summary-default") {
    return DEFAULT_OLLAMA_PROVIDER_ID;
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
    "ollama",
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
        ollama: undefined,
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
        ollama: undefined,
        localStt: undefined
      };
    }

    if (kind === "openrouter") {
      const openrouterConfig =
        candidates.map((item) => item.openrouter).find(Boolean) ?? defaults.openrouter!;
      const discoverModel =
        (openrouterConfig.discoverModel ?? "").trim().length > 0
          ? openrouterConfig.discoverModel
          : openrouterConfig.summaryModel;
      return {
        ...defaults,
        name: mergedName,
        enabled: mergedEnabled,
        openrouter: {
          ...defaults.openrouter!,
          ...openrouterConfig,
          discoverModel
        },
        bailian: undefined,
        aliyunTingwu: undefined,
        ollama: undefined,
        localStt: undefined
      };
    }

    if (kind === "ollama") {
      const ollamaConfig = candidates.map((item) => item.ollama).find(Boolean) ?? defaults.ollama!;
      return {
        ...defaults,
        name: mergedName,
        enabled: mergedEnabled,
        ollama: {
          ...defaults.ollama!,
          ...ollamaConfig
        },
        bailian: undefined,
        aliyunTingwu: undefined,
        openrouter: undefined,
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
      openrouter: undefined,
      ollama: undefined
    };
  });

  const aliasedTranscriptionSelection = normalizeAliasProviderId(input.selectedTranscriptionProviderId);
  const aliasedSummarySelection = normalizeAliasProviderId(input.selectedSummaryProviderId);
  const aliasedDiscoverSelection = normalizeAliasProviderId(
    typeof input.selectedDiscoverProviderId === "string"
      ? input.selectedDiscoverProviderId
      : input.selectedSummaryProviderId
  );
  const selectedTranscriptionProviderId =
    providers.find((provider) => provider.id === aliasedTranscriptionSelection && supportsCapability(provider, "transcription"))
      ?.id ??
    providers.find((provider) => supportsCapability(provider, "transcription"))?.id ??
    "";
  const selectedSummaryProviderId =
    providers.find((provider) => provider.id === aliasedSummarySelection && supportsCapability(provider, "summary"))?.id ??
    providers.find(
      (provider) =>
        provider.id === DEFAULT_OLLAMA_PROVIDER_ID && supportsCapability(provider, "summary")
    )?.id ??
    providers.find((provider) => supportsCapability(provider, "summary"))?.id ??
    "";
  const selectedDiscoverProviderId =
    providers.find((provider) => provider.id === aliasedDiscoverSelection && supportsCapability(provider, "summary"))?.id ??
    providers.find(
      (provider) =>
        provider.id === DEFAULT_OLLAMA_PROVIDER_ID && supportsCapability(provider, "summary")
    )?.id ??
    providers.find((provider) => supportsCapability(provider, "summary"))?.id ??
    "";

  return {
    providers,
    ossConfigs,
    selectedOssConfigId,
    selectedTranscriptionProviderId,
    selectedSummaryProviderId,
    selectedDiscoverProviderId,
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

  async function onExportSummaryPdf() {
    if (typeof window === "undefined" || typeof document === "undefined") {
      return;
    }
    const summaryRoot = document.querySelector<HTMLElement>(".summary-print-root");
    if (!summaryRoot) {
      setStatus("status.exportPdfFailed", { error: "summary view not found" });
      return;
    }

    let captureNode: HTMLElement | undefined;
    let exportRootNode: HTMLDivElement | undefined;
    try {
      const [{ default: html2canvas }, { jsPDF }] = await Promise.all([
        import("html2canvas"),
        import("jspdf")
      ]);

      const pdf = new jsPDF({
        orientation: "portrait",
        unit: "pt",
        format: "a4"
      });
      const pageWidth = pdf.internal.pageSize.getWidth();
      const pageHeight = pdf.internal.pageSize.getHeight();
      const margin = 28;
      const contentWidth = pageWidth - margin * 2;
      const contentHeight = pageHeight - margin * 2;
      const captureWidthPx = Math.max(640, Math.round((contentWidth * 96) / 72));

      exportRootNode = document.createElement("div");
      exportRootNode.style.position = "fixed";
      exportRootNode.style.left = "-100000px";
      exportRootNode.style.top = "0";
      exportRootNode.style.width = `${captureWidthPx}px`;
      exportRootNode.style.maxWidth = `${captureWidthPx}px`;
      exportRootNode.style.height = "auto";
      exportRootNode.style.maxHeight = "none";
      exportRootNode.style.overflow = "visible";
      exportRootNode.style.background = "#ffffff";
      exportRootNode.style.boxSizing = "border-box";
      exportRootNode.style.padding = "0";
      exportRootNode.style.margin = "0";

      captureNode = summaryRoot.cloneNode(true) as HTMLElement;
      captureNode.style.position = "static";
      captureNode.style.width = "100%";
      captureNode.style.maxWidth = "100%";
      captureNode.style.maxHeight = "none";
      captureNode.style.height = "auto";
      captureNode.style.overflow = "visible";
      captureNode.style.background = "#ffffff";
      captureNode.style.padding = "0";
      captureNode.style.margin = "0";
      captureNode.style.color = "#111827";
      captureNode.style.fontSize = "14px";
      captureNode.style.lineHeight = "1.6";

      const markdownTables = captureNode.querySelectorAll<HTMLTableElement>("table");
      for (const table of markdownTables) {
        table.style.display = "table";
        table.style.width = "100%";
        table.style.maxWidth = "100%";
        table.style.tableLayout = "fixed";
        table.style.overflow = "visible";
        table.style.overflowX = "visible";
        table.style.overflowY = "visible";
        table.style.whiteSpace = "normal";
        table.style.wordBreak = "break-word";
        applyAdaptiveTableColumnWidths(table);
      }
      const tableParents = captureNode.querySelectorAll<HTMLElement>("table, thead, tbody, tr");
      for (const item of tableParents) {
        item.style.maxWidth = "100%";
        item.style.overflow = "visible";
      }
      const tableCells = captureNode.querySelectorAll<HTMLElement>("th, td");
      for (const cell of tableCells) {
        cell.style.whiteSpace = "normal";
        cell.style.wordBreak = "break-word";
        cell.style.overflowWrap = "anywhere";
        cell.style.maxWidth = "none";
      }
      const textBlocks = captureNode.querySelectorAll<HTMLElement>("p, li, blockquote");
      for (const block of textBlocks) {
        block.style.whiteSpace = "pre-wrap";
        block.style.wordBreak = "break-word";
        block.style.overflowWrap = "anywhere";
      }
      const preBlocks = captureNode.querySelectorAll<HTMLElement>("pre");
      for (const pre of preBlocks) {
        pre.style.whiteSpace = "pre-wrap";
        pre.style.wordBreak = "break-word";
        pre.style.overflowWrap = "anywhere";
      }
      exportRootNode.appendChild(captureNode);
      document.body.appendChild(exportRootNode);

      await new Promise<void>((resolve) => {
        window.requestAnimationFrame(() => resolve());
      });

      const canvas = await html2canvas(exportRootNode, {
        backgroundColor: "#ffffff",
        scale: Math.min(window.devicePixelRatio || 2, 2),
        useCORS: true,
        logging: false,
        width: exportRootNode.scrollWidth,
        windowWidth: exportRootNode.scrollWidth
      });

      const pageHeightPx = Math.max(1, Math.floor((contentHeight * canvas.width) / contentWidth));
      let renderedHeightPx = 0;
      let pageIndex = 0;
      while (renderedHeightPx < canvas.height) {
        const sliceHeightPx = Math.min(pageHeightPx, canvas.height - renderedHeightPx);
        const pageCanvas = document.createElement("canvas");
        pageCanvas.width = canvas.width;
        pageCanvas.height = sliceHeightPx;
        const pageCtx = pageCanvas.getContext("2d");
        if (!pageCtx) {
          throw new Error("failed to render PDF page");
        }
        pageCtx.fillStyle = "#ffffff";
        pageCtx.fillRect(0, 0, pageCanvas.width, pageCanvas.height);
        pageCtx.drawImage(
          canvas,
          0,
          renderedHeightPx,
          canvas.width,
          sliceHeightPx,
          0,
          0,
          pageCanvas.width,
          pageCanvas.height
        );

        const pageImageData = pageCanvas.toDataURL("image/png");
        const renderedPageHeight = (sliceHeightPx * contentWidth) / canvas.width;
        if (pageIndex > 0) {
          pdf.addPage();
        }
        pdf.addImage(
          pageImageData,
          "PNG",
          margin,
          margin,
          contentWidth,
          renderedPageHeight,
          undefined,
          "FAST"
        );

        renderedHeightPx += sliceHeightPx;
        pageIndex += 1;
      }

      const sessionLabel = sanitizeFileNameSegment(
        (activeSession?.name ?? "").trim() || `session-${activeSession?.id.slice(0, 8) ?? "summary"}`
      );
      const fileName = `${sessionLabel || "summary"}-summary.pdf`;
      pdf.save(fileName);
      setStatus("status.exportPdfFinished", { fileName });
    } catch (error) {
      setStatus("status.exportPdfFailed", { error: String(error) });
      console.error("[summary-pdf] failed to export summary pdf", { error: String(error) });
    } finally {
      if (exportRootNode?.parentElement) {
        exportRootNode.parentElement.removeChild(exportRootNode);
      } else if (captureNode?.parentElement) {
        captureNode.parentElement.removeChild(captureNode);
      }
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

  async function onSetSessionDiscoverable(sessionId: string, discoverable: boolean) {
    const previousSession = sessions.find((session) => session.id === sessionId);
    const previousDiscoverable = previousSession?.discoverable ?? true;

    setSessions((previous) =>
      previous.map((session) =>
        session.id === sessionId ? { ...session, discoverable } : session
      )
    );
    setActiveSession((previous) =>
      previous && previous.id === sessionId ? { ...previous, discoverable } : previous
    );

    try {
      await setSessionDiscoverable(sessionId, discoverable);
      await refreshSessions();
      if (activeSessionId === sessionId) {
        await refreshSessionDetail(sessionId);
      }
      setStatus("status.sessionDiscoverableUpdated");
    } catch (error) {
      setSessions((previous) =>
        previous.map((session) =>
          session.id === sessionId ? { ...session, discoverable: previousDiscoverable } : session
        )
      );
      setActiveSession((previous) =>
        previous && previous.id === sessionId
          ? { ...previous, discoverable: previousDiscoverable }
          : previous
      );
      setStatus("status.sessionDiscoverableUpdateFailed", { error: String(error) });
      throw error;
    }
  }

  async function onUpdateSessionSummaryRawMarkdown(sessionId: string, rawMarkdown: string) {
    try {
      await updateSessionSummaryRawMarkdownApi(sessionId, rawMarkdown);
      await refreshSessions();
      if (activeSessionId === sessionId) {
        await refreshSessionDetail(sessionId);
      }
      setStatus("status.sessionSummaryUpdated");
    } catch (error) {
      setStatus("status.sessionSummaryUpdateFailed", { error: String(error) });
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
          onSetSessionDiscoverable={(sessionId, discoverable) =>
            void onSetSessionDiscoverable(sessionId, discoverable)
          }
          onUpdateSessionSummaryRawMarkdown={(sessionId, rawMarkdown) =>
            void onUpdateSessionSummaryRawMarkdown(sessionId, rawMarkdown)
          }
          onDeleteSession={(sessionId) => void handleDeleteSession(sessionId)}
          onPreparePlaybackAudio={() => onPrepareSessionPlaybackAudio()}
          onExportM4a={() => void onExport("m4a")}
          onExportMp3={() => void onExport("mp3")}
          onExportSummaryPdf={onExportSummaryPdf}
          onTranscribe={() => void onTranscribe()}
          onSummarize={() => void onSummarize()}
          t={t}
        />
      )}

      <div
        style={{ display: activeTab === "discover" ? "block" : "none" }}
        aria-hidden={activeTab !== "discover"}
      >
        <DiscoverTab
          sessions={sessions}
          t={t}
        />
      </div>

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
