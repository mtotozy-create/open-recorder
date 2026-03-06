use serde::{Deserialize, Serialize};

const DEFAULT_BAILIAN_TRANSCRIPTION_PROVIDER_ID: &str = "bailian-transcription-default";
const DEFAULT_ALIYUN_TRANSCRIPTION_PROVIDER_ID: &str = "aliyun-transcription-default";
const DEFAULT_BAILIAN_SUMMARY_PROVIDER_ID: &str = "bailian-summary-default";
const DEFAULT_OPENROUTER_SUMMARY_PROVIDER_ID: &str = "openrouter-summary-default";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    Recording,
    Paused,
    Stopped,
    Transcribing,
    Summarizing,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RecordingQualityPreset {
    #[default]
    Standard,
    Hd,
    Hifi,
}

/// Legacy enum retained for backward-compatible settings migration.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TranscriptionProvider {
    #[default]
    Bailian,
    AliyunTingwu,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    Bailian,
    AliyunTingwu,
    Openrouter,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderCapability {
    Transcription,
    Summary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ProviderOssSettings {
    pub access_key_id: Option<String>,
    pub access_key_secret: Option<String>,
    pub endpoint: Option<String>,
    pub bucket: Option<String>,
    pub path_prefix: Option<String>,
    pub signed_url_ttl_seconds: u64,
}

impl Default for ProviderOssSettings {
    fn default() -> Self {
        Self {
            access_key_id: None,
            access_key_secret: None,
            endpoint: None,
            bucket: None,
            path_prefix: Some("open-recorder".to_string()),
            signed_url_ttl_seconds: 1800,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct BailianProviderSettings {
    pub api_key: Option<String>,
    pub base_url: String,
    pub transcription_model: String,
    pub summary_model: String,
    pub oss: ProviderOssSettings,
}

impl Default for BailianProviderSettings {
    fn default() -> Self {
        Self {
            api_key: None,
            base_url: "https://dashscope.aliyuncs.com".to_string(),
            transcription_model: "paraformer-v2".to_string(),
            summary_model: "qwen-plus".to_string(),
            oss: ProviderOssSettings::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AliyunTingwuProviderSettings {
    pub access_key_id: Option<String>,
    pub access_key_secret: Option<String>,
    pub app_key: Option<String>,
    pub endpoint: String,
    pub source_language: String,
    pub file_url_prefix: Option<String>,
    pub language_hints: Option<String>,
    pub transcription_normalization_enabled: bool,
    pub transcription_paragraph_enabled: bool,
    pub transcription_punctuation_prediction_enabled: bool,
    pub transcription_disfluency_removal_enabled: bool,
    pub transcription_speaker_diarization_enabled: bool,
    pub poll_interval_seconds: u64,
    pub max_polling_minutes: u64,
    pub oss: ProviderOssSettings,
}

impl Default for AliyunTingwuProviderSettings {
    fn default() -> Self {
        Self {
            access_key_id: None,
            access_key_secret: None,
            app_key: None,
            endpoint: "https://tingwu.cn-beijing.aliyuncs.com".to_string(),
            source_language: "cn".to_string(),
            file_url_prefix: None,
            language_hints: None,
            transcription_normalization_enabled: true,
            transcription_paragraph_enabled: true,
            transcription_punctuation_prediction_enabled: true,
            transcription_disfluency_removal_enabled: false,
            transcription_speaker_diarization_enabled: true,
            poll_interval_seconds: 60,
            max_polling_minutes: 180,
            oss: ProviderOssSettings::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct OpenrouterProviderSettings {
    pub api_key: Option<String>,
    pub base_url: String,
    pub summary_model: String,
}

impl Default for OpenrouterProviderSettings {
    fn default() -> Self {
        Self {
            api_key: None,
            base_url: "https://openrouter.ai/api/v1".to_string(),
            summary_model: "qwen/qwen-plus".to_string(),
        }
    }
}

fn default_enabled() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ProviderConfig {
    pub id: String,
    pub name: String,
    pub kind: ProviderKind,
    pub capabilities: Vec<ProviderCapability>,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    pub bailian: Option<BailianProviderSettings>,
    pub aliyun_tingwu: Option<AliyunTingwuProviderSettings>,
    pub openrouter: Option<OpenrouterProviderSettings>,
}

impl Default for ProviderConfig {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: "Bailian".to_string(),
            kind: ProviderKind::Bailian,
            capabilities: vec![ProviderCapability::Transcription, ProviderCapability::Summary],
            enabled: true,
            bailian: Some(BailianProviderSettings::default()),
            aliyun_tingwu: None,
            openrouter: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Queued,
    Running,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobKind {
    Transcription,
    Summary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptSegment {
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
    pub confidence: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SummaryResult {
    pub title: String,
    pub decisions: Vec<String>,
    pub action_items: Vec<String>,
    pub risks: Vec<String>,
    pub timeline: Vec<String>,
    pub raw_markdown: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptTemplate {
    pub id: String,
    pub name: String,
    pub system_prompt: String,
    pub user_prompt: String,
    pub variables: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AudioSegmentMeta {
    pub path: String,
    pub sequence: u32,
    pub started_at: String,
    pub ended_at: String,
    pub duration_ms: u64,
    pub sample_rate: u32,
    pub channels: u16,
    /// 分段文件格式："wav" 或 "m4a"
    pub format: String,
    pub file_size_bytes: u64,
}

impl Default for AudioSegmentMeta {
    fn default() -> Self {
        Self {
            path: String::new(),
            sequence: 0,
            started_at: String::new(),
            ended_at: String::new(),
            duration_ms: 0,
            sample_rate: 0,
            channels: 0,
            format: "wav".to_string(),
            file_size_bytes: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    pub providers: Vec<ProviderConfig>,
    pub selected_transcription_provider_id: String,
    pub selected_summary_provider_id: String,
    pub default_template_id: String,
    pub templates: Vec<PromptTemplate>,

    // Legacy fields (read-only for migration)
    #[serde(rename = "transcriptionProvider", default, skip_serializing)]
    pub legacy_transcription_provider: Option<TranscriptionProvider>,
    #[serde(rename = "bailianApiKey", default, skip_serializing)]
    pub legacy_bailian_api_key: Option<String>,
    #[serde(rename = "bailianBaseUrl", default, skip_serializing)]
    pub legacy_bailian_base_url: Option<String>,
    #[serde(rename = "bailianTranscriptionModel", default, skip_serializing)]
    pub legacy_bailian_transcription_model: Option<String>,
    #[serde(rename = "bailianSummaryModel", default, skip_serializing)]
    pub legacy_bailian_summary_model: Option<String>,
    #[serde(rename = "bailianOssAccessKeyId", default, skip_serializing)]
    pub legacy_bailian_oss_access_key_id: Option<String>,
    #[serde(rename = "bailianOssAccessKeySecret", default, skip_serializing)]
    pub legacy_bailian_oss_access_key_secret: Option<String>,
    #[serde(rename = "bailianOssEndpoint", default, skip_serializing)]
    pub legacy_bailian_oss_endpoint: Option<String>,
    #[serde(rename = "bailianOssBucket", default, skip_serializing)]
    pub legacy_bailian_oss_bucket: Option<String>,
    #[serde(rename = "bailianOssPathPrefix", default, skip_serializing)]
    pub legacy_bailian_oss_path_prefix: Option<String>,
    #[serde(rename = "bailianOssSignedUrlTtlSeconds", default, skip_serializing)]
    pub legacy_bailian_oss_signed_url_ttl_seconds: Option<u64>,
    #[serde(rename = "aliyunAccessKeyId", default, skip_serializing)]
    pub legacy_aliyun_access_key_id: Option<String>,
    #[serde(rename = "aliyunAccessKeySecret", default, skip_serializing)]
    pub legacy_aliyun_access_key_secret: Option<String>,
    #[serde(rename = "aliyunAppKey", default, skip_serializing)]
    pub legacy_aliyun_app_key: Option<String>,
    #[serde(rename = "aliyunEndpoint", default, skip_serializing)]
    pub legacy_aliyun_endpoint: Option<String>,
    #[serde(rename = "aliyunSourceLanguage", default, skip_serializing)]
    pub legacy_aliyun_source_language: Option<String>,
    #[serde(rename = "aliyunFileUrlPrefix", default, skip_serializing)]
    pub legacy_aliyun_file_url_prefix: Option<String>,
    #[serde(rename = "aliyunLanguageHints", default, skip_serializing)]
    pub legacy_aliyun_language_hints: Option<String>,
    #[serde(
        rename = "aliyunTranscriptionNormalizationEnabled",
        default,
        skip_serializing
    )]
    pub legacy_aliyun_transcription_normalization_enabled: Option<bool>,
    #[serde(rename = "aliyunTranscriptionParagraphEnabled", default, skip_serializing)]
    pub legacy_aliyun_transcription_paragraph_enabled: Option<bool>,
    #[serde(
        rename = "aliyunTranscriptionPunctuationPredictionEnabled",
        default,
        skip_serializing
    )]
    pub legacy_aliyun_transcription_punctuation_prediction_enabled: Option<bool>,
    #[serde(
        rename = "aliyunTranscriptionDisfluencyRemovalEnabled",
        default,
        skip_serializing
    )]
    pub legacy_aliyun_transcription_disfluency_removal_enabled: Option<bool>,
    #[serde(
        rename = "aliyunTranscriptionSpeakerDiarizationEnabled",
        default,
        skip_serializing
    )]
    pub legacy_aliyun_transcription_speaker_diarization_enabled: Option<bool>,
    #[serde(rename = "aliyunPollIntervalSeconds", default, skip_serializing)]
    pub legacy_aliyun_poll_interval_seconds: Option<u64>,
    #[serde(rename = "aliyunMaxPollingMinutes", default, skip_serializing)]
    pub legacy_aliyun_max_polling_minutes: Option<u64>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            providers: create_default_providers(),
            selected_transcription_provider_id: DEFAULT_BAILIAN_TRANSCRIPTION_PROVIDER_ID
                .to_string(),
            selected_summary_provider_id: DEFAULT_BAILIAN_SUMMARY_PROVIDER_ID.to_string(),
            default_template_id: "meeting-default".to_string(),
            templates: vec![create_default_template()],

            legacy_transcription_provider: None,
            legacy_bailian_api_key: None,
            legacy_bailian_base_url: None,
            legacy_bailian_transcription_model: None,
            legacy_bailian_summary_model: None,
            legacy_bailian_oss_access_key_id: None,
            legacy_bailian_oss_access_key_secret: None,
            legacy_bailian_oss_endpoint: None,
            legacy_bailian_oss_bucket: None,
            legacy_bailian_oss_path_prefix: None,
            legacy_bailian_oss_signed_url_ttl_seconds: None,
            legacy_aliyun_access_key_id: None,
            legacy_aliyun_access_key_secret: None,
            legacy_aliyun_app_key: None,
            legacy_aliyun_endpoint: None,
            legacy_aliyun_source_language: None,
            legacy_aliyun_file_url_prefix: None,
            legacy_aliyun_language_hints: None,
            legacy_aliyun_transcription_normalization_enabled: None,
            legacy_aliyun_transcription_paragraph_enabled: None,
            legacy_aliyun_transcription_punctuation_prediction_enabled: None,
            legacy_aliyun_transcription_disfluency_removal_enabled: None,
            legacy_aliyun_transcription_speaker_diarization_enabled: None,
            legacy_aliyun_poll_interval_seconds: None,
            legacy_aliyun_max_polling_minutes: None,
        }
    }
}

impl Settings {
    pub fn normalize(&mut self) {
        if self.providers.is_empty() {
            self.providers = self.migrate_legacy_providers();
            self.selected_transcription_provider_id = match self.legacy_transcription_provider {
                Some(TranscriptionProvider::AliyunTingwu) => {
                    DEFAULT_ALIYUN_TRANSCRIPTION_PROVIDER_ID.to_string()
                }
                _ => DEFAULT_BAILIAN_TRANSCRIPTION_PROVIDER_ID.to_string(),
            };
            self.selected_summary_provider_id = DEFAULT_BAILIAN_SUMMARY_PROVIDER_ID.to_string();
        }

        for provider in &mut self.providers {
            normalize_provider(provider);
        }

        if self.templates.is_empty() {
            self.templates.push(create_default_template());
        }

        let default_exists = self
            .templates
            .iter()
            .any(|template| template.id == self.default_template_id);
        if !default_exists {
            self.default_template_id = self.templates[0].id.clone();
        }

        ensure_capability_provider(&mut self.providers, ProviderCapability::Transcription);
        ensure_capability_provider(&mut self.providers, ProviderCapability::Summary);

        self.selected_transcription_provider_id = resolve_selected_provider_id(
            &self.providers,
            &self.selected_transcription_provider_id,
            ProviderCapability::Transcription,
        );
        self.selected_summary_provider_id = resolve_selected_provider_id(
            &self.providers,
            &self.selected_summary_provider_id,
            ProviderCapability::Summary,
        );
    }

    fn migrate_legacy_providers(&self) -> Vec<ProviderConfig> {
        let default_bailian = BailianProviderSettings::default();
        let default_aliyun = AliyunTingwuProviderSettings::default();

        let legacy_oss = ProviderOssSettings {
            access_key_id: self.legacy_bailian_oss_access_key_id.clone(),
            access_key_secret: self.legacy_bailian_oss_access_key_secret.clone(),
            endpoint: self.legacy_bailian_oss_endpoint.clone(),
            bucket: self.legacy_bailian_oss_bucket.clone(),
            path_prefix: self
                .legacy_bailian_oss_path_prefix
                .clone()
                .or(default_bailian.oss.path_prefix.clone()),
            signed_url_ttl_seconds: self
                .legacy_bailian_oss_signed_url_ttl_seconds
                .unwrap_or(default_bailian.oss.signed_url_ttl_seconds),
        };

        vec![
            ProviderConfig {
                id: DEFAULT_BAILIAN_TRANSCRIPTION_PROVIDER_ID.to_string(),
                name: "Bailian Transcription".to_string(),
                kind: ProviderKind::Bailian,
                capabilities: vec![ProviderCapability::Transcription],
                enabled: true,
                bailian: Some(BailianProviderSettings {
                    api_key: self.legacy_bailian_api_key.clone(),
                    base_url: self
                        .legacy_bailian_base_url
                        .clone()
                        .unwrap_or(default_bailian.base_url.clone()),
                    transcription_model: self
                        .legacy_bailian_transcription_model
                        .clone()
                        .unwrap_or(default_bailian.transcription_model.clone()),
                    summary_model: self
                        .legacy_bailian_summary_model
                        .clone()
                        .unwrap_or(default_bailian.summary_model.clone()),
                    oss: legacy_oss.clone(),
                }),
                aliyun_tingwu: None,
                openrouter: None,
            },
            ProviderConfig {
                id: DEFAULT_ALIYUN_TRANSCRIPTION_PROVIDER_ID.to_string(),
                name: "Aliyun Tingwu Transcription".to_string(),
                kind: ProviderKind::AliyunTingwu,
                capabilities: vec![ProviderCapability::Transcription],
                enabled: true,
                bailian: None,
                aliyun_tingwu: Some(AliyunTingwuProviderSettings {
                    access_key_id: self.legacy_aliyun_access_key_id.clone(),
                    access_key_secret: self.legacy_aliyun_access_key_secret.clone(),
                    app_key: self.legacy_aliyun_app_key.clone(),
                    endpoint: self
                        .legacy_aliyun_endpoint
                        .clone()
                        .unwrap_or(default_aliyun.endpoint.clone()),
                    source_language: self
                        .legacy_aliyun_source_language
                        .clone()
                        .unwrap_or(default_aliyun.source_language.clone()),
                    file_url_prefix: self.legacy_aliyun_file_url_prefix.clone(),
                    language_hints: self.legacy_aliyun_language_hints.clone(),
                    transcription_normalization_enabled: self
                        .legacy_aliyun_transcription_normalization_enabled
                        .unwrap_or(default_aliyun.transcription_normalization_enabled),
                    transcription_paragraph_enabled: self
                        .legacy_aliyun_transcription_paragraph_enabled
                        .unwrap_or(default_aliyun.transcription_paragraph_enabled),
                    transcription_punctuation_prediction_enabled: self
                        .legacy_aliyun_transcription_punctuation_prediction_enabled
                        .unwrap_or(default_aliyun.transcription_punctuation_prediction_enabled),
                    transcription_disfluency_removal_enabled: self
                        .legacy_aliyun_transcription_disfluency_removal_enabled
                        .unwrap_or(default_aliyun.transcription_disfluency_removal_enabled),
                    transcription_speaker_diarization_enabled: self
                        .legacy_aliyun_transcription_speaker_diarization_enabled
                        .unwrap_or(default_aliyun.transcription_speaker_diarization_enabled),
                    poll_interval_seconds: self
                        .legacy_aliyun_poll_interval_seconds
                        .unwrap_or(default_aliyun.poll_interval_seconds),
                    max_polling_minutes: self
                        .legacy_aliyun_max_polling_minutes
                        .unwrap_or(default_aliyun.max_polling_minutes),
                    oss: legacy_oss.clone(),
                }),
                openrouter: None,
            },
            ProviderConfig {
                id: DEFAULT_BAILIAN_SUMMARY_PROVIDER_ID.to_string(),
                name: "Bailian Summary".to_string(),
                kind: ProviderKind::Bailian,
                capabilities: vec![ProviderCapability::Summary],
                enabled: true,
                bailian: Some(BailianProviderSettings {
                    api_key: self.legacy_bailian_api_key.clone(),
                    base_url: self
                        .legacy_bailian_base_url
                        .clone()
                        .unwrap_or(default_bailian.base_url.clone()),
                    transcription_model: self
                        .legacy_bailian_transcription_model
                        .clone()
                        .unwrap_or(default_bailian.transcription_model.clone()),
                    summary_model: self
                        .legacy_bailian_summary_model
                        .clone()
                        .unwrap_or(default_bailian.summary_model.clone()),
                    oss: legacy_oss,
                }),
                aliyun_tingwu: None,
                openrouter: None,
            },
            ProviderConfig {
                id: DEFAULT_OPENROUTER_SUMMARY_PROVIDER_ID.to_string(),
                name: "OpenRouter Summary".to_string(),
                kind: ProviderKind::Openrouter,
                capabilities: vec![ProviderCapability::Summary],
                enabled: true,
                bailian: None,
                aliyun_tingwu: None,
                openrouter: Some(OpenrouterProviderSettings::default()),
            },
        ]
    }
}

fn create_default_template() -> PromptTemplate {
    PromptTemplate {
        id: "meeting-default".to_string(),
        name: "Meeting Default".to_string(),
        system_prompt: "You are an assistant for writing concise meeting notes.".to_string(),
        user_prompt: "Organize transcript into: conclusion, action items, risks, timeline."
            .to_string(),
        variables: vec!["language".to_string(), "audience".to_string()],
    }
}

fn create_default_providers() -> Vec<ProviderConfig> {
    vec![
        ProviderConfig {
            id: DEFAULT_BAILIAN_TRANSCRIPTION_PROVIDER_ID.to_string(),
            name: "Bailian Transcription".to_string(),
            kind: ProviderKind::Bailian,
            capabilities: vec![ProviderCapability::Transcription],
            enabled: true,
            bailian: Some(BailianProviderSettings::default()),
            aliyun_tingwu: None,
            openrouter: None,
        },
        ProviderConfig {
            id: DEFAULT_ALIYUN_TRANSCRIPTION_PROVIDER_ID.to_string(),
            name: "Aliyun Tingwu Transcription".to_string(),
            kind: ProviderKind::AliyunTingwu,
            capabilities: vec![ProviderCapability::Transcription],
            enabled: true,
            bailian: None,
            aliyun_tingwu: Some(AliyunTingwuProviderSettings::default()),
            openrouter: None,
        },
        ProviderConfig {
            id: DEFAULT_BAILIAN_SUMMARY_PROVIDER_ID.to_string(),
            name: "Bailian Summary".to_string(),
            kind: ProviderKind::Bailian,
            capabilities: vec![ProviderCapability::Summary],
            enabled: true,
            bailian: Some(BailianProviderSettings::default()),
            aliyun_tingwu: None,
            openrouter: None,
        },
        ProviderConfig {
            id: DEFAULT_OPENROUTER_SUMMARY_PROVIDER_ID.to_string(),
            name: "OpenRouter Summary".to_string(),
            kind: ProviderKind::Openrouter,
            capabilities: vec![ProviderCapability::Summary],
            enabled: true,
            bailian: None,
            aliyun_tingwu: None,
            openrouter: Some(OpenrouterProviderSettings::default()),
        },
    ]
}

fn normalize_provider(provider: &mut ProviderConfig) {
    if provider.id.trim().is_empty() {
        provider.id = format!("provider-{}", provider.kind_name());
    }

    if provider.name.trim().is_empty() {
        provider.name = provider.kind_name().to_string();
    }

    if provider.capabilities.is_empty() {
        provider.capabilities = default_capabilities_for_kind(&provider.kind);
    }

    provider.bailian = match provider.kind {
        ProviderKind::Bailian => {
            let mut config = provider.bailian.clone().unwrap_or_default();
            config.oss.signed_url_ttl_seconds = config.oss.signed_url_ttl_seconds.clamp(60, 86_400);
            Some(config)
        }
        _ => None,
    };

    provider.aliyun_tingwu = match provider.kind {
        ProviderKind::AliyunTingwu => {
            let mut config = provider.aliyun_tingwu.clone().unwrap_or_default();
            config.poll_interval_seconds = config.poll_interval_seconds.clamp(60, 300);
            config.max_polling_minutes = config.max_polling_minutes.clamp(5, 720);
            config.oss.signed_url_ttl_seconds = config.oss.signed_url_ttl_seconds.clamp(60, 86_400);
            Some(config)
        }
        _ => None,
    };

    provider.openrouter = match provider.kind {
        ProviderKind::Openrouter => Some(provider.openrouter.clone().unwrap_or_default()),
        _ => None,
    };
}

fn default_capabilities_for_kind(kind: &ProviderKind) -> Vec<ProviderCapability> {
    match kind {
        ProviderKind::Bailian => vec![ProviderCapability::Transcription, ProviderCapability::Summary],
        ProviderKind::AliyunTingwu => vec![ProviderCapability::Transcription],
        ProviderKind::Openrouter => vec![ProviderCapability::Summary],
    }
}

fn provider_supports_capability(provider: &ProviderConfig, capability: ProviderCapability) -> bool {
    provider.enabled && provider.capabilities.iter().any(|item| *item == capability)
}

fn resolve_selected_provider_id(
    providers: &[ProviderConfig],
    current: &str,
    capability: ProviderCapability,
) -> String {
    let current_exists = providers
        .iter()
        .any(|provider| provider.id == current && provider_supports_capability(provider, capability.clone()));
    if current_exists {
        return current.to_string();
    }

    providers
        .iter()
        .find(|provider| provider_supports_capability(provider, capability.clone()))
        .map(|provider| provider.id.clone())
        .unwrap_or_default()
}

fn ensure_capability_provider(providers: &mut Vec<ProviderConfig>, capability: ProviderCapability) {
    if providers
        .iter()
        .any(|provider| provider_supports_capability(provider, capability.clone()))
    {
        return;
    }

    let fallback = match capability {
        ProviderCapability::Transcription => ProviderConfig {
            id: "bailian-transcription-auto".to_string(),
            name: "Bailian Transcription Auto".to_string(),
            kind: ProviderKind::Bailian,
            capabilities: vec![ProviderCapability::Transcription],
            enabled: true,
            bailian: Some(BailianProviderSettings::default()),
            aliyun_tingwu: None,
            openrouter: None,
        },
        ProviderCapability::Summary => ProviderConfig {
            id: "openrouter-summary-auto".to_string(),
            name: "OpenRouter Summary Auto".to_string(),
            kind: ProviderKind::Openrouter,
            capabilities: vec![ProviderCapability::Summary],
            enabled: true,
            bailian: None,
            aliyun_tingwu: None,
            openrouter: Some(OpenrouterProviderSettings::default()),
        },
    };

    providers.push(fallback);
}

impl ProviderConfig {
    fn kind_name(&self) -> &'static str {
        match self.kind {
            ProviderKind::Bailian => "bailian",
            ProviderKind::AliyunTingwu => "aliyun_tingwu",
            ProviderKind::Openrouter => "openrouter",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Session {
    pub id: String,
    pub name: Option<String>,
    pub status: SessionStatus,
    pub created_at: String,
    pub updated_at: String,
    pub input_device_id: Option<String>,
    pub audio_segments: Vec<String>,
    pub audio_segment_meta: Vec<AudioSegmentMeta>,
    pub quality_preset: RecordingQualityPreset,
    pub sample_rate: u32,
    pub channels: u16,
    pub elapsed_ms: u64,
    pub exported_wav_path: Option<String>,
    pub exported_wav_size: Option<u64>,
    pub exported_wav_created_at: Option<String>,
    pub exported_mp3_path: Option<String>,
    pub exported_mp3_size: Option<u64>,
    pub exported_mp3_created_at: Option<String>,
    pub exported_m4a_path: Option<String>,
    pub exported_m4a_size: Option<u64>,
    pub exported_m4a_created_at: Option<String>,
    pub transcript: Vec<TranscriptSegment>,
    pub summary: Option<SummaryResult>,
}

impl Default for Session {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: None,
            status: SessionStatus::Stopped,
            created_at: String::new(),
            updated_at: String::new(),
            input_device_id: None,
            audio_segments: vec![],
            audio_segment_meta: vec![],
            quality_preset: RecordingQualityPreset::Standard,
            sample_rate: 0,
            channels: 0,
            elapsed_ms: 0,
            exported_wav_path: None,
            exported_wav_size: None,
            exported_wav_created_at: None,
            exported_mp3_path: None,
            exported_mp3_size: None,
            exported_mp3_created_at: None,
            exported_m4a_path: None,
            exported_m4a_size: None,
            exported_m4a_created_at: None,
            transcript: vec![],
            summary: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSummary {
    pub id: String,
    pub name: Option<String>,
    pub status: SessionStatus,
    pub created_at: String,
    pub updated_at: String,
    pub elapsed_ms: u64,
    pub quality_preset: RecordingQualityPreset,
}

impl From<&Session> for SessionSummary {
    fn from(value: &Session) -> Self {
        Self {
            id: value.id.clone(),
            name: value.name.clone(),
            status: value.status.clone(),
            created_at: value.created_at.clone(),
            updated_at: value.updated_at.clone(),
            elapsed_ms: value.elapsed_ms,
            quality_preset: value.quality_preset.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Job {
    pub id: String,
    pub session_id: String,
    pub kind: JobKind,
    pub status: JobStatus,
    pub created_at: String,
    pub updated_at: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartSessionResponse {
    pub session_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecorderStatus {
    pub session_id: String,
    pub elapsed_ms: u64,
    pub segment_count: usize,
    pub quality_preset: RecordingQualityPreset,
    pub rms: f32,
    pub peak: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecorderExportResponse {
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobEnqueueResponse {
    pub job_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsPatch {
    pub providers: Option<Vec<ProviderConfig>>,
    pub selected_transcription_provider_id: Option<String>,
    pub selected_summary_provider_id: Option<String>,
    pub default_template_id: Option<String>,
    pub templates: Option<Vec<PromptTemplate>>,
}
