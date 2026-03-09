import {
  useEffect,
  useRef,
  useState,
  type ChangeEvent,
  type KeyboardEvent,
  type MouseEvent
} from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import ReactMarkdown from "react-markdown";
import rehypeSanitize from "rehype-sanitize";
import remarkGfm from "remark-gfm";

import type { Translator } from "../i18n";
import type {
  JobInfo,
  PromptTemplate,
  RecordingQualityPreset,
  SessionDetail,
  SessionSummary
} from "../types/domain";

type DetailTab = "transcription" | "meta" | "tasks";
type ViewMode = "readable" | "raw";
type CopyStatus = "idle" | "success" | "error";
const SAFE_LINK_PROTOCOLS = new Set(["http:", "https:", "mailto:"]);
const COPY_STATUS_RESET_MS = 1800;
const MAX_SESSION_TAGS = 3;

type SessionsTabProps = {
  sessions: SessionSummary[];
  templates: PromptTemplate[];
  activeSessionId?: string;
  activeSession?: SessionDetail;
  summaryTemplateId: string;
  tagCatalog: string[];
  onSummaryTemplateChange: (value: string) => void;
  sessionJobs?: JobInfo[];
  isTranscribing?: boolean;
  isSummarizing?: boolean;
  isCreatingSession?: boolean;
  onCreateSessionFromFile: (file: File) => void | Promise<void>;
  onRefresh: () => void;
  onSelectSession: (sessionId: string) => void;
  onRenameSession: (sessionId: string, name: string) => void;
  onSetSessionTags: (sessionId: string, tags: string[]) => void | Promise<void>;
  onDeleteSession: (sessionId: string) => void;
  onPreparePlaybackAudio: () => Promise<string>;
  onExportM4a: () => void;
  onExportMp3: () => void;
  onTranscribe: () => void;
  onSummarize: () => void;
  t: Translator;
};

function formatDateTime(input: string): string {
  const date = new Date(input);
  if (Number.isNaN(date.getTime())) {
    return "";
  }
  const pad = (value: number) => String(value).padStart(2, "0");
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())} ${pad(
    date.getHours()
  )}:${pad(date.getMinutes())}`;
}

function formatDuration(ms: number): string {
  const totalSeconds = Math.floor(ms / 1000);
  const hours = Math.floor(totalSeconds / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  const seconds = totalSeconds % 60;
  const pad = (value: number) => String(value).padStart(2, "0");
  return hours > 0
    ? `${pad(hours)}:${pad(minutes)}:${pad(seconds)}`
    : `${pad(minutes)}:${pad(seconds)}`;
}

function formatFileSize(bytes: number): string {
  if (bytes === 0) return "0 B";
  const k = 1024;
  const sizes = ["B", "KB", "MB", "GB", "TB"];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return `${Number.parseFloat((bytes / k ** i).toFixed(2))} ${sizes[i]}`;
}

function formatSegmentTime(ms: number): string {
  const totalSeconds = Math.floor(ms / 1000);
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;
  return `${String(minutes).padStart(2, "0")}:${String(seconds).padStart(2, "0")}`;
}

function parseTimestampMs(value?: string): number | undefined {
  if (!value) {
    return undefined;
  }
  const parsed = Date.parse(value);
  return Number.isNaN(parsed) ? undefined : parsed;
}

function resolveRunningElapsedMs(
  nowMs: number,
  runningJob?: JobInfo,
  fallbackStartMs?: number
): number {
  const jobStartMs = parseTimestampMs(runningJob?.createdAt);
  const effectiveStartMs = jobStartMs ?? fallbackStartMs ?? nowMs;
  return Math.max(0, nowMs - effectiveStartMs);
}

function normalizeSessionName(name?: string): string {
  return (name ?? "").trim();
}

function normalizeTag(rawTag: string): string | undefined {
  const trimmed = rawTag.trim();
  if (!trimmed) {
    return undefined;
  }
  const body = trimmed.replace(/^#+/, "").trim();
  if (!body) {
    return undefined;
  }
  return `#${body.toLowerCase()}`;
}

function uniqueTags(input: string[]): string[] {
  const normalized = input
    .map((tag) => normalizeTag(tag))
    .filter((tag): tag is string => Boolean(tag));
  return Array.from(new Set(normalized));
}

function logPlaybackDebug(scope: string, payload?: Record<string, unknown>) {
  const message = `[playback-debug] ${scope}`;
  if (payload) {
    console.info(message, payload);
    return;
  }
  console.info(message);
}

function isAutoplayBlockedError(error: unknown): boolean {
  const message = String(error);
  return message.includes("NotAllowedError");
}

function resolveSinglePlayablePath(session?: SessionDetail): string | undefined {
  if (!session) {
    return undefined;
  }
  const candidates = [
    session.exportedM4aPath,
    session.exportedMp3Path,
    session.exportedWavPath,
    ...session.audioSegmentMeta.map((meta) => meta.path),
    ...session.audioSegments
  ]
    .map((path) => (path ?? "").trim())
    .filter((path) => path.length > 0);
  const uniquePaths = Array.from(new Set(candidates));
  return uniquePaths.length === 1 ? uniquePaths[0] : undefined;
}

function resolvePreferredPlaybackPath(session?: SessionDetail): string | undefined {
  if (!session) {
    return undefined;
  }
  const singlePath = resolveSinglePlayablePath(session);
  if (singlePath) {
    return singlePath;
  }
  return session.exportedM4aPath || session.exportedMp3Path || session.exportedWavPath;
}

function getDisplayName(
  session: Pick<SessionSummary, "id" | "name" | "createdAt">,
  t: Translator
): string {
  const name = normalizeSessionName(session.name);
  if (name) {
    return name;
  }
  const dateTime = formatDateTime(session.createdAt);
  if (dateTime) {
    return `${dateTime} - ${session.id.slice(0, 8)}`;
  }
  return `${t("sessionDetail.renamePlaceholder")} - ${session.id.slice(0, 8)}`;
}

function getQualityLabel(qualityPreset: RecordingQualityPreset, t: Translator): string {
  switch (qualityPreset) {
    case "voice_low_storage":
      return t("recorder.quality.voiceLowStorage");
    case "legacy_compatible":
      return t("recorder.quality.legacyCompatible");
    case "hd":
      return t("recorder.quality.hd");
    case "hifi":
      return t("recorder.quality.hifi");
    default:
      return t("recorder.quality.standard");
  }
}

function toSafeHref(rawHref?: string): string | undefined {
  if (!rawHref) {
    return undefined;
  }

  const href = rawHref.trim();
  if (!href) {
    return undefined;
  }

  if (href.startsWith("#")) {
    return href;
  }

  try {
    const parsed = new URL(href);
    if (SAFE_LINK_PROTOCOLS.has(parsed.protocol)) {
      return href;
    }
    return undefined;
  } catch {
    return undefined;
  }
}

async function copyTextToClipboard(text: string): Promise<void> {
  if (typeof navigator !== "undefined" && navigator.clipboard?.writeText) {
    await navigator.clipboard.writeText(text);
    return;
  }

  if (typeof document !== "undefined") {
    const textarea = document.createElement("textarea");
    textarea.value = text;
    textarea.style.position = "fixed";
    textarea.style.left = "-9999px";
    textarea.style.top = "0";
    textarea.setAttribute("readonly", "true");
    document.body.appendChild(textarea);
    textarea.select();
    textarea.setSelectionRange(0, textarea.value.length);
    const copied = document.execCommand("copy");
    document.body.removeChild(textarea);
    if (copied) {
      return;
    }
  }

  throw new Error("clipboard is unavailable");
}

function SessionsTab({
  sessions,
  templates,
  activeSessionId,
  activeSession,
  summaryTemplateId,
  tagCatalog,
  onSummaryTemplateChange,
  sessionJobs = [],
  isTranscribing = false,
  isSummarizing = false,
  isCreatingSession = false,
  onCreateSessionFromFile,
  onRefresh,
  onSelectSession,
  onRenameSession,
  onSetSessionTags,
  onDeleteSession,
  onPreparePlaybackAudio,
  onExportM4a,
  onExportMp3,
  onTranscribe,
  onSummarize,
  t
}: SessionsTabProps) {
  const [isListCollapsed, setIsListCollapsed] = useState<boolean>(() => {
    if (typeof window === "undefined") {
      return false;
    }
    return window.localStorage.getItem("sessions-list-collapsed") === "1";
  });
  const [listEditingId, setListEditingId] = useState<string>();
  const [listDraftName, setListDraftName] = useState<string>("");
  const [detailDraftName, setDetailDraftName] = useState<string>("");
  const [transcriptViewMode, setTranscriptViewMode] = useState<ViewMode>("readable");
  const [isTranscriptCollapsed, setIsTranscriptCollapsed] = useState<boolean>(false);
  const [summaryViewMode, setSummaryViewMode] = useState<ViewMode>("readable");
  const [activeDetailTab, setActiveDetailTab] = useState<DetailTab>("transcription");
  const [showConfirmDelete, setShowConfirmDelete] = useState(false);
  const [isPreparingPlaybackAudio, setIsPreparingPlaybackAudio] = useState(false);
  const [playbackAudioPath, setPlaybackAudioPath] = useState<string>();
  const [playbackAudioSrc, setPlaybackAudioSrc] = useState<string>();
  const [runningNowMs, setRunningNowMs] = useState<number>(() => Date.now());
  const [transcriptionStartMs, setTranscriptionStartMs] = useState<number>();
  const [summaryStartMs, setSummaryStartMs] = useState<number>();
  const [summaryCopyStatus, setSummaryCopyStatus] = useState<CopyStatus>("idle");
  const [isTagFilterExpanded, setIsTagFilterExpanded] = useState(false);
  const [selectedFilterTags, setSelectedFilterTags] = useState<string[]>([]);
  const [tagEditorSessionId, setTagEditorSessionId] = useState<string>();
  const [tagDraft, setTagDraft] = useState<string[]>([]);
  const [tagDraftInput, setTagDraftInput] = useState("");
  const [tagDraftError, setTagDraftError] = useState<string>();
  const [isSavingTags, setIsSavingTags] = useState(false);
  const skipListBlurRef = useRef(false);
  const audioFileInputRef = useRef<HTMLInputElement>(null);
  const playbackAudioRef = useRef<HTMLAudioElement>(null);
  const summaryCopyResetTimerRef = useRef<number | undefined>(undefined);

  const availableTags = uniqueTags([
    ...tagCatalog,
    ...sessions.flatMap((session) => session.tags ?? [])
  ]);
  const tagSessionCounts = new Map<string, number>();
  for (const tag of availableTags) {
    tagSessionCounts.set(tag, 0);
  }
  for (const session of sessions) {
    for (const tag of uniqueTags(session.tags ?? [])) {
      tagSessionCounts.set(tag, (tagSessionCounts.get(tag) ?? 0) + 1);
    }
  }
  const filteredSessions = selectedFilterTags.length === 0
    ? sessions
    : sessions.filter((session) =>
        (session.tags ?? []).some((tag) => selectedFilterTags.includes(tag))
      );
  const runningTranscribeJob = sessionJobs?.find((j) => j.kind === "transcription" && j.status === "running");
  const runningSummaryJob = sessionJobs?.find((j) => j.kind === "summary" && j.status === "running");
  const transcriptionElapsedLabel = formatDuration(
    resolveRunningElapsedMs(runningNowMs, runningTranscribeJob, transcriptionStartMs)
  );
  const summaryElapsedLabel = formatDuration(
    resolveRunningElapsedMs(runningNowMs, runningSummaryJob, summaryStartMs)
  );
  const transcriptionRunningLabel = runningTranscribeJob?.progressMsg
    ? `${t("status.transcriptionRunning", { elapsed: transcriptionElapsedLabel })} (${runningTranscribeJob.progressMsg})`
    : t("status.transcriptionRunning", { elapsed: transcriptionElapsedLabel });
  const summaryRunningLabel = runningSummaryJob?.progressMsg
    ? `${t("status.summaryRunning", { elapsed: summaryElapsedLabel })} (${runningSummaryJob.progressMsg})`
    : t("status.summaryRunning", { elapsed: summaryElapsedLabel });
  const summaryCopyText = activeSession?.summary?.rawMarkdown?.trim() ?? "";
  const canCopySummary = summaryViewMode === "readable" && summaryCopyText.length > 0;
  const summaryCopyLabel = summaryCopyStatus === "success"
    ? t("sessionDetail.copySummarySuccess")
    : summaryCopyStatus === "error"
      ? t("sessionDetail.copySummaryFailed")
      : t("sessionDetail.copySummary");

  useEffect(() => {
    if (typeof window === "undefined") {
      return;
    }
    window.localStorage.setItem("sessions-list-collapsed", isListCollapsed ? "1" : "0");
  }, [isListCollapsed]);

  useEffect(() => {
    setDetailDraftName(activeSession?.name ?? "");
    setActiveDetailTab("transcription");
    setIsTranscriptCollapsed(!!activeSession?.summary);
  }, [activeSession?.id, activeSession?.name, !!activeSession?.summary]);

  useEffect(() => {
    setSummaryCopyStatus("idle");
    if (summaryCopyResetTimerRef.current) {
      window.clearTimeout(summaryCopyResetTimerRef.current);
      summaryCopyResetTimerRef.current = undefined;
    }
  }, [activeSession?.id, activeSession?.summary?.rawMarkdown, summaryViewMode]);

  useEffect(() => {
    return () => {
      if (summaryCopyResetTimerRef.current) {
        window.clearTimeout(summaryCopyResetTimerRef.current);
      }
    };
  }, []);

  useEffect(() => {
    if (!isTranscribing && !isSummarizing) {
      return;
    }
    setRunningNowMs(Date.now());
    const timer = window.setInterval(() => {
      setRunningNowMs(Date.now());
    }, 1000);
    return () => window.clearInterval(timer);
  }, [isTranscribing, isSummarizing]);

  useEffect(() => {
    if (isTranscribing) {
      setTranscriptionStartMs((current) => current ?? Date.now());
      return;
    }
    setTranscriptionStartMs(undefined);
  }, [isTranscribing]);

  useEffect(() => {
    if (isSummarizing) {
      setSummaryStartMs((current) => current ?? Date.now());
      return;
    }
    setSummaryStartMs(undefined);
  }, [isSummarizing]);

  useEffect(() => {
    if (!tagEditorSessionId) {
      return;
    }
    if (sessions.some((session) => session.id === tagEditorSessionId)) {
      return;
    }
    setTagEditorSessionId(undefined);
    setTagDraft([]);
    setTagDraftInput("");
    setTagDraftError(undefined);
  }, [sessions, tagEditorSessionId]);

  useEffect(() => {
    setSelectedFilterTags((previous) => {
      const next = previous.filter((tag) => availableTags.includes(tag));
      if (next.length === previous.length && next.every((tag, index) => tag === previous[index])) {
        return previous;
      }
      return next;
    });
  }, [availableTags]);

  function handleTranscribeClick() {
    if (!isTranscribing) {
      setTranscriptionStartMs(Date.now());
    }
    onTranscribe();
  }

  function handleSummarizeClick() {
    if (!isSummarizing) {
      setSummaryStartMs(Date.now());
    }
    onSummarize();
  }

  function setAndPlayAudio(path: string) {
    try {
      const src = convertFileSrc(path);
      logPlaybackDebug("set-source", { path, src });
      setPlaybackAudioPath(path);
      setPlaybackAudioSrc(src);
      if (!playbackAudioRef.current) {
        logPlaybackDebug("audio-ref-missing", { path });
        return;
      }
      playbackAudioRef.current.src = src;
      playbackAudioRef.current.load();
      void playbackAudioRef.current.play().then(() => {
        logPlaybackDebug("play-invoked", { path });
      }).catch((error) => {
        if (isAutoplayBlockedError(error)) {
          logPlaybackDebug("play-autoplay-blocked", { path, src, error: String(error) });
          return;
        }
        console.error("[playback-debug] audio.play failed", { path, src, error: String(error) });
      });
    } catch (error) {
      console.error("[playback-debug] convertFileSrc failed", {
        path,
        error: String(error)
      });
      throw error;
    }
  }

  useEffect(() => {
    const initialPath = resolvePreferredPlaybackPath(activeSession);
    if (!initialPath) {
      logPlaybackDebug("initial-path-empty", {
        activeSessionId: activeSession?.id,
        exportedM4aPath: activeSession?.exportedM4aPath,
        exportedMp3Path: activeSession?.exportedMp3Path,
        exportedWavPath: activeSession?.exportedWavPath,
        segmentCount: activeSession?.audioSegments.length ?? 0
      });
      setPlaybackAudioPath(undefined);
      setPlaybackAudioSrc(undefined);
      return;
    }
    try {
      const src = convertFileSrc(initialPath);
      logPlaybackDebug("initial-path-resolved", {
        activeSessionId: activeSession?.id,
        initialPath,
        src
      });
      setPlaybackAudioPath(initialPath);
      setPlaybackAudioSrc(src);
    } catch (error) {
      console.error("[playback-debug] initial convertFileSrc failed", {
        activeSessionId: activeSession?.id,
        initialPath,
        error: String(error)
      });
      setPlaybackAudioPath(undefined);
      setPlaybackAudioSrc(undefined);
    }
  }, [
    activeSession?.id,
    activeSession?.exportedM4aPath,
    activeSession?.exportedMp3Path,
    activeSession?.exportedWavPath,
    activeSession?.audioSegments,
    activeSession?.audioSegmentMeta
  ]);

  function handleTemplateChange(event: ChangeEvent<HTMLSelectElement>) {
    onSummaryTemplateChange(event.target.value);
  }

  function submitDetailRename() {
    if (!activeSession) {
      return;
    }
    if (normalizeSessionName(detailDraftName) === normalizeSessionName(activeSession.name)) {
      return;
    }
    onRenameSession(activeSession.id, detailDraftName);
  }

  function startListRename(session: SessionSummary) {
    setListEditingId(session.id);
    setListDraftName(session.name ?? "");
  }

  function cancelListRename() {
    setListEditingId(undefined);
    setListDraftName("");
  }

  function submitListRename(session: SessionSummary) {
    if (normalizeSessionName(listDraftName) !== normalizeSessionName(session.name)) {
      onRenameSession(session.id, listDraftName);
    }
    cancelListRename();
  }

  function handleSessionKeyDown(event: KeyboardEvent<HTMLElement>, sessionId: string) {
    if (event.target !== event.currentTarget) {
      return;
    }
    if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      onSelectSession(sessionId);
    }
  }

  function handleAudioFileChange(event: ChangeEvent<HTMLInputElement>) {
    const file = event.target.files?.[0];
    event.target.value = "";
    if (!file) {
      return;
    }
    void onCreateSessionFromFile(file);
  }

  function toggleFilterTag(tag: string) {
    setSelectedFilterTags((previous) =>
      previous.includes(tag)
        ? previous.filter((item) => item !== tag)
        : [...previous, tag]
    );
  }

  function openTagEditor(session: SessionSummary, event: MouseEvent<HTMLButtonElement>) {
    event.stopPropagation();
    if (tagEditorSessionId === session.id) {
      setTagEditorSessionId(undefined);
      setTagDraft([]);
      setTagDraftInput("");
      setTagDraftError(undefined);
      return;
    }
    setTagEditorSessionId(session.id);
    setTagDraft(uniqueTags(session.tags ?? []).slice(0, MAX_SESSION_TAGS));
    setTagDraftInput("");
    setTagDraftError(undefined);
  }

  function toggleTagDraft(tag: string) {
    const normalized = normalizeTag(tag);
    if (!normalized) {
      return;
    }
    setTagDraftError(undefined);
    setTagDraft((previous) => {
      if (previous.includes(normalized)) {
        return previous.filter((item) => item !== normalized);
      }
      if (previous.length >= MAX_SESSION_TAGS) {
        setTagDraftError(t("sessions.tagLimit", { max: String(MAX_SESSION_TAGS) }));
        return previous;
      }
      return [...previous, normalized];
    });
  }

  function addCustomTagToDraft() {
    const normalized = normalizeTag(tagDraftInput);
    if (!normalized) {
      setTagDraftError(t("sessions.tagInvalid"));
      return;
    }
    setTagDraftError(undefined);
    setTagDraft((previous) => {
      if (previous.includes(normalized)) {
        return previous;
      }
      if (previous.length >= MAX_SESSION_TAGS) {
        setTagDraftError(t("sessions.tagLimit", { max: String(MAX_SESSION_TAGS) }));
        return previous;
      }
      return [...previous, normalized];
    });
    setTagDraftInput("");
  }

  async function saveTagDraft(sessionId: string, event: MouseEvent<HTMLButtonElement>) {
    event.stopPropagation();
    if (isSavingTags) {
      return;
    }
    if (tagDraft.length > MAX_SESSION_TAGS) {
      setTagDraftError(t("sessions.tagLimit", { max: String(MAX_SESSION_TAGS) }));
      return;
    }
    setIsSavingTags(true);
    setTagDraftError(undefined);
    try {
      await onSetSessionTags(sessionId, tagDraft);
      setTagEditorSessionId(undefined);
      setTagDraft([]);
      setTagDraftInput("");
    } catch (error) {
      setTagDraftError(String(error));
    } finally {
      setIsSavingTags(false);
    }
  }

  async function handlePreparePlaybackAudio() {
    if (isPreparingPlaybackAudio) {
      logPlaybackDebug("prepare-skipped-busy");
      return;
    }
    setIsPreparingPlaybackAudio(true);
    try {
      const currentPath = playbackAudioPath?.trim();
      if (currentPath) {
        logPlaybackDebug("prepare-branch-current-path", { currentPath });
        setAndPlayAudio(currentPath);
        return;
      }
      const preferredPath = resolvePreferredPlaybackPath(activeSession);
      if (preferredPath) {
        logPlaybackDebug("prepare-branch-preferred", { preferredPath });
        setAndPlayAudio(preferredPath);
        return;
      }
      const singlePath = resolveSinglePlayablePath(activeSession);
      logPlaybackDebug("prepare-click", {
        activeSessionId: activeSession?.id,
        singlePath,
        exportedM4aPath: activeSession?.exportedM4aPath,
        exportedMp3Path: activeSession?.exportedMp3Path,
        exportedWavPath: activeSession?.exportedWavPath,
        segmentCount: activeSession?.audioSegments.length ?? 0
      });
      if (singlePath) {
        logPlaybackDebug("prepare-branch-single", { singlePath });
        setAndPlayAudio(singlePath);
        return;
      }
      logPlaybackDebug("prepare-branch-backend");
      const path = await onPreparePlaybackAudio();
      logPlaybackDebug("prepare-backend-success", { path });
      setAndPlayAudio(path);
    } catch (error) {
      console.error("[playback-debug] prepare flow failed", {
        activeSessionId: activeSession?.id,
        error: String(error)
      });
    } finally {
      setIsPreparingPlaybackAudio(false);
    }
  }

  function queueSummaryCopyStatusReset() {
    if (summaryCopyResetTimerRef.current) {
      window.clearTimeout(summaryCopyResetTimerRef.current);
    }
    summaryCopyResetTimerRef.current = window.setTimeout(() => {
      setSummaryCopyStatus("idle");
      summaryCopyResetTimerRef.current = undefined;
    }, COPY_STATUS_RESET_MS);
  }

  async function handleCopySummary() {
    if (!canCopySummary) {
      return;
    }
    try {
      await copyTextToClipboard(summaryCopyText);
      setSummaryCopyStatus("success");
    } catch (error) {
      console.warn("[summary-copy] failed to copy summary", { error: String(error) });
      setSummaryCopyStatus("error");
    }
    queueSummaryCopyStatusReset();
  }

  const canShowCurrentShortcut = Boolean(activeSessionId);

  return (
    <div className={`sessions-layout${isListCollapsed ? " list-collapsed" : ""}`}>
      <section className={`panel sessions-sidebar${isListCollapsed ? " collapsed" : ""}`}>
        <div className="sessions-sidebar-controls">
          <button
            type="button"
            className="btn-secondary sessions-toolbar-btn sessions-collapse-btn"
            onClick={() => setIsListCollapsed((previous) => !previous)}
            aria-label={isListCollapsed ? t("sessions.showList") : t("sessions.hideList")}
            title={isListCollapsed ? t("sessions.showList") : t("sessions.hideList")}
          >
            <svg viewBox="0 0 24 24" aria-hidden="true">
              {isListCollapsed ? (
                <path d="M9 6l6 6-6 6" />
              ) : (
                <path d="M15 6l-6 6 6 6" />
              )}
            </svg>
          </button>

          {!isListCollapsed && (
            <>
              <button
                type="button"
                className="btn-secondary sessions-toolbar-btn"
                onClick={() => audioFileInputRef.current?.click()}
                disabled={isCreatingSession}
                aria-label={t("sessions.create")}
                title={t("sessions.create")}
              >
                <svg viewBox="0 0 24 24" aria-hidden="true">
                  <path d="M12 5v14M5 12h14" />
                </svg>
              </button>

              <button
                type="button"
                className="btn-secondary sessions-toolbar-btn"
                onClick={onRefresh}
                aria-label={t("sessions.refresh")}
                title={t("sessions.refresh")}
              >
                <svg viewBox="0 0 24 24" aria-hidden="true">
                  <path d="M20 12a8 8 0 10-2.34 5.66M20 12V6m0 6h-6" />
                </svg>
              </button>

              <button
                type="button"
                className="btn-secondary sessions-toolbar-btn"
                onClick={() => setShowConfirmDelete(true)}
                disabled={!activeSessionId}
                aria-label={t("sessions.delete")}
                title={t("sessions.delete")}
              >
                <svg viewBox="0 0 24 24" aria-hidden="true" style={{ stroke: "var(--danger)" }}>
                  <path d="M3 6h18M19 6v14a2 2 0 01-2 2H7a2 2 0 01-2-2V6m3 0V4a2 2 0 012-2h4a2 2 0 012 2v2M10 11v6M14 11v6" />
                </svg>
              </button>

              <input
                ref={audioFileInputRef}
                type="file"
                accept="audio/*"
                style={{ display: "none" }}
                onChange={handleAudioFileChange}
              />
            </>
          )}
        </div>

        {isListCollapsed && canShowCurrentShortcut && (
          <button
            type="button"
            className="session-current-shortcut"
            onClick={() => {
              setIsListCollapsed(false);
              onSelectSession(activeSessionId!);
            }}
            title={t("sessions.currentSession")}
          >
            {t("sessions.currentSession")}
          </button>
        )}

        {!isListCollapsed && (
          <>
            <header className="sessions-sidebar-header">
              <h2>{t("sessions.title")}</h2>
              <span>{filteredSessions.length}</span>
            </header>

            <section className="session-tag-filter">
              <button
                type="button"
                className="session-tag-filter-toggle"
                aria-expanded={isTagFilterExpanded}
                onClick={() => setIsTagFilterExpanded((previous) => !previous)}
                title={t("sessions.filterByTags")}
              >
                <span>{t("sessions.filterByTags")}</span>
                <span className="session-tag-filter-toggle-meta">
                  {selectedFilterTags.length}/{availableTags.length}
                </span>
                <svg
                  viewBox="0 0 24 24"
                  aria-hidden="true"
                  className={`session-tag-filter-toggle-icon${isTagFilterExpanded ? " expanded" : ""}`}
                >
                  <path d="M6 9l6 6 6-6" />
                </svg>
              </button>
              {isTagFilterExpanded && (
                <div className="session-tag-filter-body">
                  <div className="session-tag-filter-header">
                    <span>{t("sessions.filterByTags")}</span>
                    <button
                      type="button"
                      className="session-tag-filter-clear"
                      onClick={() => setSelectedFilterTags([])}
                      disabled={selectedFilterTags.length === 0}
                    >
                      {t("sessions.clearFilter")}
                    </button>
                  </div>
                  {availableTags.length === 0 ? (
                    <p className="session-tag-empty">{t("sessions.noTags")}</p>
                  ) : (
                    <div className="session-tag-chip-list">
                      {availableTags.map((tag) => {
                        const selected = selectedFilterTags.includes(tag);
                        return (
                          <button
                            key={`filter-${tag}`}
                            type="button"
                            className={`session-tag-filter-chip${selected ? " active" : ""}`}
                            onClick={() => toggleFilterTag(tag)}
                          >
                            <span>{tag}</span>
                            <span className="session-tag-filter-chip-count">
                              {tagSessionCounts.get(tag) ?? 0}
                            </span>
                          </button>
                        );
                      })}
                    </div>
                  )}
                </div>
              )}
            </section>

            {sessions.length === 0 && <p className="empty-hint">{t("sessions.empty")}</p>}
            {sessions.length > 0 && filteredSessions.length === 0 && (
              <p className="empty-hint">{t("sessions.filterEmpty")}</p>
            )}

            <ul className="session-list">
              {filteredSessions.map((session) => {
                const active = session.id === activeSessionId;
                const statusClass = session.status.toLowerCase();
                const isEditing = listEditingId === session.id;
                const sessionTags = uniqueTags(session.tags ?? []).slice(0, MAX_SESSION_TAGS);
                const isTagEditorOpen = tagEditorSessionId === session.id;
                return (
                  <li key={session.id}>
                    <article
                      className={`session-item${active ? " active" : ""}`}
                      role="button"
                      tabIndex={0}
                      onClick={() => onSelectSession(session.id)}
                      onKeyDown={(event) => handleSessionKeyDown(event, session.id)}
                    >
                      <div className="session-item-row">
                        {!isEditing && (
                          <span
                            className="session-item-name"
                            onDoubleClick={(event) => {
                              event.stopPropagation();
                              startListRename(session);
                            }}
                            title={getDisplayName(session, t)}
                          >
                            {getDisplayName(session, t)}
                          </span>
                        )}
                        {isEditing && (
                          <input
                            type="text"
                            className="session-name-input"
                            value={listDraftName}
                            autoFocus
                            placeholder={t("sessionDetail.renamePlaceholder")}
                            onClick={(event) => event.stopPropagation()}
                            onChange={(event) => setListDraftName(event.target.value)}
                            onBlur={() => {
                              if (skipListBlurRef.current) {
                                skipListBlurRef.current = false;
                                return;
                              }
                              submitListRename(session);
                            }}
                            onKeyDown={(event) => {
                              if (event.key === "Enter") {
                                event.preventDefault();
                                event.currentTarget.blur();
                              }
                              if (event.key === "Escape") {
                                event.preventDefault();
                                skipListBlurRef.current = true;
                                cancelListRename();
                              }
                            }}
                          />
                        )}
                      </div>

                      <div className="session-item-row">
                        <span className="session-item-time">{formatDateTime(session.createdAt)}</span>
                      </div>

                      <div className="session-item-row session-item-tags-row">
                        <button
                          type="button"
                          className="session-tag-manage-btn"
                          onClick={(event) => openTagEditor(session, event)}
                          title={t("sessions.manageTags")}
                          aria-label={t("sessions.manageTags")}
                        >
                          <svg viewBox="0 0 24 24" aria-hidden="true">
                            <path d="M12 3a2 2 0 012 2v1.1a5.5 5.5 0 012.1 1.2l.98-.56a2 2 0 012.73.73l1 1.73a2 2 0 01-.73 2.73l-.97.56a5.6 5.6 0 010 2.4l.97.56a2 2 0 01.73 2.73l-1 1.73a2 2 0 01-2.73.73l-.98-.56a5.5 5.5 0 01-2.1 1.2V19a2 2 0 11-4 0v-1.1a5.5 5.5 0 01-2.1-1.2l-.98.56a2 2 0 01-2.73-.73l-1-1.73a2 2 0 01.73-2.73l.97-.56a5.6 5.6 0 010-2.4l-.97-.56a2 2 0 01-.73-2.73l1-1.73a2 2 0 012.73-.73l.98.56a5.5 5.5 0 012.1-1.2V5a2 2 0 012-2zM12 9.5a2.5 2.5 0 100 5 2.5 2.5 0 000-5z" />
                          </svg>
                        </button>
                        <div className="session-tag-inline-list">
                          {sessionTags.length === 0 && (
                            <span className="session-tag-empty">{t("sessions.noTags")}</span>
                          )}
                          {sessionTags.map((tag) => (
                            <span key={`${session.id}-${tag}`} className="session-tag-chip">
                              {tag}
                            </span>
                          ))}
                        </div>
                      </div>

                      {isTagEditorOpen && (
                        <div
                          className="session-tag-editor"
                          onClick={(event) => event.stopPropagation()}
                        >
                          <div className="session-tag-editor-title">
                            {t("sessions.tagDraftCount", {
                              count: String(tagDraft.length),
                              max: String(MAX_SESSION_TAGS)
                            })}
                          </div>
                          <div className="session-tag-chip-list">
                            {uniqueTags([...availableTags, ...tagDraft]).map((tag) => {
                              const selected = tagDraft.includes(tag);
                              const disabled = !selected && tagDraft.length >= MAX_SESSION_TAGS;
                              return (
                                <button
                                  key={`${session.id}-edit-${tag}`}
                                  type="button"
                                  className={`session-tag-chip${selected ? " active" : ""}`}
                                  disabled={disabled}
                                  onClick={() => toggleTagDraft(tag)}
                                >
                                  {tag}
                                </button>
                              );
                            })}
                          </div>
                          <div className="session-tag-input-row">
                            <input
                              type="text"
                              value={tagDraftInput}
                              placeholder={t("sessions.addTagPlaceholder")}
                              onClick={(event) => event.stopPropagation()}
                              onChange={(event) => {
                                setTagDraftInput(event.target.value);
                                setTagDraftError(undefined);
                              }}
                              onKeyDown={(event) => {
                                if (event.key === "Enter") {
                                  event.preventDefault();
                                  addCustomTagToDraft();
                                }
                              }}
                            />
                            <button
                              type="button"
                              className="btn-secondary"
                              onClick={(event) => {
                                event.stopPropagation();
                                addCustomTagToDraft();
                              }}
                            >
                              {t("sessions.addTag")}
                            </button>
                          </div>
                          {tagDraftError && (
                            <p className="session-tag-error">{tagDraftError}</p>
                          )}
                          <div className="session-tag-editor-actions">
                            <button
                              type="button"
                              className="btn-secondary"
                              onClick={(event) => {
                                event.stopPropagation();
                                setTagEditorSessionId(undefined);
                                setTagDraft([]);
                                setTagDraftInput("");
                                setTagDraftError(undefined);
                              }}
                            >
                              {t("action.cancel")}
                            </button>
                            <button
                              type="button"
                              className="btn-primary"
                              disabled={isSavingTags}
                              onClick={(event) => {
                                void saveTagDraft(session.id, event);
                              }}
                            >
                              {t("action.confirm")}
                            </button>
                          </div>
                        </div>
                      )}

                      <div className="session-item-row">
                        <div className="session-badges">
                          <span className="session-badge quality">
                            {getQualityLabel(session.qualityPreset, t)}
                          </span>
                          <span className={`session-badge ${statusClass}`}>{session.status}</span>
                        </div>
                      </div>
                    </article>
                  </li>
                );
              })}
            </ul>
          </>
        )}
      </section>

      <section className="sessions-main">
        {!activeSession && (
          <section className="panel">
            <h2>{t("sessionDetail.title")}</h2>
            <p>{t("sessionDetail.noSelection")}</p>
          </section>
        )}

        {activeSession && (
          <>
            <section className="panel session-detail-panel">
              <div className="session-detail-tabs-header">
                <button
                  type="button"
                  className={`session-detail-tab-btn${activeDetailTab === "transcription" ? " active" : ""}`}
                  onClick={() => setActiveDetailTab("transcription")}
                >
                  {t("sessionDetail.transcriptionTab")}
                </button>
                <button
                  type="button"
                  className={`session-detail-tab-btn${activeDetailTab === "meta" ? " active" : ""}`}
                  onClick={() => setActiveDetailTab("meta")}
                >
                  {t("sessionDetail.metaTab")}
                </button>
                <button
                  type="button"
                  className={`session-detail-tab-btn${activeDetailTab === "tasks" ? " active" : ""}`}
                  onClick={() => setActiveDetailTab("tasks")}
                >
                  {t("sessionDetail.tasksTab")}
                </button>
              </div>

              {activeDetailTab === "transcription" && (
                <div className="session-actions-row">
                  <button
                    type="button"
                    className={`action-btn${isTranscribing ? " loading" : ""}`}
                    onClick={handleTranscribeClick}
                    disabled={isTranscribing}
                  >
                    {isTranscribing && <span className="spinner" />}
                    {isTranscribing ? transcriptionRunningLabel : t("sessionDetail.runTranscription")}
                  </button>
                  <button
                    type="button"
                    className={`action-btn${isSummarizing ? " loading" : ""}`}
                    onClick={handleSummarizeClick}
                    disabled={isSummarizing}
                  >
                    {isSummarizing && <span className="spinner" />}
                    {isSummarizing ? summaryRunningLabel : t("sessionDetail.generateSummary")}
                  </button>
                  <div className="summary-template-inline">
                    <span>{t("sessionDetail.summaryTemplate")}</span>
                    <select value={summaryTemplateId} onChange={handleTemplateChange}>
                      {templates.map((template) => (
                        <option key={template.id} value={template.id}>
                          {template.name}
                        </option>
                      ))}
                    </select>
                  </div>
                </div>
              )}

              {activeDetailTab === "meta" && (
                <div className="session-meta-grid">
                  <label style={{ gridColumn: "1 / -1", marginBottom: "12px" }}>
                    {t("sessionDetail.name")}
                    <input
                      type="text"
                      value={detailDraftName}
                      placeholder={t("sessionDetail.renamePlaceholder")}
                      onChange={(event) => setDetailDraftName(event.target.value)}
                      onBlur={submitDetailRename}
                      onKeyDown={(event) => {
                        if (event.key === "Enter") {
                          event.preventDefault();
                          event.currentTarget.blur();
                        }
                        if (event.key === "Escape") {
                          event.preventDefault();
                          setDetailDraftName(activeSession.name ?? "");
                        }
                      }}
                    />
                  </label>
                  <p>
                    <strong>{t("sessionDetail.id")}:</strong> {activeSession.id}
                  </p>
                  <p>
                    <strong>{t("sessionDetail.status")}:</strong> {activeSession.status}
                  </p>
                  <p>
                    <strong>{t("sessionDetail.audioSegments")}:</strong> {activeSession.audioSegments.length}
                  </p>
                  <p>
                    <strong>{t("recorder.duration")}:</strong> {formatDuration(activeSession.elapsedMs)}
                  </p>
                  <p>
                    <strong>{t("recorder.quality")}:</strong>{" "}
                    {getQualityLabel(activeSession.qualityPreset, t)}
                  </p>
                  <div className="session-meta-full-width session-meta-audio" style={{ gridColumn: "1 / -1", marginTop: "16px" }}>
                    <h4 style={{ marginBottom: "12px", color: "var(--text-primary)" }}>{t("sessionDetail.audioPlayback")}</h4>
                    <div className="session-actions-row" style={{ marginBottom: "12px" }}>
                      <button
                        type="button"
                        className={`action-btn${isPreparingPlaybackAudio ? " loading" : ""}`}
                        onClick={() => void handlePreparePlaybackAudio()}
                        disabled={isPreparingPlaybackAudio}
                      >
                        {isPreparingPlaybackAudio && <span className="spinner" />}
                        {isPreparingPlaybackAudio
                          ? t("sessionDetail.preparingAudio")
                          : t("sessionDetail.playMergedAudio")}
                      </button>
                      <button
                        type="button"
                        className="action-btn"
                        onClick={onExportM4a}
                      >
                        {t("recorder.exportM4a")}
                      </button>
                      <button
                        type="button"
                        className="action-btn"
                        onClick={onExportMp3}
                      >
                        {t("recorder.exportMp3")}
                      </button>
                    </div>
                    {playbackAudioSrc ? (
                      <>
                        <audio
                          ref={playbackAudioRef}
                          className="session-audio-player"
                          controls
                          preload="metadata"
                          src={playbackAudioSrc}
                          onError={(event) => {
                            const mediaError = event.currentTarget.error;
                            console.error("[playback-debug] audio element error", {
                              path: playbackAudioPath,
                              src: playbackAudioSrc,
                              mediaErrorCode: mediaError?.code,
                              mediaErrorMessage: mediaError?.message
                            });
                          }}
                        />
                        {playbackAudioPath && (
                          <p style={{ marginTop: "8px", wordBreak: "break-all", fontSize: "0.85em", color: "var(--text-secondary)" }}>
                            <strong>{t("sessionDetail.audioSegmentPath")}:</strong> {playbackAudioPath}
                          </p>
                        )}
                      </>
                    ) : (
                      <p className="empty-hint" style={{ marginTop: 0 }}>{t("sessionDetail.playbackEmpty")}</p>
                    )}
                  </div>
                  {activeSession.audioSegmentMeta && activeSession.audioSegmentMeta.length > 0 ? (
                    <div className="session-meta-full-width" style={{ gridColumn: "1 / -1", marginTop: "16px" }}>
                      <h4 style={{ marginBottom: "12px", color: "var(--text-primary)" }}>{t("sessionDetail.audioSegmentsDetail")}</h4>
                      <ul className="segment-meta-list" style={{ listStyle: "none", padding: 0, margin: 0, display: "grid", gap: "8px" }}>
                        {activeSession.audioSegmentMeta.map((meta, index) => (
                          <li key={`${meta.sequence}-${index}`} style={{ background: "var(--bg-secondary)", padding: "12px", borderRadius: "6px", border: "1px solid var(--border-color, #e5e7eb)" }}>
                            <p style={{ margin: "0 0 8px 0", wordBreak: "break-all", fontSize: "0.9em", color: "var(--text-primary)" }}>
                              <strong>{t("sessionDetail.audioSegmentPath")}:</strong> {meta.path}
                            </p>
                            <div style={{ display: "flex", flexWrap: "wrap", gap: "16px", fontSize: "0.85em", color: "var(--text-secondary)" }}>
                              <span><strong>{t("sessionDetail.audioSegmentSequence")}:</strong> {meta.sequence}</span>
                              <span><strong>{t("sessionDetail.audioSegmentDuration")}:</strong> {formatDuration(meta.durationMs)}</span>
                              <span><strong>{t("sessionDetail.audioSegmentSampleRate")}:</strong> {meta.sampleRate}Hz</span>
                              <span><strong>{t("sessionDetail.audioSegmentChannels")}:</strong> {meta.channels}</span>
                              {meta.fileSizeBytes !== undefined && <span><strong>{t("sessionDetail.audioSegmentFileSize")}:</strong> {formatFileSize(meta.fileSizeBytes)}</span>}
                            </div>
                          </li>
                        ))}
                      </ul>
                    </div>
                  ) : (
                    <p style={{ gridColumn: "1 / -1", wordBreak: "break-all" }}>
                      <strong>{t("sessionDetail.audioSegmentPaths")}:</strong>{" "}
                      {activeSession.audioSegments.length > 0 ? activeSession.audioSegments.join(", ") : "-"}
                    </p>
                  )}

                  {(activeSession.exportedM4aPath || activeSession.exportedMp3Path || activeSession.exportedWavPath) && (
                    <div className="session-meta-full-width" style={{ gridColumn: "1 / -1", marginTop: "16px" }}>
                      <h4 style={{ marginBottom: "12px", color: "var(--text-primary)" }}>{t("sessionDetail.mergedFile")}</h4>
                      <ul className="segment-meta-list" style={{ listStyle: "none", padding: 0, margin: 0, display: "grid", gap: "8px" }}>
                        {[
                          { path: activeSession.exportedM4aPath, size: activeSession.exportedM4aSize, createdAt: activeSession.exportedM4aCreatedAt },
                          { path: activeSession.exportedMp3Path, size: activeSession.exportedMp3Size, createdAt: activeSession.exportedMp3CreatedAt },
                          { path: activeSession.exportedWavPath, size: activeSession.exportedWavSize, createdAt: activeSession.exportedWavCreatedAt },
                        ].filter((file) => file.path).map((file, index) => (
                          <li key={`merged-${index}`} style={{ background: "var(--bg-secondary)", padding: "12px", borderRadius: "6px", border: "1px solid var(--border-color, #e5e7eb)" }}>
                            <p style={{ margin: "0 0 8px 0", wordBreak: "break-all", fontSize: "0.9em", color: "var(--text-primary)" }}>
                              <strong>{t("sessionDetail.audioSegmentPath")}:</strong> {file.path}
                            </p>
                            <div style={{ display: "flex", flexWrap: "wrap", gap: "16px", fontSize: "0.85em", color: "var(--text-secondary)" }}>
                              {file.size !== undefined && file.size > 0 && <span><strong>{t("sessionDetail.audioSegmentFileSize")}:</strong> {formatFileSize(file.size)}</span>}
                              {file.createdAt && <span><strong>{t("sessionDetail.audioSegmentCreatedAt")}:</strong> {formatDateTime(file.createdAt)}</span>}
                            </div>
                          </li>
                        ))}
                      </ul>
                    </div>
                  )}
                </div>
              )}

              {activeDetailTab === "tasks" && (
                <div className="session-tasks-list">
                  {sessionJobs.length === 0 && (
                    <p className="empty-hint">{t("sessionDetail.tasksEmpty")}</p>
                  )}
                  {sessionJobs.length > 0 && (
                    <table className="tasks-table">
                      <thead>
                        <tr>
                          <th>{t("sessionDetail.taskKind")}</th>
                          <th>{t("sessionDetail.taskStatus")}</th>
                          <th>{t("sessionDetail.taskTime")}</th>
                          <th>{t("sessionDetail.taskError")}</th>
                        </tr>
                      </thead>
                      <tbody>
                        {sessionJobs.map(job => (
                          <tr key={job.id}>
                            <td>{job.kind}</td>
                            <td className={`status-${job.status.toLowerCase()}`}>
                              {job.status}
                              {job.status === "running" && job.progressMsg ? ` (${job.progressMsg})` : ""}
                            </td>
                            <td>{formatDateTime(job.updatedAt)}</td>
                            <td className="task-error-text" title={job.error ?? ""}>
                              {job.error ?? "-"}
                            </td>
                          </tr>
                        ))}
                      </tbody>
                    </table>
                  )}
                </div>
              )}
            </section>

            {activeDetailTab === "transcription" && (
              <div className={`session-results-grid ${isTranscriptCollapsed ? "transcript-collapsed" : ""}`}>
                <section className={`panel session-result-panel ${isTranscriptCollapsed ? "collapsed" : ""}`}>
                  <div
                    className="session-result-header"
                    style={{ cursor: "pointer", userSelect: "none" }}
                    onClick={() => setIsTranscriptCollapsed(!isTranscriptCollapsed)}
                  >
                    <div style={{ display: "flex", alignItems: "center", gap: "8px" }}>
                      <svg
                        viewBox="0 0 24 24"
                        aria-hidden="true"
                        style={{
                          width: "20px",
                          height: "20px",
                          stroke: "currentColor",
                          strokeWidth: 2,
                          strokeLinecap: "round",
                          strokeLinejoin: "round",
                          fill: "none",
                          transition: "transform 200ms ease",
                          transform: isTranscriptCollapsed ? "rotate(-90deg)" : "rotate(0deg)"
                        }}
                      >
                        <polyline points="6 9 12 15 18 9"></polyline>
                      </svg>
                      <h3 style={{ margin: 0 }}>{t("sessionDetail.transcript")}</h3>
                    </div>

                    {!isTranscriptCollapsed && (
                      <div className="view-switch" onClick={(e) => e.stopPropagation()}>
                        <button
                          type="button"
                          className={`view-switch-btn${transcriptViewMode === "readable" ? " active" : ""}`}
                          onClick={() => setTranscriptViewMode("readable")}
                        >
                          {t("sessionDetail.readableView")}
                        </button>
                        <button
                          type="button"
                          className={`view-switch-btn${transcriptViewMode === "raw" ? " active" : ""}`}
                          onClick={() => setTranscriptViewMode("raw")}
                        >
                          {t("sessionDetail.rawView")}
                        </button>
                      </div>
                    )}
                  </div>

                  {!isTranscriptCollapsed && transcriptViewMode === "raw" && (
                    <pre>{JSON.stringify(activeSession.transcript, null, 2)}</pre>
                  )}

                  {!isTranscriptCollapsed && transcriptViewMode === "readable" && activeSession.transcript.length === 0 && (
                    <p className="empty-hint">{t("sessionDetail.transcriptEmpty")}</p>
                  )}

                  {!isTranscriptCollapsed && transcriptViewMode === "readable" && activeSession.transcript.length > 0 && (
                    <ul className="transcript-list">
                      {activeSession.transcript.map((segment, index) => (
                        <li key={`${index}-${segment.startMs}-${segment.endMs}`} className="transcript-item">
                          <div className="transcript-item-header">
                            <span>
                              {segment.speakerLabel ? `${segment.speakerLabel} · ` : ""}
                              {formatSegmentTime(segment.startMs)} - {formatSegmentTime(segment.endMs)}
                            </span>
                            {typeof segment.confidence === "number" && (
                              <span>{Math.round(segment.confidence * 100)}%</span>
                            )}
                          </div>
                          <p style={{ whiteSpace: "pre-wrap", wordBreak: "break-word" }}>{segment.text}</p>
                        </li>
                      ))}
                    </ul>
                  )}
                </section>

                <section className="panel session-result-panel">
                  <div className="session-result-header">
                    <h3>{t("sessionDetail.summary")}</h3>
                    <div className="session-result-actions">
                      {summaryViewMode === "readable" && (
                        <button
                          type="button"
                          className={`summary-copy-btn${summaryCopyStatus === "success" ? " success" : ""}${summaryCopyStatus === "error" ? " error" : ""}`}
                          onClick={() => void handleCopySummary()}
                          disabled={!canCopySummary}
                          aria-label={summaryCopyLabel}
                          title={summaryCopyLabel}
                        >
                          <svg viewBox="0 0 24 24" aria-hidden="true">
                            {summaryCopyStatus === "success" ? (
                              <path d="M20 7l-11 11-5-5" />
                            ) : summaryCopyStatus === "error" ? (
                              <path d="M18 6L6 18M6 6l12 12" />
                            ) : (
                              <path d="M9 9V4a1 1 0 011-1h9a1 1 0 011 1v11a1 1 0 01-1 1h-5M9 9H5a1 1 0 00-1 1v10a1 1 0 001 1h9a1 1 0 001-1v-4M9 9h6a1 1 0 011 1v6" />
                            )}
                          </svg>
                          <span>{summaryCopyLabel}</span>
                        </button>
                      )}
                      <div className="view-switch">
                        <button
                          type="button"
                          className={`view-switch-btn${summaryViewMode === "readable" ? " active" : ""}`}
                          onClick={() => setSummaryViewMode("readable")}
                        >
                          {t("sessionDetail.readableView")}
                        </button>
                        <button
                          type="button"
                          className={`view-switch-btn${summaryViewMode === "raw" ? " active" : ""}`}
                          onClick={() => setSummaryViewMode("raw")}
                        >
                          {t("sessionDetail.rawView")}
                        </button>
                      </div>
                    </div>
                  </div>

                  {summaryViewMode === "raw" && (
                    <pre>{JSON.stringify(activeSession.summary ?? null, null, 2)}</pre>
                  )}

                  {summaryViewMode === "readable" && !activeSession.summary && (
                    <p className="empty-hint">{t("sessionDetail.summaryEmpty")}</p>
                  )}

                  {summaryViewMode === "readable" && activeSession.summary && (
                    <div className="summary-markdown-view">
                      {activeSession.summary.rawMarkdown.trim() ? (
                        <ReactMarkdown
                          skipHtml
                          remarkPlugins={[remarkGfm]}
                          rehypePlugins={[rehypeSanitize]}
                          components={{
                            a: ({ href, children, ...props }) => {
                              const safeHref = toSafeHref(href);
                              if (!safeHref) {
                                return <span className="md-link-disabled">{children}</span>;
                              }
                              return (
                                <a
                                  {...props}
                                  href={safeHref}
                                  target="_blank"
                                  rel="noopener noreferrer"
                                >
                                  {children}
                                </a>
                              );
                            }
                          }}
                        >
                          {activeSession.summary.rawMarkdown}
                        </ReactMarkdown>
                      ) : (
                        <p className="empty-hint">-</p>
                      )}
                    </div>
                  )}
                </section>
              </div>
            )}
          </>
        )}
      </section>

      {showConfirmDelete && (
        <div className="modal-overlay">
          <div className="modal-content panel">
            <h3>{t("sessions.delete")}</h3>
            <p>{t("sessions.deleteConfirm")}</p>
            <div className="modal-actions">
              <button
                type="button"
                className="btn-secondary"
                onClick={() => setShowConfirmDelete(false)}
              >
                {t("action.cancel")}
              </button>
              <button
                type="button"
                className="btn-primary"
                style={{ background: "var(--danger)", color: "#fff" }}
                onClick={() => {
                  setShowConfirmDelete(false);
                  if (activeSessionId) onDeleteSession(activeSessionId);
                }}
              >
                {t("action.confirm")}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

export default SessionsTab;
