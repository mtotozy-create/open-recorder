import { useEffect, useState, type ChangeEvent, type KeyboardEvent } from "react";

import type { Translator } from "../i18n";
import type { Locale, TranslationKey } from "../i18n/messages";
import type {
  OssConfig,
  OssProviderKind,
  PromptTemplate,
  ProviderCapability,
  ProviderConfig,
  ProviderKind,
  Settings
} from "../types/domain";

type SettingsTabProps = {
  locale: Locale;
  settings: Settings;
  onLocaleChange: (locale: Locale) => void;
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
  return t("settings.localStt");
}

function ossKindLabel(kind: OssProviderKind): string {
  return kind === "r2" ? "Cloudflare R2" : "Aliyun OSS";
}

function SettingsTab({
  locale,
  settings,
  onLocaleChange,
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

      return (
        <>
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
                updateAliyun({ transcriptionPunctuationPredictionEnabled: event.target.value === "true" })
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
              <option value="small">small</option>
              <option value="medium">medium</option>
              <option value="large-v3">large-v3</option>
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
  const summaryProviders = settings.providers.filter((provider) =>
    supportsCapability(provider, "summary")
  );
  const activeProvider = settings.providers.find((provider) => provider.id === activeProviderId);
  const activeOssConfig = settings.ossConfigs.find((config) => config.id === activeOssConfigId);

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
        <button type="button" className="settings-save-btn" onClick={onSave}>
          {t("settings.save")}
        </button>
      </div>
    </section>
  );
}

export default SettingsTab;
