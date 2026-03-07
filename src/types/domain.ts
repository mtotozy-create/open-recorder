export type SessionStatus =
  | "recording"
  | "paused"
  | "stopped"
  | "transcribing"
  | "summarizing"
  | "completed"
  | "failed";

export type RecordingQualityPreset = "standard" | "hd" | "hifi";

export type JobStatus = "queued" | "running" | "completed" | "failed";

export type TranscriptSegment = {
  startMs: number;
  endMs: number;
  text: string;
  confidence?: number;
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

export type ProviderKind = "bailian" | "aliyun_tingwu" | "openrouter";

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
  pollIntervalSeconds: number;
  maxPollingMinutes: number;
};

export type OpenrouterProviderSettings = {
  apiKey?: string;
  baseUrl: string;
  summaryModel: string;
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
};

export type Settings = {
  providers: ProviderConfig[];
  ossConfigs: OssConfig[];
  selectedOssConfigId: string;
  selectedTranscriptionProviderId: string;
  selectedSummaryProviderId: string;
  defaultTemplateId: string;
  templates: PromptTemplate[];
};

export type SessionSummary = {
  id: string;
  name?: string;
  status: SessionStatus;
  createdAt: string;
  updatedAt: string;
  elapsedMs: number;
  qualityPreset: RecordingQualityPreset;
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

export type RecorderRuntimeStatus = {
  sessionId: string;
  elapsedMs: number;
  segmentCount: number;
  qualityPreset: RecordingQualityPreset;
  rms: number;
  peak: number;
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
