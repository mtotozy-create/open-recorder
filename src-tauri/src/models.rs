use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use serde_json::Value;

const DEFAULT_BAILIAN_PROVIDER_ID: &str = "bailian-default";
const DEFAULT_ALIYUN_PROVIDER_ID: &str = "aliyun-tingwu-default";
const DEFAULT_OPENROUTER_PROVIDER_ID: &str = "openrouter-default";
const DEFAULT_OLLAMA_PROVIDER_ID: &str = "ollama-default";
const DEFAULT_LOCAL_STT_PROVIDER_ID: &str = "local-stt-default";
const DEFAULT_SELECTED_OSS_CONFIG_ID: &str = "oss-aliyun-default";
const DEFAULT_R2_OSS_CONFIG_ID: &str = "oss-r2-default";
const DEFAULT_RECORDING_SEGMENT_SECONDS: u64 = 120;
const MIN_RECORDING_SEGMENT_SECONDS: u64 = 10;
const MAX_RECORDING_SEGMENT_SECONDS: u64 = 1800;
pub const MAX_SESSION_TAGS: usize = 3;
pub const DEFAULT_RECORDING_SESSION_TAG: &str = "#or";
pub const DEFAULT_IMPORTED_SESSION_TAG: &str = "#imported";
pub const BUILTIN_SESSION_TAGS: [&str; 4] = ["#or", "#meeting", "#call", "#imported"];

fn default_recording_segment_seconds() -> u64 {
    DEFAULT_RECORDING_SEGMENT_SECONDS
}

fn default_session_tag_catalog() -> Vec<String> {
    BUILTIN_SESSION_TAGS
        .iter()
        .map(|tag| (*tag).to_string())
        .collect()
}

pub fn normalize_tag(raw_tag: &str) -> Option<String> {
    let trimmed = raw_tag.trim();
    if trimmed.is_empty() {
        return None;
    }
    let body = trimmed.trim_start_matches('#').trim();
    if body.is_empty() {
        return None;
    }
    Some(format!("#{}", body.to_lowercase()))
}

pub fn normalize_tags(raw_tags: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut normalized = Vec::with_capacity(raw_tags.len());
    for raw_tag in raw_tags {
        if let Some(tag) = normalize_tag(raw_tag) {
            if seen.insert(tag.clone()) {
                normalized.push(tag);
            }
        }
    }
    normalized
}

pub fn validate_session_tags(raw_tags: &[String]) -> Result<Vec<String>, String> {
    let normalized = normalize_tags(raw_tags);
    if normalized.len() > MAX_SESSION_TAGS {
        return Err(format!(
            "a session can have at most {MAX_SESSION_TAGS} tags"
        ));
    }
    Ok(normalized)
}

pub fn merge_session_tags_into_catalog(catalog: &mut Vec<String>, tags: &[String]) {
    let mut merged = Vec::with_capacity(catalog.len() + tags.len());
    merged.extend(catalog.iter().cloned());
    merged.extend(tags.iter().cloned());
    *catalog = normalize_tags(&merged);
}

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
    VoiceLowStorage,
    LegacyCompatible,
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
    Ollama,
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
    pub realtime_enabled_by_default: bool,
    #[serde(default, skip_serializing)]
    pub realtime_output_level: u8,
    pub realtime_format: String,
    pub realtime_sample_rate: u32,
    pub realtime_source_language: String,
    pub realtime_language_hints: Option<String>,
    pub realtime_task_key: Option<String>,
    pub realtime_progressive_callbacks_enabled: bool,
    pub realtime_transcoding_target_audio_format: Option<String>,
    pub realtime_transcription_output_level: u8,
    pub realtime_transcription_diarization_enabled: bool,
    pub realtime_transcription_diarization_speaker_count: Option<u32>,
    pub realtime_transcription_phrase_id: Option<String>,
    pub realtime_translation_enabled: bool,
    pub realtime_translation_output_level: u8,
    pub realtime_translation_target_languages: Option<String>,
    pub realtime_auto_chapters_enabled: bool,
    pub realtime_meeting_assistance_enabled: bool,
    pub realtime_summarization_enabled: bool,
    pub realtime_summarization_types: Option<String>,
    pub realtime_text_polish_enabled: bool,
    pub realtime_service_inspection_enabled: bool,
    pub realtime_service_inspection: Option<Value>,
    pub realtime_custom_prompt_enabled: bool,
    pub realtime_custom_prompt: Option<Value>,
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
            realtime_enabled_by_default: false,
            realtime_output_level: 1,
            realtime_format: "pcm".to_string(),
            realtime_sample_rate: 16000,
            realtime_source_language: "cn".to_string(),
            realtime_language_hints: None,
            realtime_task_key: None,
            realtime_progressive_callbacks_enabled: false,
            realtime_transcoding_target_audio_format: None,
            realtime_transcription_output_level: 1,
            realtime_transcription_diarization_enabled: false,
            realtime_transcription_diarization_speaker_count: None,
            realtime_transcription_phrase_id: None,
            realtime_translation_enabled: false,
            realtime_translation_output_level: 1,
            realtime_translation_target_languages: Some("en".to_string()),
            realtime_auto_chapters_enabled: false,
            realtime_meeting_assistance_enabled: false,
            realtime_summarization_enabled: false,
            realtime_summarization_types: None,
            realtime_text_polish_enabled: false,
            realtime_service_inspection_enabled: false,
            realtime_service_inspection: None,
            realtime_custom_prompt_enabled: false,
            realtime_custom_prompt: None,
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
    pub discover_model: String,
}

impl Default for OpenrouterProviderSettings {
    fn default() -> Self {
        Self {
            api_key: None,
            base_url: "https://openrouter.ai/api/v1".to_string(),
            summary_model: "qwen/qwen-plus".to_string(),
            discover_model: "qwen/qwen-plus".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct OllamaProviderSettings {
    pub api_key: Option<String>,
    pub base_url: String,
    pub summary_model: String,
}

impl Default for OllamaProviderSettings {
    fn default() -> Self {
        Self {
            api_key: None,
            base_url: "http://127.0.0.1:11434/v1".to_string(),
            summary_model: "qwen2.5:7b-instruct".to_string(),
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
    pub ollama: Option<OllamaProviderSettings>,
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
            ollama: None,
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
    Insight,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptSegment {
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
    pub translation_text: Option<String>,
    pub translation_target_language: Option<String>,
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
#[serde(rename_all = "snake_case")]
pub enum InsightTaskStatus {
    Pending,
    InProgress,
    Completed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InsightTopicStatus {
    #[serde(alias = "in_progress")]
    Active,
    Completed,
    Blocked,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum InsightSuggestionPriority {
    High,
    Medium,
    Low,
}

impl Default for InsightSuggestionPriority {
    fn default() -> Self {
        Self::Medium
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct InsightSuggestion {
    pub title: String,
    pub rationale: String,
    pub priority: InsightSuggestionPriority,
    pub owner_hint: Option<String>,
    pub source_session_ids: Vec<String>,
}

impl Default for InsightSuggestion {
    fn default() -> Self {
        Self {
            title: String::new(),
            rationale: String::new(),
            priority: InsightSuggestionPriority::Medium,
            owner_hint: None,
            source_session_ids: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InsightTask {
    pub description: String,
    pub status: InsightTaskStatus,
    pub deadline: Option<String>,
    pub source_session_id: String,
    pub source_date: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InsightPerson {
    pub name: String,
    pub tasks: Vec<InsightTask>,
    pub decisions: Vec<String>,
    pub risks: Vec<String>,
    #[serde(default)]
    pub suggestions: Vec<InsightSuggestion>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InsightTopicProgress {
    pub date: String,
    pub description: String,
    pub source_session_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InsightTopic {
    pub name: String,
    pub progress: Vec<InsightTopicProgress>,
    pub status: InsightTopicStatus,
    pub related_people: Vec<String>,
    #[serde(default)]
    pub suggestions: Vec<InsightSuggestion>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InsightAction {
    pub description: String,
    pub assignee: Option<String>,
    pub deadline: Option<String>,
    pub source_session_id: String,
    pub source_date: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InsightResult {
    pub people: Vec<InsightPerson>,
    pub topics: Vec<InsightTopic>,
    pub upcoming_actions: Vec<InsightAction>,
    pub generated_at: String,
    pub time_range_type: String,
    pub session_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct InsightCacheEntry {
    pub key: String,
    pub result: InsightResult,
    pub cached_at: String,
    pub session_fingerprint: String,
    pub provider_id: String,
    pub model: String,
    pub prompt_version: String,
    pub keyword: Option<String>,
}

impl Default for InsightCacheEntry {
    fn default() -> Self {
        Self {
            key: String::new(),
            result: InsightResult {
                people: vec![],
                topics: vec![],
                upcoming_actions: vec![],
                generated_at: String::new(),
                time_range_type: String::new(),
                session_ids: vec![],
            },
            cached_at: String::new(),
            session_fingerprint: String::new(),
            provider_id: String::new(),
            model: String::new(),
            prompt_version: String::new(),
            keyword: None,
        }
    }
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
    pub selected_discover_provider_id: String,
    #[serde(default = "default_recording_segment_seconds")]
    pub recording_segment_seconds: u64,
    #[serde(default)]
    pub recording_input_device_id: Option<String>,
    #[serde(default = "default_session_tag_catalog")]
    pub session_tag_catalog: Vec<String>,
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
            selected_summary_provider_id: DEFAULT_OLLAMA_PROVIDER_ID.to_string(),
            selected_discover_provider_id: DEFAULT_OLLAMA_PROVIDER_ID.to_string(),
            recording_segment_seconds: DEFAULT_RECORDING_SEGMENT_SECONDS,
            recording_input_device_id: None,
            session_tag_catalog: default_session_tag_catalog(),
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
            self.selected_discover_provider_id = self.selected_summary_provider_id.clone();
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
        self.selected_discover_provider_id =
            normalize_provider_id_alias(self.selected_discover_provider_id.as_str());

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
        self.selected_discover_provider_id = resolve_selected_provider_id(
            &self.providers,
            &self.selected_discover_provider_id,
            ProviderCapability::Summary,
        );
        self.session_tag_catalog = normalize_tags(&self.session_tag_catalog);
        merge_session_tags_into_catalog(
            &mut self.session_tag_catalog,
            &default_session_tag_catalog(),
        );
        self.recording_segment_seconds = self
            .recording_segment_seconds
            .clamp(MIN_RECORDING_SEGMENT_SECONDS, MAX_RECORDING_SEGMENT_SECONDS);
        self.recording_input_device_id = self.recording_input_device_id.clone().and_then(|value| {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        });
    }

    fn migrate_legacy_providers(&self) -> (Vec<ProviderConfig>, ProviderOssSettings) {
        let default_bailian = BailianProviderSettings::default();
        let default_aliyun = AliyunTingwuProviderSettings::default();
        let default_oss = ProviderOssSettings::default();
        let default_openrouter = OpenrouterProviderSettings::default();
        let default_ollama = OllamaProviderSettings::default();

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
            ollama: None,
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
                realtime_enabled_by_default: default_aliyun.realtime_enabled_by_default,
                realtime_output_level: default_aliyun.realtime_output_level,
                realtime_format: default_aliyun.realtime_format.clone(),
                realtime_sample_rate: default_aliyun.realtime_sample_rate,
                realtime_source_language: default_aliyun.realtime_source_language.clone(),
                realtime_language_hints: default_aliyun.realtime_language_hints.clone(),
                realtime_task_key: default_aliyun.realtime_task_key.clone(),
                realtime_progressive_callbacks_enabled: default_aliyun
                    .realtime_progressive_callbacks_enabled,
                realtime_transcoding_target_audio_format: default_aliyun
                    .realtime_transcoding_target_audio_format
                    .clone(),
                realtime_transcription_output_level: default_aliyun
                    .realtime_transcription_output_level,
                realtime_transcription_diarization_enabled: default_aliyun
                    .realtime_transcription_diarization_enabled,
                realtime_transcription_diarization_speaker_count: default_aliyun
                    .realtime_transcription_diarization_speaker_count,
                realtime_transcription_phrase_id: default_aliyun
                    .realtime_transcription_phrase_id
                    .clone(),
                realtime_translation_enabled: default_aliyun.realtime_translation_enabled,
                realtime_translation_output_level: default_aliyun.realtime_translation_output_level,
                realtime_translation_target_languages: default_aliyun
                    .realtime_translation_target_languages
                    .clone(),
                realtime_auto_chapters_enabled: default_aliyun.realtime_auto_chapters_enabled,
                realtime_meeting_assistance_enabled: default_aliyun
                    .realtime_meeting_assistance_enabled,
                realtime_summarization_enabled: default_aliyun.realtime_summarization_enabled,
                realtime_summarization_types: default_aliyun.realtime_summarization_types.clone(),
                realtime_text_polish_enabled: default_aliyun.realtime_text_polish_enabled,
                realtime_service_inspection_enabled: default_aliyun
                    .realtime_service_inspection_enabled,
                realtime_service_inspection: default_aliyun.realtime_service_inspection.clone(),
                realtime_custom_prompt_enabled: default_aliyun.realtime_custom_prompt_enabled,
                realtime_custom_prompt: default_aliyun.realtime_custom_prompt.clone(),
                poll_interval_seconds: self
                    .legacy_aliyun_poll_interval_seconds
                    .unwrap_or(default_aliyun.poll_interval_seconds),
                max_polling_minutes: self
                    .legacy_aliyun_max_polling_minutes
                    .unwrap_or(default_aliyun.max_polling_minutes),
                legacy_oss: None,
            }),
            openrouter: None,
            ollama: None,
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
                discover_model: default_openrouter.discover_model,
            }),
            ollama: None,
            local_stt: None,
        };
        let ollama_provider = ProviderConfig {
            id: DEFAULT_OLLAMA_PROVIDER_ID.to_string(),
            name: "Ollama".to_string(),
            kind: ProviderKind::Ollama,
            capabilities: vec![ProviderCapability::Summary],
            enabled: true,
            bailian: None,
            aliyun_tingwu: None,
            openrouter: None,
            ollama: Some(OllamaProviderSettings {
                api_key: None,
                base_url: default_ollama.base_url,
                summary_model: default_ollama.summary_model,
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
            ollama: None,
            local_stt: Some(LocalSttProviderSettings::default()),
        };

        (
            vec![
                bailian_provider,
                aliyun_provider,
                openrouter_provider,
                ollama_provider,
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
            ollama: None,
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
            ollama: None,
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
            ollama: None,
            local_stt: None,
        },
        ProviderConfig {
            id: DEFAULT_OLLAMA_PROVIDER_ID.to_string(),
            name: "Ollama".to_string(),
            kind: ProviderKind::Ollama,
            capabilities: vec![ProviderCapability::Summary],
            enabled: true,
            bailian: None,
            aliyun_tingwu: None,
            openrouter: None,
            ollama: Some(OllamaProviderSettings::default()),
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
            ollama: None,
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
            config.realtime_output_level = config.realtime_output_level.clamp(1, 2);
            if config.realtime_transcription_output_level == 1
                && config.realtime_translation_output_level == 1
                && config.realtime_output_level == 2
            {
                config.realtime_transcription_output_level = 2;
                config.realtime_translation_output_level = 2;
            }
            config.realtime_transcription_output_level =
                config.realtime_transcription_output_level.clamp(1, 2);
            config.realtime_translation_output_level =
                config.realtime_translation_output_level.clamp(1, 2);
            config.realtime_sample_rate = if config.realtime_sample_rate == 8000 {
                8000
            } else {
                16000
            };
            config.realtime_format =
                match config.realtime_format.trim().to_ascii_lowercase().as_str() {
                    "pcm" | "opus" | "aac" | "speex" | "mp3" => {
                        config.realtime_format.trim().to_ascii_lowercase()
                    }
                    _ => "pcm".to_string(),
                };
            config.realtime_source_language = match config
                .realtime_source_language
                .trim()
                .to_ascii_lowercase()
                .replace('_', "-")
                .as_str()
            {
                "zh" | "zh-cn" | "cn" => "cn".to_string(),
                "en" | "yue" | "ja" | "ko" | "multilingual" => config
                    .realtime_source_language
                    .trim()
                    .to_ascii_lowercase()
                    .replace('_', "-"),
                _ => "cn".to_string(),
            };
            config.realtime_transcoding_target_audio_format = config
                .realtime_transcoding_target_audio_format
                .clone()
                .and_then(|value| {
                    let normalized = value.trim().to_ascii_lowercase();
                    if normalized == "mp3" {
                        Some("mp3".to_string())
                    } else {
                        None
                    }
                });
            config.realtime_transcription_diarization_speaker_count = config
                .realtime_transcription_diarization_speaker_count
                .map(|value| value.clamp(0, 64));
            config.legacy_oss = None;
            Some(config)
        }
        _ => None,
    };

    provider.openrouter = match provider.kind {
        ProviderKind::Openrouter => {
            let mut config = provider.openrouter.clone().unwrap_or_default();
            if config.discover_model.trim().is_empty() {
                config.discover_model = config.summary_model.clone();
            }
            Some(config)
        }
        _ => None,
    };

    provider.ollama = match provider.kind {
        ProviderKind::Ollama => Some(provider.ollama.clone().unwrap_or_default()),
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
        ProviderKind::Ollama => vec![ProviderCapability::Summary],
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
        "ollama-summary-default" => DEFAULT_OLLAMA_PROVIDER_ID.to_string(),
        other => other.to_string(),
    }
}

fn canonicalize_providers_by_kind(providers: &[ProviderConfig]) -> Vec<ProviderConfig> {
    let defaults = create_default_providers();
    let ordered_kinds = [
        ProviderKind::Bailian,
        ProviderKind::AliyunTingwu,
        ProviderKind::Openrouter,
        ProviderKind::Ollama,
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
                ProviderKind::Ollama => {
                    if let Some(config) = candidates
                        .iter()
                        .find_map(|item| item.ollama.as_ref())
                        .cloned()
                    {
                        provider.ollama = Some(config);
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

    let preferred_id = match capability {
        ProviderCapability::Transcription => DEFAULT_BAILIAN_PROVIDER_ID,
        ProviderCapability::Summary => DEFAULT_OLLAMA_PROVIDER_ID,
    };
    if let Some(provider) = providers
        .iter()
        .find(|provider| provider.id == preferred_id)
        .filter(|provider| provider_supports_capability(provider, capability.clone()))
    {
        return provider.id.clone();
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
            ollama: None,
            local_stt: None,
        },
        ProviderCapability::Summary => ProviderConfig {
            id: DEFAULT_OLLAMA_PROVIDER_ID.to_string(),
            name: "Ollama".to_string(),
            kind: ProviderKind::Ollama,
            capabilities: vec![ProviderCapability::Summary],
            enabled: true,
            bailian: None,
            aliyun_tingwu: None,
            openrouter: None,
            ollama: Some(OllamaProviderSettings::default()),
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
            ProviderKind::Ollama => "ollama",
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

fn default_session_discoverable() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Session {
    pub id: String,
    pub name: Option<String>,
    #[serde(default = "default_session_discoverable")]
    pub discoverable: bool,
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
    pub tags: Vec<String>,
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
            discoverable: default_session_discoverable(),
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
            tags: vec![],
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
    pub discoverable: bool,
    pub status: SessionStatus,
    pub created_at: String,
    pub updated_at: String,
    pub elapsed_ms: u64,
    pub quality_preset: RecordingQualityPreset,
    pub tags: Vec<String>,
}

impl From<&Session> for SessionSummary {
    fn from(value: &Session) -> Self {
        Self {
            id: value.id.clone(),
            name: value.name.clone(),
            discoverable: value.discoverable,
            status: value.status.clone(),
            created_at: value.created_at.clone(),
            updated_at: value.updated_at.clone(),
            elapsed_ms: value.elapsed_ms,
            quality_preset: value.quality_preset.clone(),
            tags: value.tags.clone(),
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
    pub input_device_id: Option<String>,
    pub input_device_name: Option<String>,
    pub fallback_from_input_device_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecorderInputDevice {
    pub id: String,
    pub name: String,
    pub is_default: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecorderStatus {
    pub session_id: String,
    pub elapsed_ms: u64,
    pub segment_count: usize,
    pub persisted_segment_count: usize,
    pub quality_preset: RecordingQualityPreset,
    pub rms: f32,
    pub peak: f32,
    pub phase: RecorderPhase,
    pub pending_jobs: usize,
    pub last_processing_error: Option<String>,
    pub realtime: RecorderRealtimeStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecorderRealtimeState {
    Idle,
    Connecting,
    Running,
    Paused,
    Stopping,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecorderRealtimeStatus {
    pub enabled: bool,
    pub source_language: String,
    pub translation_enabled: bool,
    pub translation_target_language: String,
    pub state: RecorderRealtimeState,
    pub preview_text: String,
    pub segment_count: usize,
    pub segments: Vec<TranscriptSegment>,
    pub last_error: Option<String>,
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
    pub selected_discover_provider_id: Option<String>,
    pub recording_segment_seconds: Option<u64>,
    pub recording_input_device_id: Option<String>,
    pub session_tag_catalog: Option<Vec<String>>,
    pub default_template_id: Option<String>,
    pub templates: Option<Vec<PromptTemplate>>,
}

#[cfg(test)]
mod tests {
    use super::{OssConfig, OssProviderKind, ProviderKind, ProviderOssSettings, Settings};

    #[test]
    fn default_settings_select_ollama_for_summary() {
        let settings = Settings::default();
        assert_eq!(settings.selected_summary_provider_id, "ollama-default");
        assert_eq!(settings.selected_discover_provider_id, "ollama-default");
        assert!(settings
            .providers
            .iter()
            .any(|provider| provider.kind == ProviderKind::Ollama));
    }

    #[test]
    fn normalize_preserves_existing_valid_summary_provider_selection() {
        let mut settings = Settings::default();
        settings.selected_summary_provider_id = "bailian-default".to_string();
        settings.selected_discover_provider_id = "bailian-default".to_string();

        settings.normalize();

        assert_eq!(settings.selected_summary_provider_id, "bailian-default");
        assert_eq!(settings.selected_discover_provider_id, "bailian-default");
    }

    #[test]
    fn normalize_prefers_ollama_when_summary_selection_is_missing() {
        let mut settings = Settings::default();
        settings.selected_summary_provider_id = "missing-provider".to_string();
        settings.selected_discover_provider_id = "missing-provider".to_string();

        settings.normalize();

        assert_eq!(settings.selected_summary_provider_id, "ollama-default");
        assert_eq!(settings.selected_discover_provider_id, "ollama-default");
    }

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

    #[test]
    fn normalize_clamps_recording_segment_seconds() {
        let mut settings = Settings::default();
        settings.recording_segment_seconds = 1;
        settings.normalize();
        assert_eq!(settings.recording_segment_seconds, 10);

        settings.recording_segment_seconds = 3600;
        settings.normalize();
        assert_eq!(settings.recording_segment_seconds, 1800);
    }

    #[test]
    fn normalize_trims_recording_input_device_id() {
        let mut settings = Settings::default();
        settings.recording_input_device_id = Some("  1:Built-in Microphone  ".to_string());
        settings.normalize();
        assert_eq!(
            settings.recording_input_device_id.as_deref(),
            Some("1:Built-in Microphone")
        );

        settings.recording_input_device_id = Some("   ".to_string());
        settings.normalize();
        assert!(settings.recording_input_device_id.is_none());
    }

    #[test]
    fn normalize_tag_deduplicates_and_prefixes() {
        let input = vec![
            "or".to_string(),
            "#OR".to_string(),
            "  #会议 ".to_string(),
            "  ".to_string(),
        ];

        let normalized = super::normalize_tags(&input);
        assert_eq!(normalized, vec!["#or".to_string(), "#会议".to_string()]);
    }

    #[test]
    fn validate_session_tags_enforces_max_limit() {
        let tags = vec![
            "#a".to_string(),
            "#b".to_string(),
            "#c".to_string(),
            "#d".to_string(),
        ];
        let result = super::validate_session_tags(&tags);
        assert!(result.is_err());
    }
}
