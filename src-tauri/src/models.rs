use serde::{Deserialize, Serialize};

const DEFAULT_BAILIAN_PROVIDER_ID: &str = "bailian-default";
const DEFAULT_ALIYUN_PROVIDER_ID: &str = "aliyun-tingwu-default";
const DEFAULT_OPENROUTER_PROVIDER_ID: &str = "openrouter-default";
const DEFAULT_LOCAL_STT_PROVIDER_ID: &str = "local-stt-default";
const DEFAULT_SELECTED_OSS_CONFIG_ID: &str = "oss-aliyun-default";
const DEFAULT_R2_OSS_CONFIG_ID: &str = "oss-r2-default";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    Recording,
    Paused,
    Processing,
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
    LocalStt,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderCapability {
    Transcription,
    Summary,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OssProviderKind {
    Aliyun,
    R2,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct OssConfig {
    pub id: String,
    pub name: String,
    pub kind: OssProviderKind,
    pub access_key_id: Option<String>,
    pub access_key_secret: Option<String>,
    pub endpoint: Option<String>,
    pub bucket: Option<String>,
    pub path_prefix: Option<String>,
    pub signed_url_ttl_seconds: u64,
}

impl Default for OssConfig {
    fn default() -> Self {
        Self {
            id: DEFAULT_SELECTED_OSS_CONFIG_ID.to_string(),
            name: "Aliyun OSS".to_string(),
            kind: OssProviderKind::Aliyun,
            access_key_id: None,
            access_key_secret: None,
            endpoint: None,
            bucket: None,
            path_prefix: Some("open-recorder".to_string()),
            signed_url_ttl_seconds: 1800,
        }
    }
}

/// Legacy single-OSS schema retained for backward-compatible settings migration.
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
    #[serde(rename = "oss", default, skip_serializing)]
    pub legacy_oss: Option<ProviderOssSettings>,
}

impl Default for BailianProviderSettings {
    fn default() -> Self {
        Self {
            api_key: None,
            base_url: "https://dashscope.aliyuncs.com".to_string(),
            transcription_model: "paraformer-v2".to_string(),
            summary_model: "qwen-plus".to_string(),
            legacy_oss: None,
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
    #[serde(rename = "oss", default, skip_serializing)]
    pub legacy_oss: Option<ProviderOssSettings>,
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
            legacy_oss: None,
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

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum LocalSttEngine {
    #[default]
    Whisper,
    SensevoiceSmall,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct LocalSttProviderSettings {
    pub python_path: Option<String>,
    pub venv_dir: Option<String>,
    pub model_cache_dir: Option<String>,
    pub engine: LocalSttEngine,
    pub whisper_model: String,
    pub sense_voice_model: String,
    pub language: String,
    pub diarization_enabled: bool,
    pub min_speakers: Option<u32>,
    pub max_speakers: Option<u32>,
    pub speaker_count_hint: Option<u32>,
    pub compute_device: String,
    pub vad_enabled: bool,
    pub chunk_seconds: u64,
}

impl Default for LocalSttProviderSettings {
    fn default() -> Self {
        Self {
            python_path: None,
            venv_dir: None,
            model_cache_dir: None,
            engine: LocalSttEngine::Whisper,
            whisper_model: "small".to_string(),
            sense_voice_model: "iic/SenseVoiceSmall".to_string(),
            language: "auto".to_string(),
            diarization_enabled: true,
            min_speakers: None,
            max_speakers: None,
            speaker_count_hint: None,
            compute_device: "auto".to_string(),
            vad_enabled: true,
            chunk_seconds: 30,
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
    pub local_stt: Option<LocalSttProviderSettings>,
}

impl Default for ProviderConfig {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: "Bailian".to_string(),
            kind: ProviderKind::Bailian,
            capabilities: vec![
                ProviderCapability::Transcription,
                ProviderCapability::Summary,
            ],
            enabled: true,
            bailian: Some(BailianProviderSettings::default()),
            aliyun_tingwu: None,
            openrouter: None,
            local_stt: None,
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
    pub speaker_id: Option<String>,
    pub speaker_label: Option<String>,
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
    pub oss_configs: Vec<OssConfig>,
    pub selected_oss_config_id: String,
    pub selected_transcription_provider_id: String,
    pub selected_summary_provider_id: String,
    pub default_template_id: String,
    pub templates: Vec<PromptTemplate>,

    #[serde(rename = "oss", default, skip_serializing)]
    pub legacy_oss: Option<ProviderOssSettings>,

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
    #[serde(
        rename = "aliyunTranscriptionParagraphEnabled",
        default,
        skip_serializing
    )]
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
            oss_configs: create_default_oss_configs(),
            selected_oss_config_id: DEFAULT_SELECTED_OSS_CONFIG_ID.to_string(),
            selected_transcription_provider_id: DEFAULT_BAILIAN_PROVIDER_ID.to_string(),
            selected_summary_provider_id: DEFAULT_BAILIAN_PROVIDER_ID.to_string(),
            default_template_id: "meeting-default".to_string(),
            templates: vec![create_default_template()],

            legacy_oss: None,

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
            let (providers, legacy_oss) = self.migrate_legacy_providers();
            self.providers = providers;
            self.selected_transcription_provider_id = match self.legacy_transcription_provider {
                Some(TranscriptionProvider::AliyunTingwu) => DEFAULT_ALIYUN_PROVIDER_ID.to_string(),
                _ => DEFAULT_BAILIAN_PROVIDER_ID.to_string(),
            };
            self.selected_summary_provider_id = DEFAULT_BAILIAN_PROVIDER_ID.to_string();
            if self.legacy_oss.is_none() {
                self.legacy_oss = Some(legacy_oss);
            }
        }

        self.providers = canonicalize_providers_by_kind(&self.providers);
        for provider in &mut self.providers {
            normalize_provider(provider);
        }

        if self.oss_configs.is_empty() {
            if let Some(legacy_oss) = self
                .legacy_oss
                .clone()
                .filter(legacy_oss_has_user_value)
                .or_else(|| find_legacy_provider_oss(&self.providers))
            {
                self.oss_configs.push(legacy_oss_to_config(
                    &legacy_oss,
                    DEFAULT_SELECTED_OSS_CONFIG_ID,
                    "Aliyun OSS",
                ));
            }
        }
        if self.oss_configs.is_empty() {
            self.oss_configs = create_default_oss_configs();
        }
        for config in &mut self.oss_configs {
            normalize_oss_config(config);
        }
        ensure_default_oss_kinds(&mut self.oss_configs);
        self.selected_oss_config_id =
            resolve_selected_oss_config_id(&self.oss_configs, &self.selected_oss_config_id);

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

        self.selected_transcription_provider_id =
            normalize_provider_id_alias(self.selected_transcription_provider_id.as_str());
        self.selected_summary_provider_id =
            normalize_provider_id_alias(self.selected_summary_provider_id.as_str());

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

    fn migrate_legacy_providers(&self) -> (Vec<ProviderConfig>, ProviderOssSettings) {
        let default_bailian = BailianProviderSettings::default();
        let default_aliyun = AliyunTingwuProviderSettings::default();
        let default_oss = ProviderOssSettings::default();
        let default_openrouter = OpenrouterProviderSettings::default();

        let legacy_oss = ProviderOssSettings {
            access_key_id: self.legacy_bailian_oss_access_key_id.clone(),
            access_key_secret: self.legacy_bailian_oss_access_key_secret.clone(),
            endpoint: self.legacy_bailian_oss_endpoint.clone(),
            bucket: self.legacy_bailian_oss_bucket.clone(),
            path_prefix: self
                .legacy_bailian_oss_path_prefix
                .clone()
                .or(default_oss.path_prefix.clone()),
            signed_url_ttl_seconds: self
                .legacy_bailian_oss_signed_url_ttl_seconds
                .unwrap_or(default_oss.signed_url_ttl_seconds),
        };

        let bailian_provider = ProviderConfig {
            id: DEFAULT_BAILIAN_PROVIDER_ID.to_string(),
            name: "Bailian".to_string(),
            kind: ProviderKind::Bailian,
            capabilities: vec![
                ProviderCapability::Transcription,
                ProviderCapability::Summary,
            ],
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
                legacy_oss: None,
            }),
            aliyun_tingwu: None,
            openrouter: None,
            local_stt: None,
        };
        let aliyun_provider = ProviderConfig {
            id: DEFAULT_ALIYUN_PROVIDER_ID.to_string(),
            name: "Aliyun Tingwu".to_string(),
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
                legacy_oss: None,
            }),
            openrouter: None,
            local_stt: None,
        };
        let openrouter_provider = ProviderConfig {
            id: DEFAULT_OPENROUTER_PROVIDER_ID.to_string(),
            name: "OpenRouter".to_string(),
            kind: ProviderKind::Openrouter,
            capabilities: vec![ProviderCapability::Summary],
            enabled: true,
            bailian: None,
            aliyun_tingwu: None,
            openrouter: Some(OpenrouterProviderSettings {
                api_key: None,
                base_url: default_openrouter.base_url,
                summary_model: default_openrouter.summary_model,
            }),
            local_stt: None,
        };
        let local_stt_provider = ProviderConfig {
            id: DEFAULT_LOCAL_STT_PROVIDER_ID.to_string(),
            name: "Local STT".to_string(),
            kind: ProviderKind::LocalStt,
            capabilities: vec![ProviderCapability::Transcription],
            enabled: true,
            bailian: None,
            aliyun_tingwu: None,
            openrouter: None,
            local_stt: Some(LocalSttProviderSettings::default()),
        };

        (
            vec![
                bailian_provider,
                aliyun_provider,
                openrouter_provider,
                local_stt_provider,
            ],
            legacy_oss,
        )
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

fn create_default_oss_configs() -> Vec<OssConfig> {
    vec![
        create_default_aliyun_oss_config(),
        create_default_r2_oss_config(),
    ]
}

fn create_default_aliyun_oss_config() -> OssConfig {
    OssConfig::default()
}

fn create_default_r2_oss_config() -> OssConfig {
    OssConfig {
        id: DEFAULT_R2_OSS_CONFIG_ID.to_string(),
        name: "Cloudflare R2".to_string(),
        kind: OssProviderKind::R2,
        access_key_id: None,
        access_key_secret: None,
        endpoint: None,
        bucket: None,
        path_prefix: Some("open-recorder".to_string()),
        signed_url_ttl_seconds: 1800,
    }
}

fn create_default_providers() -> Vec<ProviderConfig> {
    vec![
        ProviderConfig {
            id: DEFAULT_BAILIAN_PROVIDER_ID.to_string(),
            name: "Bailian".to_string(),
            kind: ProviderKind::Bailian,
            capabilities: vec![
                ProviderCapability::Transcription,
                ProviderCapability::Summary,
            ],
            enabled: true,
            bailian: Some(BailianProviderSettings::default()),
            aliyun_tingwu: None,
            openrouter: None,
            local_stt: None,
        },
        ProviderConfig {
            id: DEFAULT_ALIYUN_PROVIDER_ID.to_string(),
            name: "Aliyun Tingwu".to_string(),
            kind: ProviderKind::AliyunTingwu,
            capabilities: vec![ProviderCapability::Transcription],
            enabled: true,
            bailian: None,
            aliyun_tingwu: Some(AliyunTingwuProviderSettings::default()),
            openrouter: None,
            local_stt: None,
        },
        ProviderConfig {
            id: DEFAULT_OPENROUTER_PROVIDER_ID.to_string(),
            name: "OpenRouter".to_string(),
            kind: ProviderKind::Openrouter,
            capabilities: vec![ProviderCapability::Summary],
            enabled: true,
            bailian: None,
            aliyun_tingwu: None,
            openrouter: Some(OpenrouterProviderSettings::default()),
            local_stt: None,
        },
        ProviderConfig {
            id: DEFAULT_LOCAL_STT_PROVIDER_ID.to_string(),
            name: "Local STT".to_string(),
            kind: ProviderKind::LocalStt,
            capabilities: vec![ProviderCapability::Transcription],
            enabled: true,
            bailian: None,
            aliyun_tingwu: None,
            openrouter: None,
            local_stt: Some(LocalSttProviderSettings::default()),
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
            config.legacy_oss = None;
            Some(config)
        }
        _ => None,
    };

    provider.aliyun_tingwu = match provider.kind {
        ProviderKind::AliyunTingwu => {
            let mut config = provider.aliyun_tingwu.clone().unwrap_or_default();
            config.poll_interval_seconds = config.poll_interval_seconds.clamp(60, 300);
            config.max_polling_minutes = config.max_polling_minutes.clamp(5, 720);
            config.legacy_oss = None;
            Some(config)
        }
        _ => None,
    };

    provider.openrouter = match provider.kind {
        ProviderKind::Openrouter => Some(provider.openrouter.clone().unwrap_or_default()),
        _ => None,
    };

    provider.local_stt = match provider.kind {
        ProviderKind::LocalStt => {
            let mut config = provider.local_stt.clone().unwrap_or_default();
            config.chunk_seconds = config.chunk_seconds.clamp(5, 180);
            config.min_speakers = config.min_speakers.map(|value| value.clamp(1, 16));
            config.max_speakers = config.max_speakers.map(|value| value.clamp(1, 16));
            config.speaker_count_hint = config.speaker_count_hint.map(|value| value.clamp(1, 16));
            Some(config)
        }
        _ => None,
    };
}

fn default_capabilities_for_kind(kind: &ProviderKind) -> Vec<ProviderCapability> {
    match kind {
        ProviderKind::Bailian => vec![
            ProviderCapability::Transcription,
            ProviderCapability::Summary,
        ],
        ProviderKind::AliyunTingwu => vec![ProviderCapability::Transcription],
        ProviderKind::Openrouter => vec![ProviderCapability::Summary],
        ProviderKind::LocalStt => vec![ProviderCapability::Transcription],
    }
}

fn normalize_provider_id_alias(value: &str) -> String {
    match value.trim() {
        "bailian-transcription-default" | "bailian-summary-default" => {
            DEFAULT_BAILIAN_PROVIDER_ID.to_string()
        }
        "aliyun-transcription-default" => DEFAULT_ALIYUN_PROVIDER_ID.to_string(),
        "openrouter-summary-default" => DEFAULT_OPENROUTER_PROVIDER_ID.to_string(),
        other => other.to_string(),
    }
}

fn canonicalize_providers_by_kind(providers: &[ProviderConfig]) -> Vec<ProviderConfig> {
    let defaults = create_default_providers();
    let ordered_kinds = [
        ProviderKind::Bailian,
        ProviderKind::AliyunTingwu,
        ProviderKind::Openrouter,
        ProviderKind::LocalStt,
    ];

    ordered_kinds
        .iter()
        .map(|kind| {
            let mut provider = defaults
                .iter()
                .find(|item| &item.kind == kind)
                .cloned()
                .unwrap_or_default();

            let candidates: Vec<&ProviderConfig> =
                providers.iter().filter(|item| &item.kind == kind).collect();
            if candidates.is_empty() {
                return provider;
            }

            provider.enabled = candidates.iter().any(|item| item.enabled);
            if let Some(name) = candidates
                .iter()
                .map(|item| item.name.trim())
                .find(|value| !value.is_empty())
            {
                provider.name = name.to_string();
            }

            match kind {
                ProviderKind::Bailian => {
                    if let Some(config) = candidates
                        .iter()
                        .find_map(|item| item.bailian.as_ref())
                        .cloned()
                    {
                        provider.bailian = Some(config);
                    }
                }
                ProviderKind::AliyunTingwu => {
                    if let Some(config) = candidates
                        .iter()
                        .find_map(|item| item.aliyun_tingwu.as_ref())
                        .cloned()
                    {
                        provider.aliyun_tingwu = Some(config);
                    }
                }
                ProviderKind::Openrouter => {
                    if let Some(config) = candidates
                        .iter()
                        .find_map(|item| item.openrouter.as_ref())
                        .cloned()
                    {
                        provider.openrouter = Some(config);
                    }
                }
                ProviderKind::LocalStt => {
                    if let Some(config) = candidates
                        .iter()
                        .find_map(|item| item.local_stt.as_ref())
                        .cloned()
                    {
                        provider.local_stt = Some(config);
                    }
                }
            }

            provider
        })
        .collect()
}

fn provider_supports_capability(provider: &ProviderConfig, capability: ProviderCapability) -> bool {
    provider.enabled && provider.capabilities.iter().any(|item| *item == capability)
}

fn resolve_selected_provider_id(
    providers: &[ProviderConfig],
    current: &str,
    capability: ProviderCapability,
) -> String {
    let current_exists = providers.iter().any(|provider| {
        provider.id == current && provider_supports_capability(provider, capability.clone())
    });
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
            id: DEFAULT_BAILIAN_PROVIDER_ID.to_string(),
            name: "Bailian".to_string(),
            kind: ProviderKind::Bailian,
            capabilities: vec![
                ProviderCapability::Transcription,
                ProviderCapability::Summary,
            ],
            enabled: true,
            bailian: Some(BailianProviderSettings::default()),
            aliyun_tingwu: None,
            openrouter: None,
            local_stt: None,
        },
        ProviderCapability::Summary => ProviderConfig {
            id: DEFAULT_BAILIAN_PROVIDER_ID.to_string(),
            name: "Bailian".to_string(),
            kind: ProviderKind::Bailian,
            capabilities: vec![ProviderCapability::Summary],
            enabled: true,
            bailian: Some(BailianProviderSettings::default()),
            aliyun_tingwu: None,
            openrouter: None,
            local_stt: None,
        },
    };

    providers.push(fallback);
}

fn oss_config_has_user_value(oss: &OssConfig) -> bool {
    let default_oss = OssConfig::default();
    oss.access_key_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .is_some()
        || oss
            .access_key_secret
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .is_some()
        || oss
            .endpoint
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .is_some()
        || oss
            .bucket
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .is_some()
        || oss.path_prefix.as_deref().map(str::trim)
            != default_oss.path_prefix.as_deref().map(str::trim)
        || oss.signed_url_ttl_seconds != default_oss.signed_url_ttl_seconds
}

fn legacy_oss_has_user_value(oss: &ProviderOssSettings) -> bool {
    let default_oss = ProviderOssSettings::default();
    oss.access_key_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .is_some()
        || oss
            .access_key_secret
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .is_some()
        || oss
            .endpoint
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .is_some()
        || oss
            .bucket
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .is_some()
        || oss.path_prefix.as_deref().map(str::trim)
            != default_oss.path_prefix.as_deref().map(str::trim)
        || oss.signed_url_ttl_seconds != default_oss.signed_url_ttl_seconds
}

fn normalize_oss_config(oss: &mut OssConfig) {
    if oss.id.trim().is_empty() {
        oss.id = format!("oss-{}", oss.kind_name());
    }
    if oss.name.trim().is_empty() {
        oss.name = match oss.kind {
            OssProviderKind::Aliyun => "Aliyun OSS".to_string(),
            OssProviderKind::R2 => "Cloudflare R2".to_string(),
        };
    }
    if oss.path_prefix.is_none() {
        oss.path_prefix = OssConfig::default().path_prefix;
    }
    oss.signed_url_ttl_seconds = oss.signed_url_ttl_seconds.clamp(60, 86_400);
}

fn ensure_default_oss_kinds(configs: &mut Vec<OssConfig>) {
    let has_aliyun = configs
        .iter()
        .any(|config| config.kind == OssProviderKind::Aliyun);
    if !has_aliyun {
        configs.push(create_default_aliyun_oss_config());
    }

    let has_r2 = configs
        .iter()
        .any(|config| config.kind == OssProviderKind::R2);
    if !has_r2 {
        configs.push(create_default_r2_oss_config());
    }
}

fn normalize_legacy_oss_settings(oss: &mut ProviderOssSettings) {
    if oss.path_prefix.is_none() {
        oss.path_prefix = ProviderOssSettings::default().path_prefix;
    }
    oss.signed_url_ttl_seconds = oss.signed_url_ttl_seconds.clamp(60, 86_400);
}

fn find_legacy_provider_oss(providers: &[ProviderConfig]) -> Option<ProviderOssSettings> {
    providers
        .iter()
        .find_map(|provider| {
            provider
                .bailian
                .as_ref()
                .and_then(|bailian| bailian.legacy_oss.clone())
                .or_else(|| {
                    provider
                        .aliyun_tingwu
                        .as_ref()
                        .and_then(|aliyun| aliyun.legacy_oss.clone())
                })
        })
        .map(|mut oss| {
            normalize_legacy_oss_settings(&mut oss);
            oss
        })
}

fn legacy_oss_to_config(legacy: &ProviderOssSettings, id: &str, name: &str) -> OssConfig {
    let mut config = OssConfig {
        id: id.to_string(),
        name: name.to_string(),
        kind: OssProviderKind::Aliyun,
        access_key_id: legacy.access_key_id.clone(),
        access_key_secret: legacy.access_key_secret.clone(),
        endpoint: legacy.endpoint.clone(),
        bucket: legacy.bucket.clone(),
        path_prefix: legacy.path_prefix.clone(),
        signed_url_ttl_seconds: legacy.signed_url_ttl_seconds,
    };
    normalize_oss_config(&mut config);
    config
}

fn resolve_selected_oss_config_id(configs: &[OssConfig], current: &str) -> String {
    let current_exists = configs
        .iter()
        .any(|config| config.id.trim() == current.trim());
    if current_exists {
        return current.to_string();
    }
    configs
        .iter()
        .find(|config| oss_config_has_user_value(config))
        .or_else(|| configs.first())
        .map(|config| config.id.clone())
        .unwrap_or_default()
}

impl ProviderConfig {
    fn kind_name(&self) -> &'static str {
        match self.kind {
            ProviderKind::Bailian => "bailian",
            ProviderKind::AliyunTingwu => "aliyun_tingwu",
            ProviderKind::Openrouter => "openrouter",
            ProviderKind::LocalStt => "local_stt",
        }
    }
}

impl OssConfig {
    fn kind_name(&self) -> &'static str {
        match self.kind {
            OssProviderKind::Aliyun => "aliyun",
            OssProviderKind::R2 => "r2",
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
    pub progress_msg: Option<String>,
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
    pub phase: RecorderPhase,
    pub pending_jobs: usize,
    pub last_processing_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecorderPhase {
    Idle,
    Recording,
    Paused,
    Processing,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecorderProcessingStatus {
    pub session_id: String,
    pub phase: RecorderPhase,
    pub pending_jobs: usize,
    pub last_processing_error: Option<String>,
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
    pub oss_configs: Option<Vec<OssConfig>>,
    pub selected_oss_config_id: Option<String>,
    #[serde(rename = "oss", default, skip_serializing)]
    pub legacy_oss: Option<ProviderOssSettings>,
    pub selected_transcription_provider_id: Option<String>,
    pub selected_summary_provider_id: Option<String>,
    pub default_template_id: Option<String>,
    pub templates: Option<Vec<PromptTemplate>>,
}

#[cfg(test)]
mod tests {
    use super::{OssConfig, OssProviderKind, ProviderOssSettings, Settings};

    #[test]
    fn normalize_migrates_legacy_single_oss_to_oss_configs() {
        let mut settings = Settings::default();
        settings.oss_configs = vec![];
        settings.selected_oss_config_id.clear();
        settings.legacy_oss = Some(ProviderOssSettings {
            access_key_id: Some("legacy-ak".to_string()),
            access_key_secret: Some("legacy-sk".to_string()),
            endpoint: Some("https://oss-cn-beijing.aliyuncs.com".to_string()),
            bucket: Some("legacy-bucket".to_string()),
            path_prefix: Some("legacy-prefix".to_string()),
            signed_url_ttl_seconds: 1200,
        });

        settings.normalize();

        assert_eq!(settings.oss_configs.len(), 2);
        assert_eq!(settings.oss_configs[0].kind, OssProviderKind::Aliyun);
        assert_eq!(
            settings.oss_configs[0].access_key_id.as_deref(),
            Some("legacy-ak")
        );
        assert_eq!(settings.selected_oss_config_id, settings.oss_configs[0].id);
        assert!(settings
            .oss_configs
            .iter()
            .any(|config| config.kind == OssProviderKind::R2));
    }

    #[test]
    fn normalize_falls_back_to_first_oss_when_selected_id_missing() {
        let mut settings = Settings::default();
        settings.oss_configs = vec![
            OssConfig {
                id: "oss-a".to_string(),
                name: "OSS A".to_string(),
                kind: OssProviderKind::Aliyun,
                access_key_id: Some("ak".to_string()),
                access_key_secret: Some("sk".to_string()),
                endpoint: Some("https://oss-cn-beijing.aliyuncs.com".to_string()),
                bucket: Some("bucket-a".to_string()),
                path_prefix: Some("open-recorder".to_string()),
                signed_url_ttl_seconds: 1800,
            },
            OssConfig {
                id: "oss-b".to_string(),
                name: "OSS B".to_string(),
                kind: OssProviderKind::R2,
                access_key_id: Some("ak2".to_string()),
                access_key_secret: Some("sk2".to_string()),
                endpoint: Some("https://example.r2.cloudflarestorage.com".to_string()),
                bucket: Some("bucket-b".to_string()),
                path_prefix: Some("open-recorder".to_string()),
                signed_url_ttl_seconds: 1800,
            },
        ];
        settings.selected_oss_config_id = "missing-id".to_string();

        settings.normalize();

        assert_eq!(settings.selected_oss_config_id, "oss-a");
    }
}
