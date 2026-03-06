import type { ChangeEvent } from "react";

import type { Translator } from "../i18n";
import type { Locale } from "../i18n/messages";
import type { PromptTemplate, Settings } from "../types/domain";

type SettingsTabProps = {
  locale: Locale;
  settings: Settings;
  onLocaleChange: (locale: Locale) => void;
  onSettingsChange: (patch: Partial<Settings>) => void;
  onSave: () => void;
  t: Translator;
};

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

function SettingsTab({
  locale,
  settings,
  onLocaleChange,
  onSettingsChange,
  onSave,
  t
}: SettingsTabProps) {
  const isBailianSelected = settings.transcriptionProvider === "bailian";
  const isAliyunSelected = settings.transcriptionProvider === "aliyun_tingwu";

  function handleLocaleChange(event: ChangeEvent<HTMLSelectElement>) {
    onLocaleChange(event.target.value as Locale);
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

  return (
    <section className="panel settings-panel">
      <header>
        <h2>{t("settings.title")}</h2>
        <p>{t("settings.subtitle")}</p>
      </header>

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

      <div className="settings-section">
        <h3>{t("settings.transcriptionProvider")}</h3>
        <p>{t("settings.transcriptionProviderHint")}</p>
        <label>
          {t("settings.transcriptionProvider")}
          <select
            value={settings.transcriptionProvider}
            onChange={(event) =>
              onSettingsChange({
                transcriptionProvider: event.target.value as Settings["transcriptionProvider"]
              })
            }
          >
            <option value="bailian">{t("settings.transcriptionProvider.bailian")}</option>
            <option value="aliyun_tingwu">{t("settings.transcriptionProvider.aliyunTingwu")}</option>
          </select>
        </label>
      </div>

      <div className="settings-section">
        <h3>{t("settings.bailian")}</h3>
        <label>
          {t("settings.apiKey")}
          <input
            type="password"
            value={settings.bailianApiKey ?? ""}
            onChange={(event) => onSettingsChange({ bailianApiKey: event.target.value })}
          />
        </label>
        <label>
          {t("settings.baseUrl")}
          <input
            value={settings.bailianBaseUrl}
            onChange={(event) => onSettingsChange({ bailianBaseUrl: event.target.value })}
          />
        </label>
        <label>
          {t("settings.transcriptionModel")}
          <input
            value={settings.bailianTranscriptionModel}
            onChange={(event) => onSettingsChange({ bailianTranscriptionModel: event.target.value })}
          />
        </label>
        <label>
          {t("settings.summaryModel")}
          <input
            value={settings.bailianSummaryModel}
            onChange={(event) => onSettingsChange({ bailianSummaryModel: event.target.value })}
          />
        </label>
        <label>
          {t("settings.bailianOssAccessKeyId")}
          <input
            value={settings.bailianOssAccessKeyId ?? ""}
            onChange={(event) => onSettingsChange({ bailianOssAccessKeyId: event.target.value })}
          />
        </label>
        <label>
          {t("settings.bailianOssAccessKeySecret")}
          <input
            type="password"
            value={settings.bailianOssAccessKeySecret ?? ""}
            onChange={(event) =>
              onSettingsChange({ bailianOssAccessKeySecret: event.target.value })
            }
          />
        </label>
        <label>
          {t("settings.bailianOssEndpoint")}
          <input
            value={settings.bailianOssEndpoint ?? ""}
            onChange={(event) => onSettingsChange({ bailianOssEndpoint: event.target.value })}
          />
        </label>
        <label>
          {t("settings.bailianOssBucket")}
          <input
            value={settings.bailianOssBucket ?? ""}
            onChange={(event) => onSettingsChange({ bailianOssBucket: event.target.value })}
          />
        </label>
        <label>
          {t("settings.bailianOssPathPrefix")}
          <input
            value={settings.bailianOssPathPrefix ?? ""}
            onChange={(event) => onSettingsChange({ bailianOssPathPrefix: event.target.value })}
          />
        </label>
        <label>
          {t("settings.bailianOssSignedUrlTtlSeconds")}
          <input
            type="number"
            min={60}
            max={86400}
            value={settings.bailianOssSignedUrlTtlSeconds}
            onChange={(event) =>
              onSettingsChange({
                bailianOssSignedUrlTtlSeconds: Number.parseInt(event.target.value || "0", 10) || 1800
              })
            }
          />
        </label>
        <p>{t("settings.bailianOssHint")}</p>
        {!isBailianSelected && <p>{t("settings.bailianUsedForSummary")}</p>}
      </div>

      <div className="settings-section">
        <h3>{t("settings.aliyunTingwu")}</h3>
        <label>
          {t("settings.aliyunAccessKeyId")}
          <input
            value={settings.aliyunAccessKeyId ?? ""}
            onChange={(event) => onSettingsChange({ aliyunAccessKeyId: event.target.value })}
          />
        </label>
        <label>
          {t("settings.aliyunAccessKeySecret")}
          <input
            type="password"
            value={settings.aliyunAccessKeySecret ?? ""}
            onChange={(event) => onSettingsChange({ aliyunAccessKeySecret: event.target.value })}
          />
        </label>
        <label>
          {t("settings.aliyunAppKey")}
          <input
            value={settings.aliyunAppKey ?? ""}
            onChange={(event) => onSettingsChange({ aliyunAppKey: event.target.value })}
          />
        </label>
        <label>
          {t("settings.aliyunEndpoint")}
          <input
            value={settings.aliyunEndpoint}
            onChange={(event) => onSettingsChange({ aliyunEndpoint: event.target.value })}
          />
        </label>
        <label>
          {t("settings.aliyunSourceLanguage")}
          <input
            value={settings.aliyunSourceLanguage}
            onChange={(event) => onSettingsChange({ aliyunSourceLanguage: event.target.value })}
          />
        </label>
        <label>
          {t("settings.aliyunFileUrlPrefix")}
          <input
            value={settings.aliyunFileUrlPrefix ?? ""}
            onChange={(event) => onSettingsChange({ aliyunFileUrlPrefix: event.target.value })}
          />
        </label>
        <p>{t("settings.aliyunFileUrlPrefixHint")}</p>
        {!isAliyunSelected && <p>{t("settings.aliyunOnlyForTranscription")}</p>}
      </div>

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

        <button type="button" onClick={addTemplate}>
          {t("settings.addTemplate")}
        </button>
      </div>

      <button type="button" onClick={onSave}>
        {t("settings.save")}
      </button>
    </section>
  );
}

export default SettingsTab;
