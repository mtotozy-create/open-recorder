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
  discoverModel: string;
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
  selectedDiscoverProviderId: string;
  recordingSegmentSeconds: number;
  recordingInputDeviceId?: string | null;
  summaryExportFolderPath?: string | null;
  sessionTagCatalog: string[];
  defaultTemplateId: string;
  templates: PromptTemplate[];
};

export type StorageUsageSummary = {
  dataDirPath: string;
  totalBytes: number;
};

export type SummaryMarkdownExportProgress = {
  totalSessions: number;
  processedSessions: number;
  exportedCount: number;
  skippedExistingCount: number;
  skippedEmptyCount: number;
  currentSessionName?: string | null;
};

export type SummaryMarkdownExportResult = {
  totalSessions: number;
  summarySessions: number;
  exportedCount: number;
  skippedExistingCount: number;
  skippedEmptyCount: number;
  folderPath: string;
};

export type InsightTimeRange = "1d" | "2d" | "3d" | "1w" | "1m";
export type InsightSelectionMode = "timeRange" | "sessions";

export type InsightQueryRequest = {
  selectionMode: InsightSelectionMode;
  timeRange?: InsightTimeRange;
  sessionIds?: string[];
  keyword?: string;
  includeSuggestions?: boolean;
};

export type DiscoverSubView = "people" | "topics" | "actions";

export type InsightTask = {
  description: string;
  status: "pending" | "in_progress" | "completed";
  deadline?: string;
  sourceSessionId: string;
  sourceDate: string;
};

export type InsightPerson = {
  name: string;
  tasks: InsightTask[];
  decisions: string[];
  risks: string[];
  suggestions: InsightSuggestion[];
};

export type InsightTopicProgress = {
  date: string;
  description: string;
  sourceSessionId: string;
};

export type InsightTopic = {
  name: string;
  progress: InsightTopicProgress[];
  status: "active" | "completed" | "blocked";
  relatedPeople: string[];
  suggestions: InsightSuggestion[];
};

export type InsightSuggestionPriority = "high" | "medium" | "low";

export type InsightSuggestion = {
  title: string;
  rationale: string;
  priority: InsightSuggestionPriority;
  ownerHint?: string;
  sourceSessionIds: string[];
};

export type InsightAction = {
  description: string;
  assignee?: string;
  deadline?: string;
  sourceSessionId: string;
  sourceDate: string;
};

export type InsightResult = {
  people: InsightPerson[];
  topics: InsightTopic[];
  upcomingActions: InsightAction[];
  generatedAt: string;
  timeRangeType: InsightTimeRange | "sessions";
  sessionIds: string[];
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
  discoverable: boolean;
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
  kind: "transcription" | "summary" | "insight";
  status: JobStatus;
  createdAt: string;
  updatedAt: string;
  error?: string;
  progressMsg?: string;
};
