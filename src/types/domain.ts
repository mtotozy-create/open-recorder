import type { LocalWhisperModelId } from "../lib/localSttWhisperModels";

export type SessionStatus =
  | "recording"
  | "paused"
  | "processing"
  | "stopped"
  | "transcribing"
  | "summarizing"
  | "completed"
  | "failed";

export type RecordingQualityPreset =
  | "voice_low_storage"
  | "legacy_compatible"
  | "standard"
  | "hd"
  | "hifi";

export type JobStatus = "queued" | "running" | "completed" | "failed";

export type TranscriptSegment = {
  startMs: number;
  endMs: number;
  text: string;
  translationText?: string;
  translationTargetLanguage?: string;
  confidence?: number;
  speakerId?: string;
  speakerLabel?: string;
};

export type SummaryResult = {
  title: string;
  decisions: string[];
  actionItems: string[];
  risks: string[];
  timeline: string[];
  rawMarkdown: string;
};

export type PromptTemplate = {
  id: string;
  name: string;
  systemPrompt: string;
  userPrompt: string;
  variables: string[];
};

export type ProviderKind = "bailian" | "aliyun_tingwu" | "openrouter" | "ollama" | "local_stt";

export type ProviderCapability = "transcription" | "summary";

export type OssProviderKind = "aliyun" | "r2";

export type OssConfig = {
  id: string;
  name: string;
  kind: OssProviderKind;
  accessKeyId?: string;
  accessKeySecret?: string;
  endpoint?: string;
  bucket?: string;
  pathPrefix?: string;
  signedUrlTtlSeconds: number;
};

export type BailianProviderSettings = {
  apiKey?: string;
  baseUrl: string;
  transcriptionModel: string;
  summaryModel: string;
};

export type AliyunTingwuProviderSettings = {
  accessKeyId?: string;
  accessKeySecret?: string;
  appKey?: string;
  endpoint: string;
  sourceLanguage: string;
  fileUrlPrefix?: string;
  languageHints?: string;
  transcriptionNormalizationEnabled: boolean;
  transcriptionParagraphEnabled: boolean;
  transcriptionPunctuationPredictionEnabled: boolean;
  transcriptionDisfluencyRemovalEnabled: boolean;
  transcriptionSpeakerDiarizationEnabled: boolean;
  realtimeEnabledByDefault: boolean;
  realtimeFormat: "pcm" | "opus" | "aac" | "speex" | "mp3";
  realtimeSampleRate: 8000 | 16000;
  realtimeSourceLanguage: "cn" | "en" | "yue" | "ja" | "ko" | "multilingual";
  realtimeLanguageHints?: string;
  realtimeTaskKey?: string;
  realtimeProgressiveCallbacksEnabled: boolean;
  realtimeTranscodingTargetAudioFormat?: "mp3";
  realtimeTranscriptionOutputLevel: 1 | 2;
  realtimeTranscriptionDiarizationEnabled: boolean;
  realtimeTranscriptionDiarizationSpeakerCount?: number;
  realtimeTranscriptionPhraseId?: string;
  realtimeTranslationEnabled: boolean;
  realtimeTranslationOutputLevel: 1 | 2;
  realtimeTranslationTargetLanguages?: string;
  realtimeAutoChaptersEnabled: boolean;
  realtimeMeetingAssistanceEnabled: boolean;
  realtimeSummarizationEnabled: boolean;
  realtimeSummarizationTypes?: string;
  realtimeTextPolishEnabled: boolean;
  realtimeServiceInspectionEnabled: boolean;
  realtimeServiceInspection?: Record<string, unknown>;
  realtimeCustomPromptEnabled: boolean;
  realtimeCustomPrompt?: Record<string, unknown>;
  pollIntervalSeconds: number;
  maxPollingMinutes: number;
};

export type OpenrouterProviderSettings = {
  apiKey?: string;
  baseUrl: string;
  summaryModel: string;
};

export type OllamaProviderSettings = {
  apiKey?: string;
  baseUrl: string;
  summaryModel: string;
};

export type LocalSttEngine = "whisper" | "sensevoice_small";

export type LocalSttProviderSettings = {
  pythonPath?: string;
  venvDir?: string;
  modelCacheDir?: string;
  engine: LocalSttEngine;
  whisperModel: LocalWhisperModelId;
  senseVoiceModel: string;
  language: "auto" | "zh" | "en";
  diarizationEnabled: boolean;
  minSpeakers?: number;
  maxSpeakers?: number;
  speakerCountHint?: number;
  computeDevice: "auto" | "cpu" | "mps" | "cuda";
  vadEnabled: boolean;
  chunkSeconds: number;
};

export type ProviderConfig = {
  id: string;
  name: string;
  kind: ProviderKind;
  capabilities: ProviderCapability[];
  enabled: boolean;
  bailian?: BailianProviderSettings;
  aliyunTingwu?: AliyunTingwuProviderSettings;
  openrouter?: OpenrouterProviderSettings;
  ollama?: OllamaProviderSettings;
  localStt?: LocalSttProviderSettings;
};

export type Settings = {
  providers: ProviderConfig[];
  ossConfigs: OssConfig[];
  selectedOssConfigId: string;
  selectedTranscriptionProviderId: string;
  selectedSummaryProviderId: string;
  recordingSegmentSeconds: number;
  recordingInputDeviceId?: string | null;
  sessionTagCatalog: string[];
  defaultTemplateId: string;
  templates: PromptTemplate[];
};

export type RecorderInputDevice = {
  id: string;
  name: string;
  isDefault: boolean;
};

export type StartRecordingResponse = {
  sessionId: string;
  inputDeviceId?: string;
  inputDeviceName?: string;
  fallbackFromInputDeviceId?: string;
};

export type LocalProviderStatus = {
  pythonReady: boolean;
  venvReady: boolean;
  workerScriptReady: boolean;
  pythonExecutable: string;
  venvDir: string;
  modelCacheDir: string;
  workerScriptPath: string;
};

export type SessionSummary = {
  id: string;
  name?: string;
  status: SessionStatus;
  createdAt: string;
  updatedAt: string;
  elapsedMs: number;
  qualityPreset: RecordingQualityPreset;
  tags: string[];
};

export type AudioSegmentMeta = {
  path: string;
  sequence: number;
  startedAt: string;
  endedAt: string;
  durationMs: number;
  sampleRate: number;
  channels: number;
  format: string;
  fileSizeBytes: number;
};

export type SessionDetail = SessionSummary & {
  inputDeviceId?: string;
  audioSegments: string[];
  audioSegmentMeta: AudioSegmentMeta[];
  sampleRate: number;
  channels: number;
  exportedM4aPath?: string;
  exportedM4aSize?: number;
  exportedM4aCreatedAt?: string;
  exportedWavPath?: string;
  exportedWavSize?: number;
  exportedWavCreatedAt?: string;
  exportedMp3Path?: string;
  exportedMp3Size?: number;
  exportedMp3CreatedAt?: string;
  transcript: TranscriptSegment[];
  summary?: SummaryResult;
};

export type RecorderPhase = "idle" | "recording" | "paused" | "processing" | "error";

export type RecorderRuntimeStatus = {
  sessionId: string;
  elapsedMs: number;
  segmentCount: number;
  persistedSegmentCount: number;
  qualityPreset: RecordingQualityPreset;
  rms: number;
  peak: number;
  phase: RecorderPhase;
  pendingJobs: number;
  lastProcessingError?: string;
  realtime: {
    enabled: boolean;
    sourceLanguage: string;
    translationEnabled: boolean;
    translationTargetLanguage: string;
    state: "idle" | "connecting" | "running" | "paused" | "stopping" | "error";
    previewText: string;
    segmentCount: number;
    segments: TranscriptSegment[];
    lastError?: string;
  };
};

export type RecorderProcessingStatus = {
  sessionId: string;
  phase: RecorderPhase;
  pendingJobs: number;
  lastProcessingError?: string;
};

export type JobInfo = {
  id: string;
  sessionId: string;
  kind: "transcription" | "summary";
  status: JobStatus;
  createdAt: string;
  updatedAt: string;
  error?: string;
  progressMsg?: string;
};
