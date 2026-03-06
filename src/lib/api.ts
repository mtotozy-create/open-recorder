import { invoke } from "@tauri-apps/api/core";
import type {
  JobInfo,
  RecorderRuntimeStatus,
  RecordingQualityPreset,
  SessionDetail,
  SessionSummary,
  Settings
} from "../types/domain";

export async function startRecording(
  inputDeviceId?: string,
  qualityPreset: RecordingQualityPreset = "standard"
): Promise<string> {
  const response = await invoke<{ sessionId: string }>("recorder_start", {
    inputDeviceId,
    qualityPreset
  });
  return response.sessionId;
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

export async function getRecorderStatus(sessionId: string): Promise<RecorderRuntimeStatus> {
  return invoke("recorder_status", {
    sessionId
  });
}

export async function exportRecording(
  sessionId: string,
  format: "wav" | "mp3"
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

export async function getSession(sessionId: string): Promise<SessionDetail> {
  return invoke("session_get", { sessionId });
}

export async function enqueueTranscription(sessionId: string): Promise<string> {
  const response = await invoke<{ jobId: string }>("transcribe_enqueue", {
    sessionId
  });
  return response.jobId;
}

export async function enqueueSummary(
  sessionId: string,
  templateId?: string,
  promptOverride?: string
): Promise<string> {
  const response = await invoke<{ jobId: string }>("summary_enqueue", {
    sessionId,
    templateId,
    promptOverride
  });
  return response.jobId;
}

export async function getJob(jobId: string): Promise<JobInfo> {
  return invoke("job_get", { jobId });
}

export async function getSettings(): Promise<Settings> {
  return invoke("settings_get");
}

export async function updateSettings(settings: Partial<Settings>): Promise<Settings> {
  return invoke("settings_update", { request: settings });
}
