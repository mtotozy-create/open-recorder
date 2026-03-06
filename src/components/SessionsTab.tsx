import {
  useEffect,
  useRef,
  useState,
  type ChangeEvent,
  type KeyboardEvent,
  type ReactNode
} from "react";

import type { Translator } from "../i18n";
import type { PromptTemplate, SessionDetail, SessionSummary } from "../types/domain";

type ViewMode = "readable" | "raw";
type MarkdownBlock =
  | { kind: "heading"; level: 1 | 2 | 3; text: string }
  | { kind: "paragraph"; text: string }
  | { kind: "list"; items: string[] };

type SessionsTabProps = {
  sessions: SessionSummary[];
  templates: PromptTemplate[];
  activeSessionId?: string;
  activeSession?: SessionDetail;
  summaryTemplateId: string;
  onSummaryTemplateChange: (value: string) => void;
  onRefresh: () => void;
  onSelectSession: (sessionId: string) => void;
  onRenameSession: (sessionId: string, name: string) => void;
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

function parseSimpleMarkdown(markdown: string): MarkdownBlock[] {
  const blocks: MarkdownBlock[] = [];
  const lines = markdown.replace(/\r\n/g, "\n").split("\n");
  let paragraphLines: string[] = [];
  let listItems: string[] = [];

  function flushParagraph() {
    if (paragraphLines.length === 0) {
      return;
    }
    blocks.push({
      kind: "paragraph",
      text: paragraphLines.join(" ").trim()
    });
    paragraphLines = [];
  }

  function flushList() {
    if (listItems.length === 0) {
      return;
    }
    blocks.push({ kind: "list", items: listItems });
    listItems = [];
  }

  for (const rawLine of lines) {
    const line = rawLine.trim();
    if (!line) {
      flushParagraph();
      flushList();
      continue;
    }

    const heading = line.match(/^(#{1,3})\s+(.*)$/);
    if (heading && heading[2].trim()) {
      flushParagraph();
      flushList();
      const level = heading[1].length as 1 | 2 | 3;
      blocks.push({ kind: "heading", level, text: heading[2].trim() });
      continue;
    }

    const unorderedListItem = line.match(/^[-*+]\s+(.*)$/);
    const orderedListItem = line.match(/^\d+\.\s+(.*)$/);
    const listText = unorderedListItem?.[1] ?? orderedListItem?.[1];
    if (listText && listText.trim()) {
      flushParagraph();
      listItems.push(listText.trim());
      continue;
    }

    flushList();
    paragraphLines.push(line);
  }

  flushParagraph();
  flushList();

  return blocks;
}

function renderSimpleMarkdown(markdown: string): ReactNode {
  const blocks = parseSimpleMarkdown(markdown);
  if (blocks.length === 0) {
    return <p className="empty-hint">{markdown.trim() || "-"}</p>;
  }

  return blocks.map((block, index) => {
    if (block.kind === "heading") {
      if (block.level === 1) {
        return <h4 key={`h1-${index}`}>{block.text}</h4>;
      }
      if (block.level === 2) {
        return <h5 key={`h2-${index}`}>{block.text}</h5>;
      }
      return <h6 key={`h3-${index}`}>{block.text}</h6>;
    }
    if (block.kind === "list") {
      return (
        <ul key={`ul-${index}`}>
          {block.items.map((item, itemIndex) => (
            <li key={`li-${index}-${itemIndex}`}>{item}</li>
          ))}
        </ul>
      );
    }
    return <p key={`p-${index}`}>{block.text}</p>;
  });
}

function SessionsTab({
  sessions,
  templates,
  activeSessionId,
  activeSession,
  summaryTemplateId,
  onSummaryTemplateChange,
  onRefresh,
  onSelectSession,
  onRenameSession,
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
  const skipListBlurRef = useRef(false);

  useEffect(() => {
    if (typeof window === "undefined") {
      return;
    }
    window.localStorage.setItem("sessions-list-collapsed", isListCollapsed ? "1" : "0");
  }, [isListCollapsed]);

  useEffect(() => {
    setDetailDraftName(activeSession?.name ?? "");
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
              <h2>{t("sessionDetail.title")}</h2>

              <label>
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

              <div className="session-meta-grid">
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
                <p>
                  <strong>{t("sessionDetail.audioSegmentPaths")}:</strong>{" "}
                  {activeSession.audioSegments.length > 0 ? activeSession.audioSegments[0] : "-"}
                </p>
              </div>

              <div className="button-grid">
                <button type="button" onClick={onTranscribe}>
                  {t("sessionDetail.runTranscription")}
                </button>
                <button type="button" onClick={onSummarize}>
                  {t("sessionDetail.generateSummary")}
                </button>
              </div>

              <label>
                {t("sessionDetail.summaryTemplate")}
                <select value={summaryTemplateId} onChange={handleTemplateChange}>
                  {templates.map((template) => (
                    <option key={template.id} value={template.id}>
                      {template.name} ({template.id})
                    </option>
                  ))}
                </select>
              </label>
            </section>

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
                        <p>{segment.text}</p>
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
                    {renderSimpleMarkdown(activeSession.summary.rawMarkdown)}
                  </div>
                )}
              </section>
            </div>
          </>
        )}
      </section>
    </div>
  );
}

export default SessionsTab;
