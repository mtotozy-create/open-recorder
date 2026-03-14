import type { TranslationKey } from "../i18n/messages";
import type { Translator } from "../i18n";
import type { KeyboardEvent } from "react";

export type AppTab = "recorder" | "sessions" | "discover" | "settings";

type TabItem = {
  id: AppTab;
  labelKey: TranslationKey;
};

type TabNavProps = {
  activeTab: AppTab;
  onChange: (tab: AppTab) => void;
  t: Translator;
};

const tabs: TabItem[] = [
  { id: "recorder", labelKey: "nav.recorder" },
  { id: "sessions", labelKey: "nav.sessions" },
  { id: "discover", labelKey: "nav.discover" },
  { id: "settings", labelKey: "nav.settings" }
];

function TabNav({ activeTab, onChange, t }: TabNavProps) {
  function handleKeyDown(event: KeyboardEvent<HTMLElement>, index: number) {
    if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") {
      return;
    }

    event.preventDefault();
    const direction = event.key === "ArrowRight" ? 1 : -1;
    const nextIndex = (index + direction + tabs.length) % tabs.length;
    onChange(tabs[nextIndex].id);
  }

  return (
    <nav className="tab-nav" aria-label="Main tabs" role="tablist">
      {tabs.map((tab, index) => {
        const active = activeTab === tab.id;
        return (
          <span
            key={tab.id}
            className={`tab-trigger${active ? " active" : ""}`}
            role="tab"
            aria-selected={active}
            tabIndex={active ? 0 : -1}
            onClick={() => onChange(tab.id)}
            onKeyDown={(event) => handleKeyDown(event, index)}
          >
            {t(tab.labelKey)}
          </span>
        );
      })}
    </nav>
  );
}

export default TabNav;
