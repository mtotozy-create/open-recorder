import type { ChangeEvent } from "react";

import type { Translator } from "../i18n";
import type { PromptTemplate, SessionDetail, SessionSummary } from "../types/domain";

type SessionsTabProps = {
  sessions: SessionSummary[];
  templates: PromptTemplate[];
  activeSessionId?: string;
  activeSession?: SessionDetail;
  summaryTemplateId: string;
  summaryPromptOverride: string;
  onSummaryTemplateChange: (value: string) => void;
  onSummaryPromptChange: (value: string) => void;
  onRefresh: () => void;
  onSelectSession: (sessionId: string) => void;
  onTranscribe: () => void;
  onSummarize: () => void;
  t: Translator;
};

function SessionsTab({
  sessions,
  templates,
  activeSessionId,
  activeSession,
  summaryTemplateId,
  summaryPromptOverride,
  onSummaryTemplateChange,
  onSummaryPromptChange,
  onRefresh,
  onSelectSession,
  onTranscribe,
  onSummarize,
  t
}: SessionsTabProps) {
  function handleSummaryInput(event: ChangeEvent<HTMLTextAreaElement>) {
    onSummaryPromptChange(event.target.value);
  }

  function handleTemplateChange(event: ChangeEvent<HTMLSelectElement>) {
    onSummaryTemplateChange(event.target.value);
  }

  return (
    <div className="tab-grid">
      <section className="panel">
        <header>
          <h2>{t("sessions.title")}</h2>
          <p>{t("sessions.subtitle")}</p>
        </header>

        <button type="button" onClick={onRefresh}>
          {t("sessions.refresh")}
        </button>

        {sessions.length === 0 && <p className="empty-hint">{t("sessions.empty")}</p>}

        <ul className="session-list">
          {sessions.map((session) => {
            const active = session.id === activeSessionId;
            return (
              <li key={session.id}>
                <button
                  type="button"
                  className={`session-item${active ? " active" : ""}`}
                  onClick={() => onSelectSession(session.id)}
                >
                  <span>{session.id.slice(0, 8)}</span>
                  <span>{session.qualityPreset}</span>
                  <span>{session.status}</span>
                </button>
              </li>
            );
          })}
        </ul>
      </section>

      <section className="panel wide-panel">
        <h2>{t("sessionDetail.title")}</h2>

        {!activeSession && <p>{t("sessionDetail.noSelection")}</p>}

        {activeSession && (
          <>
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
              <strong>{t("recorder.duration")}:</strong> {Math.floor(activeSession.elapsedMs / 1000)}s
            </p>
            <p>
              <strong>{t("recorder.quality")}:</strong> {activeSession.qualityPreset}
            </p>

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

            <label>
              {t("sessionDetail.summaryPromptOverride")}
              <textarea value={summaryPromptOverride} onChange={handleSummaryInput} />
            </label>

            <h3>{t("sessionDetail.audioSegmentPaths")}</h3>
            <pre>{JSON.stringify(activeSession.audioSegments, null, 2)}</pre>

            <h3>{t("sessionDetail.transcript")}</h3>
            <pre>{JSON.stringify(activeSession.transcript, null, 2)}</pre>

            <h3>{t("sessionDetail.summary")}</h3>
            <pre>{JSON.stringify(activeSession.summary ?? null, null, 2)}</pre>
          </>
        )}
      </section>
    </div>
  );
}

export default SessionsTab;
