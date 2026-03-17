import { useEffect, useMemo, useRef, useState } from "react";

import type { Translator } from "../i18n";
import { enqueueInsight, getCachedInsight, getJob } from "../lib/api";
import type {
  DiscoverSubView,
  InsightQueryRequest,
  InsightResult,
  InsightSelectionMode,
  InsightTimeRange,
  SessionSummary
} from "../types/domain";

type DiscoverTabProps = {
  sessions: SessionSummary[];
  t: Translator;
};

const TIME_RANGES: InsightTimeRange[] = ["1d", "2d", "3d", "1w", "1m"];
const SELECTION_MODES: InsightSelectionMode[] = ["timeRange", "sessions"];
const SUB_VIEWS: DiscoverSubView[] = ["people", "topics", "actions"];
const MAX_SELECTED_SESSIONS = 20;

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => window.setTimeout(resolve, ms));
}

function normalizeKeyword(value: string): string | undefined {
  const trimmed = value.trim();
  return trimmed.length > 0 ? trimmed : undefined;
}

function parseSortableTime(value: string): number {
  const timestamp = new Date(value).getTime();
  if (Number.isNaN(timestamp)) {
    return 0;
  }
  return timestamp;
}

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
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;
  return `${String(minutes).padStart(2, "0")}:${String(seconds).padStart(2, "0")}`;
}

function DiscoverTab({ sessions, t }: DiscoverTabProps) {
  const [selectionMode, setSelectionMode] = useState<InsightSelectionMode>("timeRange");
  const [timeRange, setTimeRange] = useState<InsightTimeRange>();
  const [selectedSessionIds, setSelectedSessionIds] = useState<string[]>([]);
  const [subView, setSubView] = useState<DiscoverSubView>("people");
  const [topicKeywordInput, setTopicKeywordInput] = useState("");
  const [topicKeyword, setTopicKeyword] = useState("");
  const [includeSuggestions, setIncludeSuggestions] = useState(false);
  const [result, setResult] = useState<InsightResult | null>(null);
  const [error, setError] = useState<string>();
  const [loadingCached, setLoadingCached] = useState(false);
  const [isRunning, setIsRunning] = useState(false);
  const [runningElapsedMs, setRunningElapsedMs] = useState(0);
  const [selectedPersonName, setSelectedPersonName] = useState<string>();
  const [isSessionPickerCollapsed, setIsSessionPickerCollapsed] = useState(false);

  const runningStartRef = useRef<number | undefined>(undefined);

  const effectiveKeyword = useMemo(() => {
    if (subView !== "topics") {
      return undefined;
    }
    return normalizeKeyword(topicKeyword);
  }, [subView, topicKeyword]);

  const discoverableSessions = useMemo(() => {
    const values = sessions.filter((session) => session.discoverable);
    values.sort((left, right) => parseSortableTime(right.updatedAt) - parseSortableTime(left.updatedAt));
    return values;
  }, [sessions]);

  const discoverableSessionIdSet = useMemo(() => {
    return new Set(discoverableSessions.map((session) => session.id));
  }, [discoverableSessions]);
  const selectedSessionIdSet = useMemo(() => new Set(selectedSessionIds), [selectedSessionIds]);
  const orderedDiscoverableSessions = useMemo(() => {
    const selected = discoverableSessions.filter((session) => selectedSessionIdSet.has(session.id));
    const unselected = discoverableSessions.filter((session) => !selectedSessionIdSet.has(session.id));
    return [...selected, ...unselected];
  }, [discoverableSessions, selectedSessionIdSet]);

  const sessionNameById = useMemo(() => {
    const map = new Map<string, string>();
    for (const session of sessions) {
      const name = session.name?.trim();
      map.set(session.id, name && name.length > 0 ? name : session.id.slice(0, 8));
    }
    return map;
  }, [sessions]);

  useEffect(() => {
    setSelectedSessionIds((previous) =>
      previous.filter((sessionId) => discoverableSessionIdSet.has(sessionId))
    );
  }, [discoverableSessionIdSet]);

  function buildInsightRequest(keyword?: string): InsightQueryRequest | undefined {
    if (selectionMode === "timeRange") {
      if (!timeRange) {
        return undefined;
      }
      return {
        selectionMode,
        timeRange,
        keyword,
        includeSuggestions
      };
    }

    const sessionIds = selectedSessionIds.filter((sessionId) => sessionId.trim().length > 0);
    if (sessionIds.length === 0 || sessionIds.length > MAX_SELECTED_SESSIONS) {
      return undefined;
    }
    return {
      selectionMode,
      sessionIds,
      keyword,
      includeSuggestions
    };
  }

  async function loadCachedInsight(request: InsightQueryRequest) {
    setLoadingCached(true);
    try {
      const cached = await getCachedInsight(request);
      setResult(cached);
      setError(undefined);
      return cached;
    } catch (loadError) {
      setError(String(loadError));
      return null;
    } finally {
      setLoadingCached(false);
    }
  }

  const cachedRequest = useMemo(
    () => buildInsightRequest(effectiveKeyword),
    [selectionMode, timeRange, selectedSessionIds, effectiveKeyword, includeSuggestions]
  );

  useEffect(() => {
    if (!cachedRequest || isRunning) {
      return;
    }
    void loadCachedInsight(cachedRequest);
  }, [cachedRequest, isRunning]);

  useEffect(() => {
    if (!result || !selectedPersonName) {
      return;
    }
    const exists = result.people.some((person) => person.name === selectedPersonName);
    if (!exists) {
      setSelectedPersonName(undefined);
    }
  }, [result, selectedPersonName]);

  async function pollInsightJob(jobId: string) {
    runningStartRef.current = Date.now();
    setRunningElapsedMs(0);

    const timeoutAt = Date.now() + 15 * 60 * 1000;
    while (Date.now() < timeoutAt) {
      const job = await getJob(jobId);
      if (job.status === "completed") {
        return;
      }
      if (job.status === "failed") {
        throw new Error(job.error || "insight job failed");
      }
      if (runningStartRef.current) {
        setRunningElapsedMs(Date.now() - runningStartRef.current);
      }
      await sleep(1000);
    }
    throw new Error("insight job polling timed out");
  }

  async function runInsight(forceRefresh: boolean, keywordOverride?: string) {
    if (isRunning) {
      return;
    }

    const runtimeKeyword =
      subView === "topics" ? normalizeKeyword(keywordOverride ?? topicKeyword) : undefined;
    const request = buildInsightRequest(runtimeKeyword);
    if (!request) {
      return;
    }

    if (forceRefresh) {
      setResult(null);
      setError(undefined);
    }
    setIsRunning(true);

    try {
      const jobId = await enqueueInsight(request, forceRefresh);
      await pollInsightJob(jobId);
      const cached = await loadCachedInsight(request);
      if (!cached) {
        throw new Error("insight result not found after job completion");
      }
    } catch (runError) {
      setError(String(runError));
    } finally {
      setIsRunning(false);
      runningStartRef.current = undefined;
      setRunningElapsedMs(0);
    }
  }

  function handleTopicSearch() {
    const keyword = normalizeKeyword(topicKeywordInput);
    setTopicKeyword(keyword ?? "");
  }

  function toggleSessionSelection(sessionId: string) {
    setSelectedSessionIds((previous) => {
      if (previous.includes(sessionId)) {
        return previous.filter((id) => id !== sessionId);
      }
      if (previous.length >= MAX_SELECTED_SESSIONS) {
        return previous;
      }
      return [...previous, sessionId];
    });
  }

  function togglePersonExpand(name: string) {
    setSelectedPersonName((previous) => (previous === name ? undefined : name));
  }

  const hasValidSelection =
    selectionMode === "timeRange"
      ? Boolean(timeRange)
      : selectedSessionIds.length > 0 && selectedSessionIds.length <= MAX_SELECTED_SESSIONS;

  const runningStatusLabel = isRunning
    ? t("status.discoverRunning", { elapsed: formatDuration(runningElapsedMs) })
    : undefined;
  const selectedPerson = result?.people.find((person) => person.name === selectedPersonName);
  const formatSuggestionSources = (sourceSessionIds: string[]) =>
    sourceSessionIds
      .map((sessionId) => sessionNameById.get(sessionId) ?? sessionId)
      .filter((value, index, values) => values.indexOf(value) === index)
      .join(", ");

  return (
    <section className="panel discover-panel">
      <header className="discover-header">
        <div className="discover-header-copy">
          <h2>{t("discover.title")}</h2>
          <p>{t("discover.subtitle")}</p>
        </div>
        <button
          type="button"
          className="discover-refresh-btn"
          onClick={() => void runInsight(true)}
          disabled={isRunning || !hasValidSelection}
        >
          {isRunning ? t("discover.refreshing") : t("discover.refresh")}
        </button>
      </header>

      <div className="discover-toolbar">
        <div className="discover-selection-mode-group" role="tablist" aria-label={t("discover.title")}>
          {SELECTION_MODES.map((item) => (
            <button
              key={item}
              type="button"
              className={`discover-selection-mode-btn${item === selectionMode ? " active" : ""}`}
              role="tab"
              aria-selected={item === selectionMode}
              onClick={() => setSelectionMode(item)}
            >
              {t(`discover.selection.mode.${item}`)}
            </button>
          ))}
        </div>

        {selectionMode === "timeRange" && (
          <div className="discover-time-range-group" role="tablist" aria-label={t("discover.title")}>
            {TIME_RANGES.map((item) => (
              <button
                key={item}
                type="button"
                className={`discover-time-range-btn${item === timeRange ? " active" : ""}`}
                role="tab"
                aria-selected={item === timeRange}
                onClick={() => {
                  setTimeRange(item);
                }}
              >
                {t(`discover.timeRange.${item}`)}
              </button>
            ))}
          </div>
        )}

        <label className="discover-option-toggle">
          <input
            type="checkbox"
            checked={includeSuggestions}
            onChange={(event) => setIncludeSuggestions(event.target.checked)}
            disabled={isRunning}
          />
          <span>{t("discover.suggestions.toggle")}</span>
        </label>
      </div>

      {selectionMode === "sessions" && (
        <div className="discover-session-picker">
          <button
            type="button"
            className="discover-session-picker-header"
            onClick={() => setIsSessionPickerCollapsed((previous) => !previous)}
          >
            <p className="discover-session-picker-caption">
              {t("discover.selection.sessions.label")}
              <span className="discover-session-picker-caption-note">
                {t("discover.selection.sessions.max", { max: String(MAX_SELECTED_SESSIONS) })}
              </span>
            </p>
            <p className="discover-session-picker-meta">
              {t("discover.selection.sessions.selected", {
                count: String(selectedSessionIds.length),
                max: String(MAX_SELECTED_SESSIONS)
              })}
              <span className="discover-session-picker-toggle">
                {isSessionPickerCollapsed
                  ? t("discover.selection.sessions.expand")
                  : t("discover.selection.sessions.collapse")}
              </span>
            </p>
          </button>
          {!isSessionPickerCollapsed && orderedDiscoverableSessions.length === 0 && (
            <p className="empty-hint">{t("discover.selection.sessions.empty")}</p>
          )}
          {!isSessionPickerCollapsed && orderedDiscoverableSessions.length > 0 && (
            <div className="discover-session-list">
              {orderedDiscoverableSessions.map((session) => {
                const isSelected = selectedSessionIdSet.has(session.id);
                const isLimitReached =
                  !isSelected && selectedSessionIds.length >= MAX_SELECTED_SESSIONS;
                return (
                  <button
                    key={session.id}
                    type="button"
                    className={`discover-session-btn${isSelected ? " active" : ""}`}
                    onClick={() => toggleSessionSelection(session.id)}
                    disabled={isRunning || isLimitReached}
                  >
                    <strong>{sessionNameById.get(session.id) ?? session.id.slice(0, 8)}</strong>
                    <span>{formatDateTime(session.updatedAt)}</span>
                  </button>
                );
              })}
            </div>
          )}
        </div>
      )}

      <div className="discover-meta-row" aria-live="polite">
        {runningStatusLabel ? (
          <span className="discover-meta-pill running">{runningStatusLabel}</span>
        ) : result ? (
          <>
            <span className="discover-meta-pill">
              {t("discover.analyzedSessions", { count: String(result.sessionIds.length) })}
            </span>
            <span className="discover-meta-pill">
              {t("discover.lastUpdated", { time: formatDateTime(result.generatedAt) })}
            </span>
          </>
        ) : loadingCached ? (
          <span className="discover-meta-pill">{t("discover.loading")}</span>
        ) : !hasValidSelection ? (
          <span className="discover-meta-pill">{t("discover.empty")}</span>
        ) : (
          <span className="discover-meta-pill">{t("discover.empty")}</span>
        )}
      </div>

      {error && <p className="danger-text">{t("discover.error", { error })}</p>}

      <div className="discover-subview-group" role="tablist" aria-label={t("discover.title")}>
        {SUB_VIEWS.map((item) => (
          <button
            key={item}
            type="button"
            className={`discover-subview-btn${item === subView ? " active" : ""}`}
            role="tab"
            aria-selected={item === subView}
            onClick={() => setSubView(item)}
          >
            {t(`discover.subview.${item}`)}
          </button>
        ))}
      </div>

      {subView === "topics" && (
        <form
          className="discover-topic-search-row"
          onSubmit={(event) => {
            event.preventDefault();
            handleTopicSearch();
          }}
        >
          <label className="discover-topic-search-label">
            {t("discover.topics.keyword")}
            <input
              value={topicKeywordInput}
              onChange={(event) => setTopicKeywordInput(event.target.value)}
              placeholder={t("discover.topics.keywordPlaceholder")}
            />
          </label>
          <button
            type="submit"
            className="btn-secondary discover-topic-search-btn"
            disabled={isRunning || !hasValidSelection}
          >
            {t("discover.topics.search")}
          </button>
        </form>
      )}

      <div className="discover-content">
        {!result && !loadingCached && <p className="empty-hint">{t("discover.empty")}</p>}

        {result && subView === "people" && (
          <div className="discover-people-view">
            <div className="discover-people-grid">
              {result.people.length === 0 && <p className="empty-hint">{t("discover.empty")}</p>}
              {result.people.map((person) => {
                const isExpanded = person.name === selectedPersonName;
                return (
                  <article
                    key={person.name}
                    className={`discover-person-card${isExpanded ? " active" : ""}`}
                    onClick={() => togglePersonExpand(person.name)}
                    role="button"
                    tabIndex={0}
                    onKeyDown={(event) => {
                      if (event.key === "Enter" || event.key === " ") {
                        event.preventDefault();
                        togglePersonExpand(person.name);
                      }
                    }}
                  >
                    <h3>{person.name}</h3>
                    <div className="discover-person-metrics">
                      <span className="discover-count-chip">
                        {t("discover.people.tasks", { count: String(person.tasks.length) })}
                      </span>
                      <span className="discover-count-chip">
                        {t("discover.people.decisions", { count: String(person.decisions.length) })}
                      </span>
                      <span className="discover-count-chip warning">
                        {t("discover.people.risks", { count: String(person.risks.length) })}
                      </span>
                      {includeSuggestions && (
                        <span className="discover-count-chip">
                          {t("discover.people.suggestions", { count: String(person.suggestions.length) })}
                        </span>
                      )}
                    </div>
                    <p className="discover-person-card-footer">
                      {isExpanded ? t("discover.people.collapse") : t("discover.people.expand")}
                    </p>
                  </article>
                );
              })}
            </div>
            {selectedPersonName && selectedPerson && (
              <div className="discover-person-expanded">
                <h3>{selectedPersonName}</h3>
                <p className="discover-person-expanded-caption">
                  {t("discover.people.tasks", { count: String(selectedPerson.tasks.length) })}
                </p>
                {selectedPerson.tasks.length === 0 && <p className="empty-hint">{t("discover.empty")}</p>}
                {selectedPerson.tasks.map((task, index) => (
                  <div key={`${task.sourceSessionId}-${index}`} className="discover-item">
                    <strong>{task.description}</strong>
                    <span>
                      ({task.status}) {t("discover.actions.source")}:{" "}
                      {sessionNameById.get(task.sourceSessionId) ?? task.sourceSessionId}
                    </span>
                  </div>
                ))}
                <p className="discover-person-expanded-caption">
                  {t("discover.people.decisions", { count: String(selectedPerson.decisions.length) })}
                </p>
                {selectedPerson.decisions.length === 0 && <p className="empty-hint">{t("discover.empty")}</p>}
                {selectedPerson.decisions.map((decision, index) => (
                  <div key={`${selectedPerson.name}-decision-${index}`} className="discover-item">
                    <strong>{decision}</strong>
                  </div>
                ))}
                <p className="discover-person-expanded-caption">
                  {t("discover.people.risks", { count: String(selectedPerson.risks.length) })}
                </p>
                {selectedPerson.risks.length === 0 && <p className="empty-hint">{t("discover.empty")}</p>}
                {selectedPerson.risks.map((risk, index) => (
                  <div key={`${selectedPerson.name}-risk-${index}`} className="discover-item">
                    <strong>{risk}</strong>
                  </div>
                ))}
                {includeSuggestions && (
                  <>
                    <p className="discover-person-expanded-caption">
                      {t("discover.people.suggestions", { count: String(selectedPerson.suggestions.length) })}
                    </p>
                    {selectedPerson.suggestions.length === 0 && (
                      <p className="empty-hint">{t("discover.suggestions.empty")}</p>
                    )}
                    {selectedPerson.suggestions.map((suggestion, index) => (
                      <div key={`${selectedPerson.name}-suggestion-${index}`} className="discover-item">
                        <strong>{suggestion.title}</strong>
                        <span>{suggestion.rationale}</span>
                        <span>
                          {t(`discover.suggestions.priority.${suggestion.priority}`)}
                          {suggestion.ownerHint
                            ? ` · ${t("discover.actions.assignee")}: ${suggestion.ownerHint}`
                            : ""}
                        </span>
                        <span>
                          {t("discover.actions.source")}: {formatSuggestionSources(suggestion.sourceSessionIds)}
                        </span>
                      </div>
                    ))}
                  </>
                )}
              </div>
            )}
          </div>
        )}

        {result && subView === "topics" && (
          <div className="discover-topic-list">
            {result.topics.length === 0 && <p className="empty-hint">{t("discover.empty")}</p>}
            {result.topics.map((topic, topicIndex) => (
              <article key={`${topic.name}-${topicIndex}`} className="discover-topic-card">
                <header>
                  <h3>{topic.name}</h3>
                  <span className={`discover-topic-status status-${topic.status}`}>
                    {t(`discover.topics.status.${topic.status}`)}
                  </span>
                </header>
                <p className="discover-topic-related">
                  {t("discover.topics.relatedPeople")}: {topic.relatedPeople.join(", ") || "-"}
                </p>
                <div className="discover-progress-list">
                  {topic.progress.map((progress, progressIndex) => (
                    <div key={`${progress.sourceSessionId}-${progressIndex}`} className="discover-item">
                      <strong>{progress.description}</strong>
                      <span>
                        {progress.date} · {sessionNameById.get(progress.sourceSessionId) ?? progress.sourceSessionId}
                      </span>
                    </div>
                  ))}
                </div>
                {includeSuggestions && (
                  <>
                    <p className="discover-person-expanded-caption">
                      {t("discover.topics.suggestions", { count: String(topic.suggestions.length) })}
                    </p>
                    {topic.suggestions.length === 0 && (
                      <p className="empty-hint">{t("discover.suggestions.empty")}</p>
                    )}
                    {topic.suggestions.map((suggestion, suggestionIndex) => (
                      <div key={`${topic.name}-suggestion-${suggestionIndex}`} className="discover-item">
                        <strong>{suggestion.title}</strong>
                        <span>{suggestion.rationale}</span>
                        <span>
                          {t(`discover.suggestions.priority.${suggestion.priority}`)}
                          {suggestion.ownerHint
                            ? ` · ${t("discover.actions.assignee")}: ${suggestion.ownerHint}`
                            : ""}
                        </span>
                        <span>
                          {t("discover.actions.source")}: {formatSuggestionSources(suggestion.sourceSessionIds)}
                        </span>
                      </div>
                    ))}
                  </>
                )}
              </article>
            ))}
          </div>
        )}

        {result && subView === "actions" && (
          <div className="discover-action-list">
            {result.upcomingActions.length === 0 && <p className="empty-hint">{t("discover.empty")}</p>}
            {result.upcomingActions.map((action, index) => (
              <article key={`${action.sourceSessionId}-${index}`} className="discover-action-card">
                <h3>{action.description}</h3>
                <div className="discover-action-meta-row">
                  <span>{t("discover.actions.assignee")}</span>
                  <strong>{action.assignee || "-"}</strong>
                </div>
                <div className="discover-action-meta-row">
                  <span>{t("discover.actions.deadline")}</span>
                  <strong>{action.deadline || "-"}</strong>
                </div>
                <div className="discover-action-meta-row">
                  <span>{t("discover.actions.source")}</span>
                  <strong>{sessionNameById.get(action.sourceSessionId) || action.sourceSessionId}</strong>
                </div>
              </article>
            ))}
          </div>
        )}
      </div>
    </section>
  );
}

export default DiscoverTab;
