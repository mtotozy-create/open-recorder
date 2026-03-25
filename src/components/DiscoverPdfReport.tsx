import type { Translator } from "../i18n";
import type {
  InsightAction,
  InsightResult,
  InsightSelectionMode,
  InsightTimeRange
} from "../types/domain";

type DiscoverPdfReportProps = {
  result: InsightResult;
  selectionMode: InsightSelectionMode;
  timeRange?: InsightTimeRange;
  keyword?: string;
  includeSuggestions: boolean;
  sessionNameById: Map<string, string>;
  t: Translator;
};

const MAX_SOURCE_PREVIEW_ITEMS = 6;

function formatDateTime(input: string): string {
  const date = new Date(input);
  if (Number.isNaN(date.getTime())) {
    return "-";
  }
  const pad = (value: number) => String(value).padStart(2, "0");
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())} ${pad(
    date.getHours()
  )}:${pad(date.getMinutes())}`;
}

function parseSortableDate(value?: string): number {
  if (!value) {
    return 0;
  }
  const timestamp = new Date(value).getTime();
  return Number.isNaN(timestamp) ? 0 : timestamp;
}

function buildSourceSessionName(sessionId: string, sessionNameById: Map<string, string>): string {
  return sessionNameById.get(sessionId) ?? sessionId.slice(0, 8);
}

function sortActions(actions: InsightAction[]): InsightAction[] {
  return [...actions].sort((left, right) => {
    const leftHasDeadline = Boolean(left.deadline?.trim());
    const rightHasDeadline = Boolean(right.deadline?.trim());
    if (leftHasDeadline !== rightHasDeadline) {
      return leftHasDeadline ? -1 : 1;
    }

    const deadlineDiff = parseSortableDate(right.deadline) - parseSortableDate(left.deadline);
    if (deadlineDiff !== 0) {
      return deadlineDiff;
    }

    return parseSortableDate(right.sourceDate) - parseSortableDate(left.sourceDate);
  });
}

function getRangeLabel(
  selectionMode: InsightSelectionMode,
  timeRange: InsightTimeRange | undefined,
  result: InsightResult,
  t: Translator
): string {
  if (selectionMode === "sessions" || result.timeRangeType === "sessions") {
    return t("discover.report.sessionsMode");
  }
  const effectiveRange = timeRange ?? result.timeRangeType;
  return t(`discover.timeRange.${effectiveRange as InsightTimeRange}`);
}

function getSuggestionCount(result: InsightResult): number {
  const peopleSuggestions = result.people.reduce((sum, person) => sum + person.suggestions.length, 0);
  const topicSuggestions = result.topics.reduce((sum, topic) => sum + topic.suggestions.length, 0);
  return peopleSuggestions + topicSuggestions;
}

function renderEmptyState(t: Translator) {
  return <p className="discover-report-empty">{t("discover.empty")}</p>;
}

function DiscoverPdfReport({
  result,
  selectionMode,
  timeRange,
  keyword,
  includeSuggestions,
  sessionNameById,
  t
}: DiscoverPdfReportProps) {
  const sourceSessionNames = result.sessionIds.map((sessionId) =>
    buildSourceSessionName(sessionId, sessionNameById)
  );
  const sourcePreview = sourceSessionNames.slice(0, MAX_SOURCE_PREVIEW_ITEMS);
  const hiddenSourceCount = Math.max(0, sourceSessionNames.length - sourcePreview.length);
  const suggestionCount = includeSuggestions ? getSuggestionCount(result) : 0;
  const sortedActions = sortActions(result.upcomingActions);
  const metadataItems = [
    {
      label: t("discover.report.generatedAt"),
      value: formatDateTime(result.generatedAt)
    },
    {
      label: t("discover.report.selection"),
      value: t(`discover.selection.mode.${selectionMode}`)
    },
    {
      label: t("discover.report.range"),
      value: getRangeLabel(selectionMode, timeRange, result, t)
    },
    {
      label: t("discover.report.sourceSessions"),
      value: String(result.sessionIds.length)
    },
    {
      label: t("discover.report.keyword"),
      value: keyword?.trim() ? keyword.trim() : "-"
    },
    {
      label: t("discover.report.suggestionsIncluded"),
      value: includeSuggestions ? t("settings.option.enabled") : t("settings.option.disabled")
    }
  ];
  const summaryMetrics = [
    {
      label: t("discover.report.sessionCount"),
      value: String(result.sessionIds.length)
    },
    {
      label: t("discover.report.peopleCount"),
      value: String(result.people.length)
    },
    {
      label: t("discover.report.topicsCount"),
      value: String(result.topics.length)
    },
    {
      label: t("discover.report.actionsCount"),
      value: String(result.upcomingActions.length)
    }
  ];
  if (includeSuggestions) {
    summaryMetrics.push({
      label: t("discover.report.suggestionsCount"),
      value: String(suggestionCount)
    });
  }

  return (
    <div className="discover-print-root">
      <section className="discover-report-hero">
        <p className="discover-report-kicker">{t("discover.title")}</p>
        <h1>{t("discover.report.title")}</h1>
        <p className="discover-report-subtitle">{t("discover.subtitle")}</p>

        <div className="discover-report-meta-grid">
          {metadataItems.map((item) => (
            <div key={item.label} className="discover-report-meta-card">
              <span>{item.label}</span>
              <strong>{item.value}</strong>
            </div>
          ))}
        </div>

        <div className="discover-report-source-preview">
          <p className="discover-report-section-label">{t("discover.report.sources")}</p>
          <div className="discover-report-pill-row">
            {sourcePreview.map((name) => (
              <span key={name} className="discover-report-pill">
                {name}
              </span>
            ))}
            {hiddenSourceCount > 0 && (
              <span className="discover-report-pill muted">
                {t("discover.report.moreSessions", { count: String(hiddenSourceCount) })}
              </span>
            )}
          </div>
        </div>
      </section>

      <section className="discover-report-section">
        <header className="discover-report-section-header">
          <p>{t("discover.meta.generatedAt", { time: formatDateTime(result.generatedAt) })}</p>
          <h2>{t("discover.report.executiveSummary")}</h2>
        </header>
        <div className="discover-report-summary-grid">
          {summaryMetrics.map((metric) => (
            <article key={metric.label} className="discover-report-summary-card">
              <strong>{metric.value}</strong>
              <span>{metric.label}</span>
            </article>
          ))}
        </div>
      </section>

      <section className="discover-report-section">
        <header className="discover-report-section-header">
          <p>{t("discover.report.sectionLabel")}</p>
          <h2>{t("discover.subview.people")}</h2>
        </header>
        <div className="discover-report-card-list">
          {result.people.length === 0 && renderEmptyState(t)}
          {result.people.map((person) => (
            <article key={person.name} className="discover-report-card">
              <header className="discover-report-card-header">
                <div>
                  <h3>{person.name}</h3>
                  <div className="discover-report-chip-row">
                    <span className="discover-report-chip">
                      {t("discover.people.tasks", { count: String(person.tasks.length) })}
                    </span>
                    <span className="discover-report-chip">
                      {t("discover.people.decisions", { count: String(person.decisions.length) })}
                    </span>
                    <span className="discover-report-chip warning">
                      {t("discover.people.risks", { count: String(person.risks.length) })}
                    </span>
                    {includeSuggestions && (
                      <span className="discover-report-chip">
                        {t("discover.people.suggestions", { count: String(person.suggestions.length) })}
                      </span>
                    )}
                  </div>
                </div>
              </header>

              {person.tasks.length > 0 && (
                <div className="discover-report-group">
                  <h4>{t("discover.report.tasksHeading")}</h4>
                  <div className="discover-report-item-list">
                    {person.tasks.map((task, index) => (
                      <div key={`${person.name}-task-${index}`} className="discover-report-item">
                        <strong>{task.description}</strong>
                        <span>
                          {task.status} · {t("discover.actions.source")}:{" "}
                          {buildSourceSessionName(task.sourceSessionId, sessionNameById)}
                        </span>
                      </div>
                    ))}
                  </div>
                </div>
              )}

              {person.decisions.length > 0 && (
                <div className="discover-report-group">
                  <h4>{t("sessionDetail.decisions")}</h4>
                  <div className="discover-report-item-list">
                    {person.decisions.map((decision, index) => (
                      <div key={`${person.name}-decision-${index}`} className="discover-report-item">
                        <strong>{decision}</strong>
                      </div>
                    ))}
                  </div>
                </div>
              )}

              {person.risks.length > 0 && (
                <div className="discover-report-group">
                  <h4>{t("sessionDetail.risks")}</h4>
                  <div className="discover-report-item-list">
                    {person.risks.map((risk, index) => (
                      <div key={`${person.name}-risk-${index}`} className="discover-report-item">
                        <strong>{risk}</strong>
                      </div>
                    ))}
                  </div>
                </div>
              )}

              {includeSuggestions && person.suggestions.length > 0 && (
                <div className="discover-report-group">
                  <h4>{t("discover.report.suggestionsHeading")}</h4>
                  <div className="discover-report-item-list">
                    {person.suggestions.map((suggestion, index) => (
                      <div key={`${person.name}-suggestion-${index}`} className="discover-report-item">
                        <strong>{suggestion.title}</strong>
                        <span>{suggestion.rationale}</span>
                        <span>
                          {t(`discover.suggestions.priority.${suggestion.priority}`)}
                          {suggestion.ownerHint
                            ? ` · ${t("discover.actions.assignee")}: ${suggestion.ownerHint}`
                            : ""}
                        </span>
                        <span>
                          {t("discover.actions.source")}:{" "}
                          {suggestion.sourceSessionIds
                            .map((sessionId) => buildSourceSessionName(sessionId, sessionNameById))
                            .filter((value, index2, values) => values.indexOf(value) === index2)
                            .join(", ")}
                        </span>
                      </div>
                    ))}
                  </div>
                </div>
              )}
            </article>
          ))}
        </div>
      </section>

      <section className="discover-report-section">
        <header className="discover-report-section-header">
          <p>{t("discover.report.sectionLabel")}</p>
          <h2>{t("discover.subview.topics")}</h2>
        </header>
        <div className="discover-report-card-list">
          {result.topics.length === 0 && renderEmptyState(t)}
          {result.topics.map((topic, topicIndex) => (
            <article key={`${topic.name}-${topicIndex}`} className="discover-report-card">
              <header className="discover-report-card-header">
                <div>
                  <h3>{topic.name}</h3>
                  <p className="discover-report-inline-meta">
                    {t("discover.topics.relatedPeople")}: {topic.relatedPeople.join(", ") || "-"}
                  </p>
                </div>
                <span className={`discover-report-status status-${topic.status}`}>
                  {t(`discover.topics.status.${topic.status}`)}
                </span>
              </header>

              <div className="discover-report-group">
                <h4>{t("discover.topics.progress")}</h4>
                <div className="discover-report-item-list">
                  {topic.progress.length === 0 && renderEmptyState(t)}
                  {topic.progress.map((progress, progressIndex) => (
                    <div key={`${topic.name}-progress-${progressIndex}`} className="discover-report-item">
                      <strong>{progress.description}</strong>
                      <span>
                        {progress.date} ·{" "}
                        {buildSourceSessionName(progress.sourceSessionId, sessionNameById)}
                      </span>
                    </div>
                  ))}
                </div>
              </div>

              {includeSuggestions && topic.suggestions.length > 0 && (
                <div className="discover-report-group">
                  <h4>{t("discover.report.suggestionsHeading")}</h4>
                  <div className="discover-report-item-list">
                    {topic.suggestions.map((suggestion, suggestionIndex) => (
                      <div key={`${topic.name}-suggestion-${suggestionIndex}`} className="discover-report-item">
                        <strong>{suggestion.title}</strong>
                        <span>{suggestion.rationale}</span>
                        <span>
                          {t(`discover.suggestions.priority.${suggestion.priority}`)}
                          {suggestion.ownerHint
                            ? ` · ${t("discover.actions.assignee")}: ${suggestion.ownerHint}`
                            : ""}
                        </span>
                        <span>
                          {t("discover.actions.source")}:{" "}
                          {suggestion.sourceSessionIds
                            .map((sessionId) => buildSourceSessionName(sessionId, sessionNameById))
                            .filter((value, index2, values) => values.indexOf(value) === index2)
                            .join(", ")}
                        </span>
                      </div>
                    ))}
                  </div>
                </div>
              )}
            </article>
          ))}
        </div>
      </section>

      <section className="discover-report-section">
        <header className="discover-report-section-header">
          <p>{t("discover.report.sectionLabel")}</p>
          <h2>{t("discover.subview.actions")}</h2>
        </header>
        <div className="discover-report-card-list">
          {sortedActions.length === 0 && renderEmptyState(t)}
          {sortedActions.map((action, index) => (
            <article key={`${action.sourceSessionId}-${index}`} className="discover-report-card compact">
              <h3>{action.description}</h3>
              <div className="discover-report-definition-grid">
                <div>
                  <span>{t("discover.actions.assignee")}</span>
                  <strong>{action.assignee || "-"}</strong>
                </div>
                <div>
                  <span>{t("discover.actions.deadline")}</span>
                  <strong>{action.deadline || "-"}</strong>
                </div>
                <div>
                  <span>{t("discover.actions.source")}</span>
                  <strong>{buildSourceSessionName(action.sourceSessionId, sessionNameById)}</strong>
                </div>
              </div>
            </article>
          ))}
        </div>
      </section>

      <section className="discover-report-section">
        <header className="discover-report-section-header">
          <p>{t("discover.report.sectionLabel")}</p>
          <h2>{t("discover.report.sources")}</h2>
        </header>
        <div className="discover-report-source-list">
          {sourceSessionNames.length === 0 && renderEmptyState(t)}
          {sourceSessionNames.map((name, index) => (
            <div key={`${name}-${index}`} className="discover-report-source-item">
              <span>{String(index + 1).padStart(2, "0")}</span>
              <strong>{name}</strong>
            </div>
          ))}
        </div>
      </section>
    </div>
  );
}

export default DiscoverPdfReport;
