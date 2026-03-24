import { useEffect, useState, type ChangeEvent, type KeyboardEvent } from "react";

import type { Translator } from "../i18n";
import type { Locale, TranslationKey } from "../i18n/messages";
import { formatFileSize } from "../lib/formatFileSize";
import { WHISPER_MODEL_PROFILES } from "../lib/localSttWhisperModels";
import type {
  OssConfig,
  OssProviderKind,
  PromptTemplate,
  ProviderCapability,
  ProviderConfig,
  ProviderKind,
  RecorderInputDevice,
  Settings,
  StorageUsageSummary
} from "../types/domain";

type SettingsTabProps = {
  locale: Locale;
  settings: Settings;
  inputDevices: RecorderInputDevice[];
  storageUsageState:
    | { status: "idle" }
    | { status: "loading"; summary?: StorageUsageSummary }
    | { status: "success"; summary: StorageUsageSummary }
    | { status: "error"; error: string; summary?: StorageUsageSummary };
  onLocaleChange: (locale: Locale) => void;
  onRefreshStorageUsage: () => void;
  onSettingsChange: (patch: Partial<Settings>) => void;
  onSave: () => void;
  t: Translator;
};

type SettingsSubTab = "general" | "provider" | "oss" | "templates" | "about";

type SettingsSubTabItem = {
  id: SettingsSubTab;
  labelKey: TranslationKey;
};

const settingsSubTabs: SettingsSubTabItem[] = [
  { id: "general", labelKey: "settings.tabs.general" },
  { id: "provider", labelKey: "settings.tabs.provider" },
  { id: "oss", labelKey: "settings.tabs.oss" },
  { id: "templates", labelKey: "settings.tabs.templates" },
  { id: "about", labelKey: "settings.tabs.about" }
];
const DEFAULT_RECORDING_SEGMENT_SECONDS = 120;

function buildTemplateId(name: string, index: number): string {
  const normalized = name
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9\-\s]/g, "")
    .replace(/\s+/g, "-")
    .replace(/\-+/g, "-");
  return normalized || `template-${index}`;
}

function createTemplate(index: number): PromptTemplate {
  const id = `template-${index}`;
  return {
    id,
    name: `Template ${index}`,
    systemPrompt: "You are an assistant for writing concise meeting notes.",
    userPrompt: "Organize transcript into: conclusion, action items, risks, timeline.",
    variables: ["language", "audience"]
  };
}

function supportsCapability(provider: ProviderConfig, capability: ProviderCapability): boolean {
  return provider.enabled && provider.capabilities.includes(capability);
}

function providerKindLabel(kind: ProviderKind, t: Translator): string {
  if (kind === "bailian") {
    return t("settings.transcriptionProvider.bailian");
  }
  if (kind === "aliyun_tingwu") {
    return t("settings.transcriptionProvider.aliyunTingwu");
  }
  if (kind === "openrouter") {
    return "OpenRouter";
  }
  if (kind === "ollama") {
    return "Ollama";
  }
  return t("settings.localStt");
}

function ossKindLabel(kind: OssProviderKind): string {
  return kind === "r2" ? "Cloudflare R2" : "Aliyun OSS";
}

function whisperModelOptionLabel(modelId: string): string {
  const profile = WHISPER_MODEL_PROFILES.find((item) => item.id === modelId);
  if (!profile) {
    return modelId;
  }
  return `${profile.label} | faster-whisper: ${profile.fasterWhisperModel} | mlx-whisper: ${profile.mlxWhisperModel}`;
}

function stringifyJsonObject(value: Record<string, unknown> | undefined): string {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return "";
  }
  try {
    return JSON.stringify(value, null, 2);
  } catch {
    return "";
  }
}

function SettingsTab({
  locale,
  settings,
  inputDevices,
  storageUsageState,
  onLocaleChange,
  onRefreshStorageUsage,
  onSettingsChange,
  onSave,
  t
}: SettingsTabProps) {
  const [activeSubTab, setActiveSubTab] = useState<SettingsSubTab>("general");
  const [activeProviderId, setActiveProviderId] = useState<string>(
    settings.providers[0]?.id ?? ""
  );
  const [activeOssConfigId, setActiveOssConfigId] = useState<string>(
    settings.selectedOssConfigId || settings.ossConfigs[0]?.id || ""
  );
  const [aliyunServiceInspectionDraft, setAliyunServiceInspectionDraft] = useState<
    Record<string, string>
  >({});
  const [aliyunCustomPromptDraft, setAliyunCustomPromptDraft] = useState<Record<string, string>>({});
  const [aliyunJsonFieldErrors, setAliyunJsonFieldErrors] = useState<
    Record<string, { serviceInspection?: TranslationKey; customPrompt?: TranslationKey }>
  >({});

  useEffect(() => {
    if (settings.providers.length === 0) {
      if (activeProviderId !== "") {
        setActiveProviderId("");
      }
      return;
    }

    const hasActiveProvider = settings.providers.some((provider) => provider.id === activeProviderId);
    if (!hasActiveProvider) {
      setActiveProviderId(settings.providers[0].id);
    }
  }, [settings.providers, activeProviderId]);

  useEffect(() => {
    if (settings.ossConfigs.length === 0) {
      if (activeOssConfigId !== "") {
        setActiveOssConfigId("");
      }
      return;
    }
    const hasActiveConfig = settings.ossConfigs.some((config) => config.id === activeOssConfigId);
    if (!hasActiveConfig) {
      setActiveOssConfigId(settings.selectedOssConfigId || settings.ossConfigs[0].id);
    }
  }, [settings.ossConfigs, settings.selectedOssConfigId, activeOssConfigId]);

  useEffect(() => {
    const aliyunProviders = settings.providers.filter(
      (provider) => provider.kind === "aliyun_tingwu" && provider.aliyunTingwu
    );

    setAliyunServiceInspectionDraft((previous) => {
      const next: Record<string, string> = {};
      for (const provider of aliyunProviders) {
        const current = previous[provider.id];
        if (typeof current === "string") {
          next[provider.id] = current;
          continue;
        }
        next[provider.id] = stringifyJsonObject(provider.aliyunTingwu?.realtimeServiceInspection);
      }
      return next;
    });

    setAliyunCustomPromptDraft((previous) => {
      const next: Record<string, string> = {};
      for (const provider of aliyunProviders) {
        const current = previous[provider.id];
        if (typeof current === "string") {
          next[provider.id] = current;
          continue;
        }
        next[provider.id] = stringifyJsonObject(provider.aliyunTingwu?.realtimeCustomPrompt);
      }
      return next;
    });

    setAliyunJsonFieldErrors((previous) => {
      const next: Record<
        string,
        { serviceInspection?: TranslationKey; customPrompt?: TranslationKey }
      > = {};
      for (const provider of aliyunProviders) {
        next[provider.id] = previous[provider.id] ?? {};
      }
      return next;
    });
  }, [settings.providers]);

  function handleLocaleChange(event: ChangeEvent<HTMLSelectElement>) {
    onLocaleChange(event.target.value as Locale);
  }

  function handleSubTabKeyDown(event: KeyboardEvent<HTMLButtonElement>, index: number) {
    if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") {
      return;
    }

    event.preventDefault();
    const direction = event.key === "ArrowRight" ? 1 : -1;
    const nextIndex = (index + direction + settingsSubTabs.length) % settingsSubTabs.length;
    setActiveSubTab(settingsSubTabs[nextIndex].id);
  }

  function handleTemplateChange(index: number, patch: Partial<PromptTemplate>) {
    const currentTemplate = settings.templates[index];
    const nextTemplate = { ...currentTemplate, ...patch };
    const templates = settings.templates.map((template, currentIndex) =>
      currentIndex === index ? nextTemplate : template
    );

    const defaultTemplateId =
      settings.defaultTemplateId === currentTemplate.id
        ? nextTemplate.id
        : settings.defaultTemplateId;

    onSettingsChange({ templates, defaultTemplateId });
  }

  function handleTemplateVariablesChange(index: number, value: string) {
    const variables = value
      .split(",")
      .map((part) => part.trim())
      .filter(Boolean);
    handleTemplateChange(index, { variables });
  }

  function addTemplate() {
    const nextIndex = settings.templates.length + 1;
    const template = createTemplate(nextIndex);
    const templates = [...settings.templates, template];
    onSettingsChange({ templates, defaultTemplateId: settings.defaultTemplateId || template.id });
  }

  function removeTemplate(index: number) {
    if (settings.templates.length <= 1) {
      return;
    }

    const templateToRemove = settings.templates[index];
    const templates = settings.templates.filter((_, currentIndex) => currentIndex !== index);

    const defaultTemplateId =
      settings.defaultTemplateId === templateToRemove.id
        ? templates[0]?.id ?? "meeting-default"
        : settings.defaultTemplateId;

    onSettingsChange({ templates, defaultTemplateId });
  }

  function updateProviders(nextProviders: ProviderConfig[]) {
    onSettingsChange({ providers: nextProviders });
  }

  function patchProvider(providerId: string, next: (provider: ProviderConfig) => ProviderConfig) {
    const providers = settings.providers.map((provider) =>
      provider.id === providerId ? next(provider) : provider
    );
    updateProviders(providers);
  }

  function updateAliyunJsonField(
    providerId: string,
    field: "serviceInspection" | "customPrompt",
    rawText: string,
    onValid: (value: Record<string, unknown> | undefined) => void
  ) {
    const setDraft =
      field === "serviceInspection" ? setAliyunServiceInspectionDraft : setAliyunCustomPromptDraft;
    setDraft((previous) => ({
      ...previous,
      [providerId]: rawText
    }));

    const trimmed = rawText.trim();
    if (!trimmed) {
      setAliyunJsonFieldErrors((previous) => ({
        ...previous,
        [providerId]: {
          ...(previous[providerId] ?? {}),
          [field]: undefined
        }
      }));
      onValid(undefined);
      return;
    }

    try {
      const parsed = JSON.parse(trimmed);
      if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
        throw new Error("not object");
      }
      setAliyunJsonFieldErrors((previous) => ({
        ...previous,
        [providerId]: {
          ...(previous[providerId] ?? {}),
          [field]: undefined
        }
      }));
      onValid(parsed as Record<string, unknown>);
    } catch {
      setAliyunJsonFieldErrors((previous) => ({
        ...previous,
        [providerId]: {
          ...(previous[providerId] ?? {}),
          [field]: "settings.aliyunJsonInvalidObject"
        }
      }));
    }
  }

  function updateOssConfigs(ossConfigs: OssConfig[]) {
    onSettingsChange({
      ossConfigs,
      selectedOssConfigId: settings.selectedOssConfigId
    });
  }

  function patchOssConfig(configId: string, next: (config: OssConfig) => OssConfig) {
    const ossConfigs = settings.ossConfigs.map((config) =>
      config.id === configId ? next(config) : config
    );
    updateOssConfigs(ossConfigs);
  }

  function renderProviderForm(provider: ProviderConfig) {
    if (provider.kind === "bailian") {
      const bailian = provider.bailian;
      if (!bailian) {
        return null;
      }
      return (
        <>
          <label>
            {t("settings.apiKey")}
            <input
              type="password"
              value={bailian.apiKey ?? ""}
              onChange={(event) =>
                patchProvider(provider.id, (current) => ({
                  ...current,
                  bailian: { ...bailian, apiKey: event.target.value }
                }))
              }
            />
          </label>
          <label>
            {t("settings.baseUrl")}
            <input
              value={bailian.baseUrl}
              onChange={(event) =>
                patchProvider(provider.id, (current) => ({
                  ...current,
                  bailian: { ...bailian, baseUrl: event.target.value }
                }))
              }
            />
          </label>
          <label>
            {t("settings.transcriptionModel")}
            <input
              value={bailian.transcriptionModel}
              onChange={(event) =>
                patchProvider(provider.id, (current) => ({
                  ...current,
                  bailian: { ...bailian, transcriptionModel: event.target.value }
                }))
              }
            />
          </label>
          <label>
            {t("settings.summaryModel")}
            <input
              value={bailian.summaryModel}
              onChange={(event) =>
                patchProvider(provider.id, (current) => ({
                  ...current,
                  bailian: { ...bailian, summaryModel: event.target.value }
                }))
              }
            />
          </label>
        </>
      );
    }

    if (provider.kind === "aliyun_tingwu") {
      const aliyun = provider.aliyunTingwu;
      if (!aliyun) {
        return null;
      }

      const updateAliyun = (next: Partial<typeof aliyun>) =>
        patchProvider(provider.id, (current) => ({
          ...current,
          aliyunTingwu: { ...aliyun, ...next }
        }));
      const serviceInspectionDraft =
        aliyunServiceInspectionDraft[provider.id] ??
        stringifyJsonObject(aliyun.realtimeServiceInspection);
      const customPromptDraft =
        aliyunCustomPromptDraft[provider.id] ?? stringifyJsonObject(aliyun.realtimeCustomPrompt);
      const serviceInspectionError = aliyunJsonFieldErrors[provider.id]?.serviceInspection;
      const customPromptError = aliyunJsonFieldErrors[provider.id]?.customPrompt;

      return (
        <>
          <section className="provider-subsection">
            <h4>{t("settings.aliyunSection.commonAccess")}</h4>
            <label>
              {t("settings.aliyunAccessKeyId")}
              <input
                value={aliyun.accessKeyId ?? ""}
                onChange={(event) => updateAliyun({ accessKeyId: event.target.value })}
              />
            </label>
            <label>
              {t("settings.aliyunAccessKeySecret")}
              <input
                type="password"
                value={aliyun.accessKeySecret ?? ""}
                onChange={(event) => updateAliyun({ accessKeySecret: event.target.value })}
              />
            </label>
            <label>
              {t("settings.aliyunAppKey")}
              <input
                value={aliyun.appKey ?? ""}
                onChange={(event) => updateAliyun({ appKey: event.target.value })}
              />
            </label>
            <label>
              {t("settings.aliyunEndpoint")}
              <input
                value={aliyun.endpoint}
                onChange={(event) => updateAliyun({ endpoint: event.target.value })}
              />
            </label>
          </section>

          <section className="provider-subsection">
            <h4>{t("settings.aliyunSection.offlineTranscription")}</h4>
            <label>
              {t("settings.aliyunSourceLanguage")}
              <select
                value={aliyun.sourceLanguage}
                onChange={(event) => updateAliyun({ sourceLanguage: event.target.value })}
              >
                <option value="cn">{t("settings.aliyunSourceLanguage.cn")}</option>
                <option value="en">{t("settings.aliyunSourceLanguage.en")}</option>
              </select>
            </label>
            <label>
              {t("settings.aliyunLanguageHints")}
              <input
                value={aliyun.languageHints ?? ""}
                onChange={(event) => updateAliyun({ languageHints: event.target.value })}
              />
            </label>
            <label>
              {t("settings.aliyunFileUrlPrefix")}
              <input
                value={aliyun.fileUrlPrefix ?? ""}
                onChange={(event) => updateAliyun({ fileUrlPrefix: event.target.value })}
              />
            </label>
            <label>
              {t("settings.aliyunNormalizationEnabled")}
              <select
                value={String(aliyun.transcriptionNormalizationEnabled)}
                onChange={(event) =>
                  updateAliyun({ transcriptionNormalizationEnabled: event.target.value === "true" })
                }
              >
                <option value="true">{t("settings.option.enabled")}</option>
                <option value="false">{t("settings.option.disabled")}</option>
              </select>
            </label>
            <label>
              {t("settings.aliyunParagraphEnabled")}
              <select
                value={String(aliyun.transcriptionParagraphEnabled)}
                onChange={(event) =>
                  updateAliyun({ transcriptionParagraphEnabled: event.target.value === "true" })
                }
              >
                <option value="true">{t("settings.option.enabled")}</option>
                <option value="false">{t("settings.option.disabled")}</option>
              </select>
            </label>
            <label>
              {t("settings.aliyunPunctuationPredictionEnabled")}
              <select
                value={String(aliyun.transcriptionPunctuationPredictionEnabled)}
                onChange={(event) =>
                  updateAliyun({
                    transcriptionPunctuationPredictionEnabled: event.target.value === "true"
                  })
                }
              >
                <option value="true">{t("settings.option.enabled")}</option>
                <option value="false">{t("settings.option.disabled")}</option>
              </select>
            </label>
            <label>
              {t("settings.aliyunDisfluencyRemovalEnabled")}
              <select
                value={String(aliyun.transcriptionDisfluencyRemovalEnabled)}
                onChange={(event) =>
                  updateAliyun({ transcriptionDisfluencyRemovalEnabled: event.target.value === "true" })
                }
              >
                <option value="true">{t("settings.option.enabled")}</option>
                <option value="false">{t("settings.option.disabled")}</option>
              </select>
            </label>
            <label>
              {t("settings.aliyunSpeakerDiarizationEnabled")}
              <select
                value={String(aliyun.transcriptionSpeakerDiarizationEnabled)}
                onChange={(event) =>
                  updateAliyun({
                    transcriptionSpeakerDiarizationEnabled: event.target.value === "true"
                  })
                }
              >
                <option value="true">{t("settings.option.enabled")}</option>
                <option value="false">{t("settings.option.disabled")}</option>
              </select>
            </label>
            <label>
              {t("settings.aliyunPollIntervalSeconds")}
              <input
                type="number"
                min={60}
                max={300}
                value={aliyun.pollIntervalSeconds}
                onChange={(event) =>
                  updateAliyun({ pollIntervalSeconds: Number.parseInt(event.target.value || "0", 10) || 60 })
                }
              />
            </label>
            <label>
              {t("settings.aliyunMaxPollingMinutes")}
              <input
                type="number"
                min={5}
                max={720}
                value={aliyun.maxPollingMinutes}
                onChange={(event) =>
                  updateAliyun({ maxPollingMinutes: Number.parseInt(event.target.value || "0", 10) || 180 })
                }
              />
            </label>
          </section>

          <section className="provider-subsection">
            <h4>{t("settings.aliyunSection.realtimeRecording")}</h4>
            <label>
              {t("settings.aliyunRealtimeEnabledByDefault")}
              <select
                value={String(aliyun.realtimeEnabledByDefault)}
                onChange={(event) =>
                  updateAliyun({ realtimeEnabledByDefault: event.target.value === "true" })
                }
              >
                <option value="true">{t("settings.option.enabled")}</option>
                <option value="false">{t("settings.option.disabled")}</option>
              </select>
            </label>
            <label>
              {t("settings.aliyunRealtimeFormat")}
              <select
                value={aliyun.realtimeFormat}
                onChange={(event) =>
                  updateAliyun({ realtimeFormat: event.target.value as typeof aliyun.realtimeFormat })
                }
              >
                <option value="pcm">pcm</option>
                <option value="opus">opus</option>
                <option value="aac">aac</option>
                <option value="speex">speex</option>
                <option value="mp3">mp3</option>
              </select>
            </label>
            <label>
              {t("settings.aliyunRealtimeSampleRate")}
              <select
                value={String(aliyun.realtimeSampleRate)}
                onChange={(event) =>
                  updateAliyun({
                    realtimeSampleRate: event.target.value === "8000" ? 8000 : 16000
                  })
                }
              >
                <option value="16000">16000</option>
                <option value="8000">8000</option>
              </select>
            </label>
            <label>
              {t("settings.aliyunRealtimeSourceLanguage")}
              <select
                value={aliyun.realtimeSourceLanguage}
                onChange={(event) =>
                  updateAliyun({
                    realtimeSourceLanguage: event.target.value as typeof aliyun.realtimeSourceLanguage
                  })
                }
              >
                <option value="cn">{t("settings.aliyunSourceLanguage.cn")}</option>
                <option value="en">{t("settings.aliyunSourceLanguage.en")}</option>
                <option value="yue">{t("settings.aliyunSourceLanguage.yue")}</option>
                <option value="ja">{t("settings.aliyunSourceLanguage.ja")}</option>
                <option value="ko">{t("settings.aliyunSourceLanguage.ko")}</option>
                <option value="multilingual">{t("settings.aliyunSourceLanguage.multilingual")}</option>
              </select>
            </label>
            <label>
              {t("settings.aliyunRealtimeLanguageHints")}
              <input
                value={aliyun.realtimeLanguageHints ?? ""}
                onChange={(event) => updateAliyun({ realtimeLanguageHints: event.target.value })}
              />
            </label>
            <label>
              {t("settings.aliyunRealtimeTaskKey")}
              <input
                value={aliyun.realtimeTaskKey ?? ""}
                onChange={(event) => updateAliyun({ realtimeTaskKey: event.target.value })}
              />
            </label>
            <label>
              {t("settings.aliyunRealtimeProgressiveCallbacksEnabled")}
              <select
                value={String(aliyun.realtimeProgressiveCallbacksEnabled)}
                onChange={(event) =>
                  updateAliyun({ realtimeProgressiveCallbacksEnabled: event.target.value === "true" })
                }
              >
                <option value="true">{t("settings.option.enabled")}</option>
                <option value="false">{t("settings.option.disabled")}</option>
              </select>
            </label>
            <label>
              {t("settings.aliyunRealtimeTranscodingTargetAudioFormat")}
              <select
                value={aliyun.realtimeTranscodingTargetAudioFormat ?? ""}
                onChange={(event) =>
                  updateAliyun({
                    realtimeTranscodingTargetAudioFormat:
                      event.target.value === "mp3" ? "mp3" : undefined
                  })
                }
              >
                <option value="">{t("settings.option.disabled")}</option>
                <option value="mp3">mp3</option>
              </select>
            </label>
            <label>
              {t("settings.aliyunRealtimeTranscriptionOutputLevel")}
              <select
                value={String(aliyun.realtimeTranscriptionOutputLevel)}
                onChange={(event) =>
                  updateAliyun({
                    realtimeTranscriptionOutputLevel: event.target.value === "2" ? 2 : 1
                  })
                }
              >
                <option value="1">{t("settings.aliyunRealtimeOutputLevel.finalOnly")}</option>
                <option value="2">{t("settings.aliyunRealtimeOutputLevel.intermediate")}</option>
              </select>
            </label>
            <label>
              {t("settings.aliyunRealtimeTranscriptionDiarizationEnabled")}
              <select
                value={String(aliyun.realtimeTranscriptionDiarizationEnabled)}
                onChange={(event) =>
                  updateAliyun({
                    realtimeTranscriptionDiarizationEnabled: event.target.value === "true"
                  })
                }
              >
                <option value="true">{t("settings.option.enabled")}</option>
                <option value="false">{t("settings.option.disabled")}</option>
              </select>
            </label>
            <label>
              {t("settings.aliyunRealtimeTranscriptionDiarizationSpeakerCount")}
              <input
                type="number"
                min={0}
                max={64}
                value={aliyun.realtimeTranscriptionDiarizationSpeakerCount ?? ""}
                onChange={(event) => {
                  const value = Number.parseInt(event.target.value, 10);
                  updateAliyun({
                    realtimeTranscriptionDiarizationSpeakerCount: Number.isFinite(value)
                      ? value
                      : undefined
                  });
                }}
              />
            </label>
            <label>
              {t("settings.aliyunRealtimeTranscriptionPhraseId")}
              <input
                value={aliyun.realtimeTranscriptionPhraseId ?? ""}
                onChange={(event) =>
                  updateAliyun({ realtimeTranscriptionPhraseId: event.target.value })
                }
              />
            </label>
            <label>
              {t("settings.aliyunRealtimeTranslationEnabled")}
              <select
                value={String(aliyun.realtimeTranslationEnabled)}
                onChange={(event) =>
                  updateAliyun({ realtimeTranslationEnabled: event.target.value === "true" })
                }
              >
                <option value="true">{t("settings.option.enabled")}</option>
                <option value="false">{t("settings.option.disabled")}</option>
              </select>
            </label>
            <label>
              {t("settings.aliyunRealtimeTranslationOutputLevel")}
              <select
                value={String(aliyun.realtimeTranslationOutputLevel)}
                onChange={(event) =>
                  updateAliyun({
                    realtimeTranslationOutputLevel: event.target.value === "2" ? 2 : 1
                  })
                }
              >
                <option value="1">{t("settings.aliyunRealtimeOutputLevel.finalOnly")}</option>
                <option value="2">{t("settings.aliyunRealtimeOutputLevel.intermediate")}</option>
              </select>
            </label>
            <label>
              {t("settings.aliyunRealtimeTranslationTargetLanguages")}
              <input
                value={aliyun.realtimeTranslationTargetLanguages ?? ""}
                onChange={(event) =>
                  updateAliyun({ realtimeTranslationTargetLanguages: event.target.value })
                }
              />
            </label>
            <label>
              {t("settings.aliyunRealtimeAutoChaptersEnabled")}
              <select
                value={String(aliyun.realtimeAutoChaptersEnabled)}
                onChange={(event) =>
                  updateAliyun({ realtimeAutoChaptersEnabled: event.target.value === "true" })
                }
              >
                <option value="true">{t("settings.option.enabled")}</option>
                <option value="false">{t("settings.option.disabled")}</option>
              </select>
            </label>
            <label>
              {t("settings.aliyunRealtimeMeetingAssistanceEnabled")}
              <select
                value={String(aliyun.realtimeMeetingAssistanceEnabled)}
                onChange={(event) =>
                  updateAliyun({ realtimeMeetingAssistanceEnabled: event.target.value === "true" })
                }
              >
                <option value="true">{t("settings.option.enabled")}</option>
                <option value="false">{t("settings.option.disabled")}</option>
              </select>
            </label>
            <label>
              {t("settings.aliyunRealtimeSummarizationEnabled")}
              <select
                value={String(aliyun.realtimeSummarizationEnabled)}
                onChange={(event) =>
                  updateAliyun({ realtimeSummarizationEnabled: event.target.value === "true" })
                }
              >
                <option value="true">{t("settings.option.enabled")}</option>
                <option value="false">{t("settings.option.disabled")}</option>
              </select>
            </label>
            <label>
              {t("settings.aliyunRealtimeSummarizationTypes")}
              <input
                value={aliyun.realtimeSummarizationTypes ?? ""}
                onChange={(event) => updateAliyun({ realtimeSummarizationTypes: event.target.value })}
              />
            </label>
            <label>
              {t("settings.aliyunRealtimeTextPolishEnabled")}
              <select
                value={String(aliyun.realtimeTextPolishEnabled)}
                onChange={(event) =>
                  updateAliyun({ realtimeTextPolishEnabled: event.target.value === "true" })
                }
              >
                <option value="true">{t("settings.option.enabled")}</option>
                <option value="false">{t("settings.option.disabled")}</option>
              </select>
            </label>
            <label>
              {t("settings.aliyunRealtimeServiceInspectionEnabled")}
              <select
                value={String(aliyun.realtimeServiceInspectionEnabled)}
                onChange={(event) =>
                  updateAliyun({ realtimeServiceInspectionEnabled: event.target.value === "true" })
                }
              >
                <option value="true">{t("settings.option.enabled")}</option>
                <option value="false">{t("settings.option.disabled")}</option>
              </select>
            </label>
            <label>
              {t("settings.aliyunRealtimeServiceInspection")}
              <textarea
                value={serviceInspectionDraft}
                onChange={(event) =>
                  updateAliyunJsonField(
                    provider.id,
                    "serviceInspection",
                    event.target.value,
                    (value) => updateAliyun({ realtimeServiceInspection: value })
                  )
                }
              />
              {serviceInspectionError ? (
                <span className="provider-validation-error">{t(serviceInspectionError)}</span>
              ) : null}
            </label>
            <label>
              {t("settings.aliyunRealtimeCustomPromptEnabled")}
              <select
                value={String(aliyun.realtimeCustomPromptEnabled)}
                onChange={(event) =>
                  updateAliyun({ realtimeCustomPromptEnabled: event.target.value === "true" })
                }
              >
                <option value="true">{t("settings.option.enabled")}</option>
                <option value="false">{t("settings.option.disabled")}</option>
              </select>
            </label>
            <label>
              {t("settings.aliyunRealtimeCustomPrompt")}
              <textarea
                value={customPromptDraft}
                onChange={(event) =>
                  updateAliyunJsonField(
                    provider.id,
                    "customPrompt",
                    event.target.value,
                    (value) => updateAliyun({ realtimeCustomPrompt: value })
                  )
                }
              />
              {customPromptError ? (
                <span className="provider-validation-error">{t(customPromptError)}</span>
              ) : null}
            </label>
          </section>
        </>
      );
    }

    if (provider.kind === "openrouter") {
      const openrouter = provider.openrouter;
      if (!openrouter) {
        return null;
      }
      return (
        <>
          <label>
            {t("settings.openrouterApiKey")}
            <input
              type="password"
              value={openrouter.apiKey ?? ""}
              onChange={(event) =>
                patchProvider(provider.id, (current) => ({
                  ...current,
                  openrouter: { ...openrouter, apiKey: event.target.value }
                }))
              }
            />
          </label>
          <label>
            {t("settings.openrouterBaseUrl")}
            <input
              value={openrouter.baseUrl}
              onChange={(event) =>
                patchProvider(provider.id, (current) => ({
                  ...current,
                  openrouter: { ...openrouter, baseUrl: event.target.value }
                }))
              }
            />
          </label>
          <label>
            {t("settings.openrouterSummaryModel")}
            <input
              value={openrouter.summaryModel}
              onChange={(event) =>
                patchProvider(provider.id, (current) => ({
                  ...current,
                  openrouter: { ...openrouter, summaryModel: event.target.value }
                }))
              }
            />
          </label>
          <label>
            {t("settings.openrouterDiscoverModel")}
            <input
              value={openrouter.discoverModel}
              onChange={(event) =>
                patchProvider(provider.id, (current) => ({
                  ...current,
                  openrouter: { ...openrouter, discoverModel: event.target.value }
                }))
              }
            />
          </label>
        </>
      );
    }

    if (provider.kind === "ollama") {
      const ollama = provider.ollama;
      if (!ollama) {
        return null;
      }
      return (
        <>
          <label>
            {t("settings.ollamaApiKey")}
            <input
              type="password"
              value={ollama.apiKey ?? ""}
              onChange={(event) =>
                patchProvider(provider.id, (current) => ({
                  ...current,
                  ollama: { ...ollama, apiKey: event.target.value }
                }))
              }
            />
          </label>
          <label>
            {t("settings.ollamaBaseUrl")}
            <input
              value={ollama.baseUrl}
              onChange={(event) =>
                patchProvider(provider.id, (current) => ({
                  ...current,
                  ollama: { ...ollama, baseUrl: event.target.value }
                }))
              }
            />
          </label>
          <label>
            {t("settings.ollamaSummaryModel")}
            <input
              value={ollama.summaryModel}
              onChange={(event) =>
                patchProvider(provider.id, (current) => ({
                  ...current,
                  ollama: { ...ollama, summaryModel: event.target.value }
                }))
              }
            />
          </label>
        </>
      );
    }

    const localStt = provider.localStt;
    if (!localStt) {
      return null;
    }

    const updateLocalStt = (next: Partial<typeof localStt>) =>
      patchProvider(provider.id, (current) => ({
        ...current,
        localStt: { ...localStt, ...next }
      }));

    return (
      <>
        <label>
          {t("settings.localSttEngine")}
          <select
            value={localStt.engine}
            onChange={(event) =>
              updateLocalStt({
                engine: event.target.value as typeof localStt.engine
              })
            }
          >
            <option value="whisper">{t("settings.localSttEngine.whisper")}</option>
            <option value="sensevoice_small">{t("settings.localSttEngine.sensevoice")}</option>
          </select>
        </label>

        {localStt.engine === "whisper" ? (
          <label>
            {t("settings.localWhisperModel")}
            <select
              value={localStt.whisperModel}
              onChange={(event) =>
                updateLocalStt({
                  whisperModel: event.target.value as typeof localStt.whisperModel
                })
              }
            >
              {WHISPER_MODEL_PROFILES.map((model) => (
                <option key={model.id} value={model.id}>
                  {whisperModelOptionLabel(model.id)}
                </option>
              ))}
            </select>
          </label>
        ) : (
          <label>
            {t("settings.localSenseVoiceModel")}
            <input
              value={localStt.senseVoiceModel}
              onChange={(event) => updateLocalStt({ senseVoiceModel: event.target.value })}
            />
          </label>
        )}

        <label>
          {t("settings.localSttLanguage")}
          <select
            value={localStt.language}
            onChange={(event) =>
              updateLocalStt({
                language: event.target.value as typeof localStt.language
              })
            }
          >
            <option value="auto">{t("settings.localSttLanguage.auto")}</option>
            <option value="zh">{t("settings.localSttLanguage.zh")}</option>
            <option value="en">{t("settings.localSttLanguage.en")}</option>
          </select>
        </label>

        <label>
          {t("settings.localSttDiarization")}
          <select
            value={String(localStt.diarizationEnabled)}
            onChange={(event) => updateLocalStt({ diarizationEnabled: event.target.value === "true" })}
          >
            <option value="true">{t("settings.option.enabled")}</option>
            <option value="false">{t("settings.option.disabled")}</option>
          </select>
        </label>

        <label>
          {t("settings.localSttComputeDevice")}
          <select
            value={localStt.computeDevice}
            onChange={(event) =>
              updateLocalStt({
                computeDevice: event.target.value as typeof localStt.computeDevice
              })
            }
          >
            <option value="auto">auto</option>
            <option value="cpu">cpu</option>
            <option value="mps">mps</option>
            <option value="cuda">cuda</option>
          </select>
        </label>

        <label>
          {t("settings.localSttVad")}
          <select
            value={String(localStt.vadEnabled)}
            onChange={(event) => updateLocalStt({ vadEnabled: event.target.value === "true" })}
          >
            <option value="true">{t("settings.option.enabled")}</option>
            <option value="false">{t("settings.option.disabled")}</option>
          </select>
        </label>

        <label>
          {t("settings.localSttChunkSeconds")}
          <input
            type="number"
            min={5}
            max={180}
            value={localStt.chunkSeconds}
            onChange={(event) =>
              updateLocalStt({ chunkSeconds: Number.parseInt(event.target.value || "0", 10) || 30 })
            }
          />
        </label>

        <label>
          {t("settings.localSttSpeakerCountHint")}
          <input
            type="number"
            min={1}
            max={16}
            value={localStt.speakerCountHint ?? ""}
            onChange={(event) => {
              const value = Number.parseInt(event.target.value, 10);
              updateLocalStt({
                speakerCountHint: Number.isFinite(value) ? value : undefined
              });
            }}
          />
        </label>

        <label>
          {t("settings.localSttMinSpeakers")}
          <input
            type="number"
            min={1}
            max={16}
            value={localStt.minSpeakers ?? ""}
            onChange={(event) => {
              const value = Number.parseInt(event.target.value, 10);
              updateLocalStt({ minSpeakers: Number.isFinite(value) ? value : undefined });
            }}
          />
        </label>

        <label>
          {t("settings.localSttMaxSpeakers")}
          <input
            type="number"
            min={1}
            max={16}
            value={localStt.maxSpeakers ?? ""}
            onChange={(event) => {
              const value = Number.parseInt(event.target.value, 10);
              updateLocalStt({ maxSpeakers: Number.isFinite(value) ? value : undefined });
            }}
          />
        </label>

        <label>
          {t("settings.localSttPythonPath")}
          <input
            value={localStt.pythonPath ?? ""}
            onChange={(event) => updateLocalStt({ pythonPath: event.target.value })}
          />
        </label>

        <label>
          {t("settings.localSttVenvDir")}
          <input
            value={localStt.venvDir ?? ""}
            onChange={(event) => updateLocalStt({ venvDir: event.target.value })}
          />
        </label>

        <label>
          {t("settings.localSttModelCacheDir")}
          <input
            value={localStt.modelCacheDir ?? ""}
            onChange={(event) => updateLocalStt({ modelCacheDir: event.target.value })}
          />
        </label>
      </>
    );
  }

  const transcriptionProviders = settings.providers.filter((provider) =>
    supportsCapability(provider, "transcription")
  );
  const selectedInputDeviceId =
    typeof settings.recordingInputDeviceId === "string" ? settings.recordingInputDeviceId : "";
  const hasSelectedInputDevice =
    selectedInputDeviceId.length === 0 ||
    inputDevices.some((device) => device.id === selectedInputDeviceId);
  const summaryProviders = settings.providers.filter((provider) =>
    supportsCapability(provider, "summary")
  );
  const discoverProviders = settings.providers.filter((provider) =>
    supportsCapability(provider, "summary")
  );
  const activeProvider = settings.providers.find((provider) => provider.id === activeProviderId);
  const activeOssConfig = settings.ossConfigs.find((config) => config.id === activeOssConfigId);
  const hasValidationErrors = Object.values(aliyunJsonFieldErrors).some(
    (value) => Boolean(value.serviceInspection || value.customPrompt)
  );
  const storageUsageSummary =
    storageUsageState.status === "loading" ||
    storageUsageState.status === "success" ||
    storageUsageState.status === "error"
      ? storageUsageState.summary
      : undefined;
  const storageUsageValue =
    storageUsageState.status === "success"
      ? formatFileSize(storageUsageState.summary.totalBytes)
      : storageUsageState.status === "loading"
        ? t("settings.storageUsageRefreshing")
        : storageUsageState.status === "error"
          ? t("settings.storageUsageError")
          : t("settings.storageUsageIdle");
  const storageUsageError =
    storageUsageState.status === "error"
      ? t("settings.storageUsageErrorDetail", { error: storageUsageState.error })
      : undefined;
  const isRefreshingStorageUsage = storageUsageState.status === "loading";

  return (
    <section className="panel settings-panel">
      <header>
        <h2>{t("settings.title")}</h2>
        <p>{t("settings.subtitle")}</p>
      </header>

      <nav className="settings-subtabs" aria-label={t("settings.tabs.ariaLabel")} role="tablist">
        {settingsSubTabs.map((tab, index) => {
          const active = activeSubTab === tab.id;
          return (
            <button
              key={tab.id}
              type="button"
              className={`settings-subtab-trigger${active ? " active" : ""}`}
              role="tab"
              id={`settings-tab-${tab.id}`}
              aria-selected={active}
              aria-controls={`settings-panel-${tab.id}`}
              tabIndex={active ? 0 : -1}
              onClick={() => setActiveSubTab(tab.id)}
              onKeyDown={(event) => handleSubTabKeyDown(event, index)}
            >
              {t(tab.labelKey)}
            </button>
          );
        })}
      </nav>

      <div
        className="settings-tab-content"
        role="tabpanel"
        id={`settings-panel-${activeSubTab}`}
        aria-labelledby={`settings-tab-${activeSubTab}`}
      >
        {activeSubTab === "general" && (
          <div className="settings-section">
            <h3>{t("settings.interface")}</h3>
            <label>
              {t("settings.language")}
              <select value={locale} onChange={handleLocaleChange}>
                <option value="zh-CN">{t("settings.language.zh")}</option>
                <option value="en-US">{t("settings.language.en")}</option>
              </select>
            </label>
            <label>
              {t("settings.recordingSegmentSeconds")}
              <input
                type="number"
                min={10}
                max={1800}
                value={settings.recordingSegmentSeconds}
                onChange={(event) =>
                  onSettingsChange({
                    recordingSegmentSeconds:
                      Number.parseInt(event.target.value || "0", 10) || DEFAULT_RECORDING_SEGMENT_SECONDS
                  })
                }
              />
            </label>
            <label>
              {t("settings.recordingInputDevice")}
              <select
                value={selectedInputDeviceId}
                onChange={(event) =>
                  onSettingsChange({ recordingInputDeviceId: event.target.value })
                }
              >
                <option value="">
                  {t("settings.recordingInputDevice.systemDefault")}
                </option>
                {inputDevices.map((device) => (
                  <option key={device.id} value={device.id}>
                    {device.name}
                    {device.isDefault
                      ? ` (${t("settings.recordingInputDevice.defaultSuffix")})`
                      : ""}
                  </option>
                ))}
                {!hasSelectedInputDevice && selectedInputDeviceId.length > 0 && (
                  <option value={selectedInputDeviceId}>
                    {t("settings.recordingInputDevice.unavailable", {
                      deviceId: selectedInputDeviceId
                    })}
                  </option>
                )}
              </select>
            </label>

            <section className="settings-storage-usage" aria-live="polite">
              <div className="settings-storage-usage-header">
                <div>
                  <h4>{t("settings.storageUsageTitle")}</h4>
                  <p>{t("settings.storageUsageHint")}</p>
                </div>
                <button
                  type="button"
                  className="btn-secondary settings-inline-btn"
                  onClick={onRefreshStorageUsage}
                  disabled={isRefreshingStorageUsage}
                >
                  {isRefreshingStorageUsage
                    ? t("settings.storageUsageRefreshing")
                    : t("settings.storageUsageRefresh")}
                </button>
              </div>

              <dl className="settings-storage-usage-grid">
                <div>
                  <dt>{t("settings.storageUsagePath")}</dt>
                  <dd>{storageUsageSummary?.dataDirPath ?? "—"}</dd>
                </div>
                <div>
                  <dt>{t("settings.storageUsageSize")}</dt>
                  <dd>{storageUsageValue}</dd>
                </div>
              </dl>

              {storageUsageError ? (
                <p className="settings-storage-usage-error">{storageUsageError}</p>
              ) : null}
            </section>
          </div>
        )}

        {activeSubTab === "provider" && (
          <div className="settings-section">
            <h3>{t("settings.providerConfigs")}</h3>
            <div className="provider-layout">
              <aside className="provider-list" role="listbox" aria-label={t("settings.providerSelect")}>
                {settings.providers.map((provider) => {
                  const active = provider.id === activeProviderId;
                  return (
                    <button
                      key={provider.id}
                      type="button"
                      role="option"
                      aria-selected={active}
                      className={`provider-list-item${active ? " active" : ""}`}
                      onClick={() => setActiveProviderId(provider.id)}
                    >
                      <strong>{providerKindLabel(provider.kind, t)}</strong>
                      <span>{provider.name}</span>
                      <span className={`provider-list-state${provider.enabled ? " enabled" : " disabled"}`}>
                        {provider.enabled ? t("settings.option.enabled") : t("settings.option.disabled")}
                      </span>
                    </button>
                  );
                })}
              </aside>

              <div>
                {activeProvider ? (
                  <article className="provider-editor">
                    <div className="provider-editor-header">
                      <strong>{providerKindLabel(activeProvider.kind, t)}</strong>
                    </div>

                    <label>
                      {t("settings.providerName")}
                      <input
                        value={activeProvider.name}
                        onChange={(event) =>
                          patchProvider(activeProvider.id, (current) => ({
                            ...current,
                            name: event.target.value
                          }))
                        }
                      />
                    </label>

                    <label>
                      {t("settings.providerEnabled")}
                      <select
                        value={String(activeProvider.enabled)}
                        onChange={(event) =>
                          patchProvider(activeProvider.id, (current) => ({
                            ...current,
                            enabled: event.target.value === "true"
                          }))
                        }
                      >
                        <option value="true">{t("settings.option.enabled")}</option>
                        <option value="false">{t("settings.option.disabled")}</option>
                      </select>
                    </label>

                    <p>
                      {t("settings.capabilities")}:
                      {activeProvider.capabilities
                        .map((capability) =>
                          capability === "transcription"
                            ? t("settings.capability.transcription")
                            : t("settings.capability.summary")
                        )
                        .join(" / ")}
                    </p>

                    {renderProviderForm(activeProvider)}
                  </article>
                ) : (
                  <p className="provider-empty-hint">{t("settings.emptyProviders")}</p>
                )}
              </div>
            </div>

            <div className="provider-selectors">
              <label>
                {t("settings.transcriptionProvider")}
                <select
                  value={settings.selectedTranscriptionProviderId}
                  onChange={(event) =>
                    onSettingsChange({ selectedTranscriptionProviderId: event.target.value })
                  }
                >
                  {transcriptionProviders.length === 0 && (
                    <option value="">{t("settings.noProvider")}</option>
                  )}
                  {transcriptionProviders.map((provider) => (
                    <option key={provider.id} value={provider.id}>
                      {provider.name} ({providerKindLabel(provider.kind, t)})
                    </option>
                  ))}
                </select>
              </label>

              <label>
                {t("settings.summaryProvider")}
                <select
                  value={settings.selectedSummaryProviderId}
                  onChange={(event) =>
                    onSettingsChange({ selectedSummaryProviderId: event.target.value })
                  }
                >
                  {summaryProviders.length === 0 && <option value="">{t("settings.noProvider")}</option>}
                  {summaryProviders.map((provider) => (
                    <option key={provider.id} value={provider.id}>
                      {provider.name} ({providerKindLabel(provider.kind, t)})
                    </option>
                  ))}
                </select>
              </label>

              <label>
                {t("settings.discoverProvider")}
                <select
                  value={settings.selectedDiscoverProviderId}
                  onChange={(event) =>
                    onSettingsChange({ selectedDiscoverProviderId: event.target.value })
                  }
                >
                  {discoverProviders.length === 0 && <option value="">{t("settings.noProvider")}</option>}
                  {discoverProviders.map((provider) => (
                    <option key={provider.id} value={provider.id}>
                      {provider.name} ({providerKindLabel(provider.kind, t)})
                    </option>
                  ))}
                </select>
              </label>
            </div>
          </div>
        )}

        {activeSubTab === "oss" && (
          <div className="settings-section">
            <h3>{t("settings.tabs.oss")}</h3>

            <div className="provider-toolbar">
              <label>
                {t("settings.ossConfig")}
                <select
                  value={activeOssConfigId}
                  onChange={(event) => setActiveOssConfigId(event.target.value)}
                >
                  {settings.ossConfigs.length === 0 && (
                    <option value="">{t("settings.noOssConfig")}</option>
                  )}
                  {settings.ossConfigs.map((config) => (
                    <option key={config.id} value={config.id}>
                      {config.name} ({ossKindLabel(config.kind)})
                    </option>
                  ))}
                </select>
              </label>
            </div>

            <label>
              {t("settings.selectedOssConfig")}
              <select
                value={settings.selectedOssConfigId}
                onChange={(event) => onSettingsChange({ selectedOssConfigId: event.target.value })}
              >
                {settings.ossConfigs.length === 0 && (
                  <option value="">{t("settings.noOssConfig")}</option>
                )}
                {settings.ossConfigs.map((config) => (
                  <option key={config.id} value={config.id}>
                    {config.name} ({ossKindLabel(config.kind)})
                  </option>
                ))}
              </select>
            </label>

            {activeOssConfig ? (
              <article className="provider-editor">
                <div className="provider-editor-header">
                  <strong>{ossKindLabel(activeOssConfig.kind)}</strong>
                </div>

                <label>
                  {t("settings.ossConfigName")}
                  <input
                    value={activeOssConfig.name}
                    onChange={(event) =>
                      patchOssConfig(activeOssConfig.id, (current) => ({
                        ...current,
                        name: event.target.value
                      }))
                    }
                  />
                </label>

                <label>
                  {t("settings.ossAccessKeyId")}
                  <input
                    value={activeOssConfig.accessKeyId ?? ""}
                    onChange={(event) =>
                      patchOssConfig(activeOssConfig.id, (current) => ({
                        ...current,
                        accessKeyId: event.target.value
                      }))
                    }
                  />
                </label>

                <label>
                  {t("settings.ossAccessKeySecret")}
                  <input
                    type="password"
                    value={activeOssConfig.accessKeySecret ?? ""}
                    onChange={(event) =>
                      patchOssConfig(activeOssConfig.id, (current) => ({
                        ...current,
                        accessKeySecret: event.target.value
                      }))
                    }
                  />
                </label>

                <label>
                  {t("settings.ossEndpoint")}
                  <input
                    value={activeOssConfig.endpoint ?? ""}
                    onChange={(event) =>
                      patchOssConfig(activeOssConfig.id, (current) => ({
                        ...current,
                        endpoint: event.target.value
                      }))
                    }
                  />
                </label>

                <label>
                  {t("settings.ossBucket")}
                  <input
                    value={activeOssConfig.bucket ?? ""}
                    onChange={(event) =>
                      patchOssConfig(activeOssConfig.id, (current) => ({
                        ...current,
                        bucket: event.target.value
                      }))
                    }
                  />
                </label>

                <label>
                  {t("settings.ossPathPrefix")}
                  <input
                    value={activeOssConfig.pathPrefix ?? ""}
                    onChange={(event) =>
                      patchOssConfig(activeOssConfig.id, (current) => ({
                        ...current,
                        pathPrefix: event.target.value
                      }))
                    }
                  />
                </label>

                <label>
                  {t("settings.ossSignedUrlTtlSeconds")}
                  <input
                    type="number"
                    min={60}
                    max={86400}
                    value={activeOssConfig.signedUrlTtlSeconds}
                    onChange={(event) =>
                      patchOssConfig(activeOssConfig.id, (current) => ({
                        ...current,
                        signedUrlTtlSeconds:
                          Number.parseInt(event.target.value || "0", 10) || 1800
                      }))
                    }
                  />
                </label>
              </article>
            ) : (
              <p className="provider-empty-hint">{t("settings.noOssConfig")}</p>
            )}
          </div>
        )}

        {activeSubTab === "templates" && (
          <div className="settings-section">
            <h3>{t("settings.prompts")}</h3>
            <p>{t("settings.templateHint")}</p>

            <label>
              {t("settings.defaultTemplateId")}
              <select
                value={settings.defaultTemplateId}
                onChange={(event) => onSettingsChange({ defaultTemplateId: event.target.value })}
              >
                {settings.templates.map((template) => (
                  <option key={template.id} value={template.id}>
                    {template.name} ({template.id})
                  </option>
                ))}
              </select>
            </label>

            <div className="template-list">
              {settings.templates.map((template, index) => (
                <article key={`${template.id}-${index}`} className="template-card">
                  <div className="template-actions">
                    <strong>{template.name}</strong>
                    <button type="button" onClick={() => removeTemplate(index)}>
                      {t("settings.removeTemplate")}
                    </button>
                  </div>

                  <label>
                    {t("settings.templateName")}
                    <input
                      value={template.name}
                      onChange={(event) => {
                        const nextName = event.target.value;
                        handleTemplateChange(index, {
                          name: nextName,
                          id: template.id.startsWith("template-")
                            ? buildTemplateId(nextName, index + 1)
                            : template.id
                        });
                      }}
                    />
                  </label>

                  <label>
                    {t("settings.templateId")}
                    <input
                      value={template.id}
                      onChange={(event) =>
                        handleTemplateChange(index, {
                          id: buildTemplateId(event.target.value, index + 1)
                        })
                      }
                    />
                  </label>

                  <label>
                    {t("settings.systemPrompt")}
                    <textarea
                      value={template.systemPrompt}
                      onChange={(event) =>
                        handleTemplateChange(index, {
                          systemPrompt: event.target.value
                        })
                      }
                    />
                  </label>

                  <label>
                    {t("settings.userPrompt")}
                    <textarea
                      value={template.userPrompt}
                      onChange={(event) =>
                        handleTemplateChange(index, {
                          userPrompt: event.target.value
                        })
                      }
                    />
                  </label>

                  <label>
                    {t("settings.variables")}
                    <input
                      value={template.variables.join(", ")}
                      onChange={(event) => handleTemplateVariablesChange(index, event.target.value)}
                    />
                  </label>
                </article>
              ))}
            </div>

            <button type="button" className="settings-inline-btn" onClick={addTemplate}>
              {t("settings.addTemplate")}
            </button>
          </div>
        )}

        {activeSubTab === "about" && (
          <div className="settings-section">
            <h3>{t("settings.tabs.about")}</h3>
            <p>
              <strong>{t("settings.about.author")}:</strong> renx
            </p>
            <p>
              <strong>{t("settings.about.version")}:</strong>{" "}
              {t("settings.about.versionValue", { version: __APP_VERSION__ })}
            </p>
          </div>
        )}
      </div>

      <div className="settings-save-row">
        {hasValidationErrors ? (
          <p className="provider-validation-error">{t("settings.aliyunJsonFixBeforeSave")}</p>
        ) : null}
        <button
          type="button"
          className="settings-save-btn"
          onClick={onSave}
          disabled={hasValidationErrors}
        >
          {t("settings.save")}
        </button>
      </div>
    </section>
  );
}

export default SettingsTab;
