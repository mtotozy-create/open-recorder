use serde::{Deserialize, Serialize};

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

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TranscriptionProvider {
    #[default]
    Bailian,
    AliyunTingwu,
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
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    pub transcription_provider: TranscriptionProvider,
    pub bailian_api_key: Option<String>,
    pub bailian_base_url: String,
    pub bailian_transcription_model: String,
    pub bailian_summary_model: String,
    pub bailian_oss_access_key_id: Option<String>,
    pub bailian_oss_access_key_secret: Option<String>,
    pub bailian_oss_endpoint: Option<String>,
    pub bailian_oss_bucket: Option<String>,
    pub bailian_oss_path_prefix: Option<String>,
    pub bailian_oss_signed_url_ttl_seconds: u64,
    pub aliyun_access_key_id: Option<String>,
    pub aliyun_access_key_secret: Option<String>,
    pub aliyun_app_key: Option<String>,
    pub aliyun_endpoint: String,
    pub aliyun_source_language: String,
    pub aliyun_file_url_prefix: Option<String>,
    pub aliyun_language_hints: Option<String>,
    pub aliyun_transcription_normalization_enabled: bool,
    pub aliyun_transcription_paragraph_enabled: bool,
    pub aliyun_transcription_punctuation_prediction_enabled: bool,
    pub aliyun_transcription_disfluency_removal_enabled: bool,
    pub aliyun_transcription_speaker_diarization_enabled: bool,
    pub aliyun_poll_interval_seconds: u64,
    pub aliyun_max_polling_minutes: u64,
    pub default_template_id: String,
    pub templates: Vec<PromptTemplate>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            transcription_provider: TranscriptionProvider::Bailian,
            bailian_api_key: None,
            bailian_base_url: "https://dashscope.aliyuncs.com".to_string(),
            bailian_transcription_model: "paraformer-v2".to_string(),
            bailian_summary_model: "qwen-plus".to_string(),
            bailian_oss_access_key_id: None,
            bailian_oss_access_key_secret: None,
            bailian_oss_endpoint: None,
            bailian_oss_bucket: None,
            bailian_oss_path_prefix: Some("open-recorder".to_string()),
            bailian_oss_signed_url_ttl_seconds: 1800,
            aliyun_access_key_id: None,
            aliyun_access_key_secret: None,
            aliyun_app_key: None,
            aliyun_endpoint: "https://tingwu.cn-beijing.aliyuncs.com".to_string(),
            aliyun_source_language: "cn".to_string(),
            aliyun_file_url_prefix: None,
            aliyun_language_hints: None,
            aliyun_transcription_normalization_enabled: true,
            aliyun_transcription_paragraph_enabled: true,
            aliyun_transcription_punctuation_prediction_enabled: true,
            aliyun_transcription_disfluency_removal_enabled: false,
            aliyun_transcription_speaker_diarization_enabled: true,
            aliyun_poll_interval_seconds: 60,
            aliyun_max_polling_minutes: 180,
            default_template_id: "meeting-default".to_string(),
            templates: vec![PromptTemplate {
                id: "meeting-default".to_string(),
                name: "Meeting Default".to_string(),
                system_prompt: "You are an assistant for writing concise meeting notes."
                    .to_string(),
                user_prompt: "Organize transcript into: conclusion, action items, risks, timeline."
                    .to_string(),
                variables: vec!["language".to_string(), "audience".to_string()],
            }],
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
    pub exported_mp3_path: Option<String>,
    pub exported_m4a_path: Option<String>,
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
            exported_mp3_path: None,
            exported_m4a_path: None,
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
    pub transcription_provider: Option<TranscriptionProvider>,
    pub bailian_api_key: Option<Option<String>>,
    pub bailian_base_url: Option<String>,
    pub bailian_transcription_model: Option<String>,
    pub bailian_summary_model: Option<String>,
    pub bailian_oss_access_key_id: Option<Option<String>>,
    pub bailian_oss_access_key_secret: Option<Option<String>>,
    pub bailian_oss_endpoint: Option<Option<String>>,
    pub bailian_oss_bucket: Option<Option<String>>,
    pub bailian_oss_path_prefix: Option<Option<String>>,
    pub bailian_oss_signed_url_ttl_seconds: Option<u64>,
    pub aliyun_access_key_id: Option<Option<String>>,
    pub aliyun_access_key_secret: Option<Option<String>>,
    pub aliyun_app_key: Option<Option<String>>,
    pub aliyun_endpoint: Option<String>,
    pub aliyun_source_language: Option<String>,
    pub aliyun_file_url_prefix: Option<Option<String>>,
    pub aliyun_language_hints: Option<Option<String>>,
    pub aliyun_transcription_normalization_enabled: Option<bool>,
    pub aliyun_transcription_paragraph_enabled: Option<bool>,
    pub aliyun_transcription_punctuation_prediction_enabled: Option<bool>,
    pub aliyun_transcription_disfluency_removal_enabled: Option<bool>,
    pub aliyun_transcription_speaker_diarization_enabled: Option<bool>,
    pub aliyun_poll_interval_seconds: Option<u64>,
    pub aliyun_max_polling_minutes: Option<u64>,
    pub default_template_id: Option<String>,
    pub templates: Option<Vec<PromptTemplate>>,
}
