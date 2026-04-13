import { invoke } from "@tauri-apps/api/core";
import type {
  InsightQueryRequest,
  InsightResult,
  JobInfo,
  LocalProviderStatus,
  RecorderInputDevice,
  RecorderProcessingStatus,
  RecorderRuntimeStatus,
  RecordingQualityPreset,
  SessionDetail,
  SessionSummary,
  StartRecordingResponse,
  Settings,
  StorageUsageSummary
} from "../types/domain";

export async function startRecording(
  inputDeviceId?: string,
  qualityPreset: RecordingQualityPreset = "standard",
  realtimeEnabled?: boolean,
  realtimeSourceLanguage?: string,
  realtimeTranslateEnabled?: boolean,
  realtimeTranslateTargetLanguage?: string
): Promise<StartRecordingResponse> {
  return invoke<StartRecordingResponse>("recorder_start", {
    inputDeviceId,
    qualityPreset,
    realtimeEnabled,
    realtimeSourceLanguage,
    realtimeTranslateEnabled,
    realtimeTranslateTargetLanguage
  });
}

export async function listInputDevices(): Promise<RecorderInputDevice[]> {
  return invoke("recorder_list_input_devices");
}

export async function pauseRecording(sessionId: string): Promise<void> {
  await invoke("recorder_pause", { sessionId });
}

export async function resumeRecording(sessionId: string): Promise<void> {
  await invoke("recorder_resume", { sessionId });
}

export async function stopRecording(sessionId: string): Promise<void> {
  await invoke("recorder_stop", { sessionId });
}

export async function toggleRealtimeTranscription(
  sessionId: string,
  enabled: boolean
): Promise<void> {
  await invoke("recorder_toggle_realtime", { sessionId, enabled });
}

export async function toggleRealtimeTranslation(
  sessionId: string,
  enabled: boolean
): Promise<void> {
  await invoke("recorder_toggle_realtime_translation", { sessionId, enabled });
}

export async function setRealtimeTranslationTargetLanguage(
  sessionId: string,
  targetLanguage: string
): Promise<void> {
  await invoke("recorder_set_realtime_translation_target", { sessionId, targetLanguage });
}

export async function setRealtimeSourceLanguage(
  sessionId: string,
  sourceLanguage: string
): Promise<void> {
  await invoke("recorder_set_realtime_source_language", { sessionId, sourceLanguage });
}

export async function getRecorderStatus(sessionId: string): Promise<RecorderRuntimeStatus> {
  return invoke("recorder_status", {
    sessionId
  });
}

export async function getRecorderProcessingStatus(
  sessionId: string
): Promise<RecorderProcessingStatus> {
  return invoke("recorder_processing_status", {
    sessionId
  });
}

export async function exportRecording(
  sessionId: string,
  format: "m4a" | "mp3"
): Promise<string> {
  const response = await invoke<{ path: string }>("recorder_export", {
    sessionId,
    format
  });
  return response.path;
}

export async function listSessions(): Promise<SessionSummary[]> {
  return invoke("session_list");
}

export async function createSessionFromAudio(
  fileName: string,
  audioBytes: number[],
  mimeType?: string,
  durationMs?: number
): Promise<string> {
  const response = await invoke<{ sessionId: string }>("session_create_from_audio", {
    fileName,
    audioBytes,
    mimeType,
    durationMs
  });
  return response.sessionId;
}

export async function createSessionFromSegments(
  sessionId: string,
  segmentPaths: string[]
): Promise<string> {
  const response = await invoke<{ sessionId: string }>("session_create_from_segments", {
    sessionId,
    segmentPaths
  });
  return response.sessionId;
}

export async function getSession(sessionId: string): Promise<SessionDetail> {
  return invoke("session_get", { sessionId });
}

export async function renameSession(sessionId: string, name: string): Promise<void> {
  await invoke("session_rename", { sessionId, name });
}

export async function setSessionTags(sessionId: string, tags: string[]): Promise<void> {
  await invoke("session_set_tags", { sessionId, tags });
}

export async function setSessionDiscoverable(
  sessionId: string,
  discoverable: boolean
): Promise<void> {
  await invoke("session_set_discoverable", { sessionId, discoverable });
}

export async function updateSessionSummaryRawMarkdown(
  sessionId: string,
  rawMarkdown: string
): Promise<void> {
  await invoke("session_update_summary_raw_markdown", { sessionId, rawMarkdown });
}

export async function deleteSession(sessionId: string): Promise<void> {
  await invoke("session_delete", { sessionId });
}

export async function deleteSessionSegment(
  sessionId: string,
  segmentPath: string
): Promise<void> {
  await invoke("session_delete_segment", { sessionId, segmentPath });
}

export async function deleteSessionSegments(sessionId: string): Promise<void> {
  await invoke("session_delete_segments", { sessionId });
}

export async function enqueueTranscription(
  sessionId: string,
  providerId?: string
): Promise<string> {
  const response = await invoke<{ jobId: string }>("transcribe_enqueue", {
    sessionId,
    providerId
  });
  return response.jobId;
}

export async function prepareTranscriptionAudio(
  sessionId: string
): Promise<{ path: string; format: string; merged: boolean }> {
  return invoke("session_prepare_transcription_audio", { sessionId });
}

export async function enqueueSummary(
  sessionId: string,
  templateId?: string,
  providerId?: string
): Promise<string> {
  const response = await invoke<{ jobId: string }>("summary_enqueue", {
    sessionId,
    templateId,
    providerId
  });
  return response.jobId;
}

export async function enqueueInsight(
  request: InsightQueryRequest,
  forceRefresh?: boolean
): Promise<string> {
  const response = await invoke<{ jobId: string }>("insight_enqueue", {
    request,
    forceRefresh
  });
  return response.jobId;
}

export async function getCachedInsight(request: InsightQueryRequest): Promise<InsightResult | null> {
  return invoke("insight_get_cached", { request });
}

export async function getJob(jobId: string): Promise<JobInfo> {
  return invoke("job_get", { jobId });
}

export async function getSessionJobs(sessionId: string): Promise<JobInfo[]> {
  return invoke("session_jobs", { sessionId });
}

export async function getSettings(): Promise<Settings> {
  return invoke("settings_get");
}

export async function updateSettings(settings: Partial<Settings>): Promise<Settings> {
  return invoke("settings_update", { request: settings });
}

export async function getStorageUsage(): Promise<StorageUsageSummary> {
  return invoke("settings_get_storage_usage");
}

export async function getLocalProviderStatus(): Promise<LocalProviderStatus> {
  return invoke("local_provider_status");
}

export async function prepareLocalProvider(): Promise<LocalProviderStatus> {
  return invoke("local_provider_prepare");
}
