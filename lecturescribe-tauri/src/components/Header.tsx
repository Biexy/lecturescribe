import type { AppSettings, EnvironmentSnapshot } from "../types/contracts";
import { Icon } from "./Icon";
import { Button, IconButton, StatusPill } from "./ui";

export function Header({
  settings,
  environment,
  onTheme,
  onSetup,
  onOpenOutput,
  onAbout,
  onSettings,
}: {
  settings: AppSettings | null;
  environment: EnvironmentSnapshot | null;
  onTheme: () => void;
  onSetup: () => void;
  onOpenOutput: () => void;
  onAbout: () => void;
  onSettings: () => void;
}) {
  const theme = settings?.theme ?? "light";
  const setupTone = !environment
    ? "neutral"
    : environment.setup_complete
      ? "success"
      : "warning";
  const setupLabel = !environment
    ? "Checking setup"
    : environment.setup_complete
      ? "Setup ready"
      : `${environment.problems.length || 1} setup issue${environment.problems.length === 1 ? "" : "s"}`;
  return (
    <header className="app-header">
      <div className="brand-cluster">
        <div aria-hidden="true" className="brand-mark">
          <Icon name="file" size={20} />
        </div>
        <div className="brand-copy">
          <h1>LectureScribe</h1>
          <p>Local audio &amp; video transcription</p>
        </div>
        <IconButton
          icon={theme === "light" ? "moon" : "sun"}
          label={theme === "light" ? "Switch to Dark mode" : "Switch to Light mode"}
          onClick={onTheme}
        />
      </div>

      <nav aria-label="Application" className="header-actions">
        <button className="setup-indicator" onClick={onSetup} title="Open setup and diagnostics" type="button">
          <StatusPill tone={setupTone}>
            <Icon name={environment?.setup_complete ? "shield" : "wrench"} size={14} />
            {setupLabel}
          </StatusPill>
        </button>
        <Button icon="folder" onClick={onOpenOutput} size="sm" variant="ghost">
          Open output
        </Button>
        <Button icon="help" onClick={onAbout} size="sm" variant="ghost">
          Help &amp; About
        </Button>
        <Button icon="settings" onClick={onSettings} size="sm" variant="ghost">
          Settings
        </Button>
      </nav>
    </header>
  );
}
