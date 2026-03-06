import {
  useEffect,
  useRef,
  useState,
  type ChangeEvent,
  type KeyboardEvent
} from "react";
import ReactMarkdown from "react-markdown";
import rehypeSanitize from "rehype-sanitize";
import remarkGfm from "remark-gfm";

import type { Translator } from "../i18n";
import type { JobInfo, PromptTemplate, SessionDetail, SessionSummary } from "../types/domain";

type DetailTab = "transcription" | "meta" | "tasks";
type ViewMode = "readable" | "raw";
const SAFE_LINK_PROTOCOLS = new Set(["http:", "https:", "mailto:"]);

type SessionsTabProps = {
  sessions: SessionSummary[];
  templates: PromptTemplate[];
  activeSessionId?: string;
  activeSession?: SessionDetail;
  summaryTemplateId: string;
  onSummaryTemplateChange: (value: string) => void;
  sessionJobs?: JobInfo[];
  isTranscribing?: boolean;
  isSummarizing?: boolean;
  isCreatingSession?: boolean;
  onCreateSessionFromFile: (file: File) => void | Promise<void>;
  onRefresh: () => void;
  onSelectSession: (sessionId: string) => void;
  onRenameSession: (sessionId: string, name: string) => void;
  onDeleteSession: (sessionId: string) => void;
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

function normalizeSessionName(name?: string): string {
  return (name ?? "").trim();
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

function SessionsTab({
  sessions,
  templates,
  activeSessionId,
  activeSession,
  summaryTemplateId,
  onSummaryTemplateChange,
  sessionJobs = [],
  isTranscribing = false,
  isSummarizing = false,
  isCreatingSession = false,
  onCreateSessionFromFile,
  onRefresh,
  onSelectSession,
  onRenameSession,
  onDeleteSession,
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
  const [summaryViewMode, setSummaryViewMode] = useState<ViewMode>("readable");
  const [activeDetailTab, setActiveDetailTab] = useState<DetailTab>("transcription");
  const [showConfirmDelete, setShowConfirmDelete] = useState(false);
  const skipListBlurRef = useRef(false);
  const audioFileInputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (typeof window === "undefined") {
      return;
    }
    window.localStorage.setItem("sessions-list-collapsed", isListCollapsed ? "1" : "0");
  }, [isListCollapsed]);

  useEffect(() => {
    setDetailDraftName(activeSession?.name ?? "");
    setActiveDetailTab("transcription");
  }, [activeSession?.id, activeSession?.name]);

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
              <span>{sessions.length}</span>
            </header>

            {sessions.length === 0 && <p className="empty-hint">{t("sessions.empty")}</p>}

            <ul className="session-list">
              {sessions.map((session) => {
                const active = session.id === activeSessionId;
                const statusClass = session.status.toLowerCase();
                const isEditing = listEditingId === session.id;
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

                      <div className="session-item-row">
                        <div className="session-badges">
                          <span className="session-badge quality">{session.qualityPreset}</span>
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
                    onClick={onTranscribe}
                    disabled={isTranscribing}
                  >
                    {isTranscribing && <span className="spinner" />}
                    {isTranscribing ? t("status.transcriptionRunning", { elapsed: "" }) : t("sessionDetail.runTranscription")}
                  </button>
                  <button
                    type="button"
                    className={`action-btn${isSummarizing ? " loading" : ""}`}
                    onClick={onSummarize}
                    disabled={isSummarizing}
                  >
                    {isSummarizing && <span className="spinner" />}
                    {isSummarizing ? t("status.summaryRunning", { elapsed: "" }) : t("sessionDetail.generateSummary")}
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
                    <strong>{t("recorder.quality")}:</strong> {activeSession.qualityPreset}
                  </p>
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
                            <td className={`status-${job.status.toLowerCase()}`}>{job.status}</td>
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
              <div className="session-results-grid">
                <section className="panel session-result-panel">
                  <div className="session-result-header">
                    <h3>{t("sessionDetail.transcript")}</h3>
                    <div className="view-switch">
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
                  </div>

                  {transcriptViewMode === "raw" && (
                    <pre>{JSON.stringify(activeSession.transcript, null, 2)}</pre>
                  )}

                  {transcriptViewMode === "readable" && activeSession.transcript.length === 0 && (
                    <p className="empty-hint">{t("sessionDetail.transcriptEmpty")}</p>
                  )}

                  {transcriptViewMode === "readable" && activeSession.transcript.length > 0 && (
                    <ul className="transcript-list">
                      {activeSession.transcript.map((segment, index) => (
                        <li key={`${index}-${segment.startMs}-${segment.endMs}`} className="transcript-item">
                          <div className="transcript-item-header">
                            <span>
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
