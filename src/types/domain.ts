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

export type TranscriptionProvider = "bailian" | "aliyun_tingwu";

export type Settings = {
  transcriptionProvider: TranscriptionProvider;
  bailianApiKey?: string;
  bailianBaseUrl: string;
  bailianTranscriptionModel: string;
  bailianSummaryModel: string;
  bailianOssAccessKeyId?: string;
  bailianOssAccessKeySecret?: string;
  bailianOssEndpoint?: string;
  bailianOssBucket?: string;
  bailianOssPathPrefix?: string;
  bailianOssSignedUrlTtlSeconds: number;
  aliyunAccessKeyId?: string;
  aliyunAccessKeySecret?: string;
  aliyunAppKey?: string;
  aliyunEndpoint: string;
  aliyunSourceLanguage: string;
  aliyunFileUrlPrefix?: string;
  aliyunLanguageHints?: string;
  aliyunTranscriptionNormalizationEnabled: boolean;
  aliyunTranscriptionParagraphEnabled: boolean;
  aliyunTranscriptionPunctuationPredictionEnabled: boolean;
  aliyunTranscriptionDisfluencyRemovalEnabled: boolean;
  aliyunTranscriptionSpeakerDiarizationEnabled: boolean;
  aliyunPollIntervalSeconds: number;
  aliyunMaxPollingMinutes: number;
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
};

export type SessionDetail = SessionSummary & {
  inputDeviceId?: string;
  audioSegments: string[];
  audioSegmentMeta: AudioSegmentMeta[];
  sampleRate: number;
  channels: number;
  exportedM4aPath?: string;
  exportedWavPath?: string;
  exportedMp3Path?: string;
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
};
