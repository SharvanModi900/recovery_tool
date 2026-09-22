import type { ReactNode } from "react";
import type { Step } from "../types";

export interface DataStepTab {
  key: string;
  label: string;
  icon: string;
  count: number;
  /** Optional trailing indicator, e.g. a live dot. */
  badge?: ReactNode;
}

export interface DataStepLayoutProps {
  /** Panel-bar title (emoji + text). */
  title: ReactNode;
  /** Summary hint shown after the separator. */
  hint: string;
  /** Filter input value. */
  filter: string;
  onFilterChange: (v: string) => void;
  /** Filter input placeholder. */
  filterPlaceholder?: string;
  /** Tab definitions. */
  tabs: DataStepTab[];
  /** Currently active tab key. */
  activeTab: string;
  onTabChange: (key: string) => void;
  /** Main content area — the tables / lists for the active tab. */
  children: ReactNode;
  /** Bottom navigation buttons. */
  nav?: ReactNode;
}

/**
 * Shared scaffolding for data-viewer steps (Messages, Calls, Contacts, …).
 * Renders the section > panel > panel-bar > tabs > body > nav skeleton so
 * each step only defines its data-specific content.
 */
export function DataStepLayout({
  title, hint, filter, onFilterChange, filterPlaceholder = "Filter…",
  tabs, activeTab, onTabChange, children, nav,
}: DataStepLayoutProps) {
  return (
    <section className="fx-step-section">
      <div className="panel">
        <div className="panel-bar">
          {title}
          <span className="sep" />
          <span className="hint">{hint}</span>
          <input
            type="text"
            className="fx-filter-input"
            placeholder={filterPlaceholder}
            value={filter}
            onChange={(e) => onFilterChange(e.target.value)}
          />
        </div>

        <div className="fx-tabs">
          {tabs.map((t) => (
            <button
              key={t.key}
              className={`fx-tab${activeTab === t.key ? " active" : ""}`}
              onClick={() => onTabChange(t.key)}
            >
              {t.icon} {t.label}
              {t.count > 0 && <span className="fx-tab-count">{t.count}</span>}
              {t.badge}
            </button>
          ))}
        </div>

        <div className="panel-body" style={{ overflow: "auto" }}>
          {children}
        </div>

        {nav && <div className="fx-step-nav">{nav}</div>}
      </div>
    </section>
  );
}

/* ── Reusable empty-state building blocks ─────────────────────────────── */

export function EmptyState({ icon, text, cta }: {
  icon: string;
  text: string;
  cta?: { label: string; target: Step; onGoTo: (s: Step) => void };
}) {
  return (
    <div className="fx-empty">
      <span className="fx-empty-icon">{icon}</span>
      <span className="fx-empty-text">{text}</span>
      {cta && (
        <button
          className="mini"
          style={{ marginTop: 8, border: "1px solid var(--line)", color: "var(--muted)" }}
          onClick={() => cta.onGoTo(cta.target)}
        >
          {cta.label}
        </button>
      )}
    </div>
  );
}

export function ConnectPrompt(_props: { onGoTo?: (s: Step) => void }) {
  return (
    <EmptyState icon="📱" text="Connect a device to view data" />
  );
}

/* ── Step navigation bar ─────────────────────────────────────────────── */

export interface StepNavButton {
  label: string;
  target: Step;
  variant?: "default" | "primary";
}

/**
 * Renders a row of step-navigation buttons.
 * Used by non-DataStepLayout steps (ForensicsStep, DeviceToolsStep, etc.)
 * for consistent bottom-of-panel navigation.
 */
export function StepNav({ buttons, onGoTo }: {
  buttons: StepNavButton[];
  onGoTo: (s: Step) => void;
}) {
  return (
    <div className="fx-step-nav">
      {buttons.map((b) => (
        <button
          key={b.label}
          className={b.variant === "primary" ? "primary mini" : "mini"}
          onClick={() => onGoTo(b.target)}
        >
          {b.label}
        </button>
      ))}
    </div>
  );
}
