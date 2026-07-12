import { capabilityLabel } from "../lib/setup";
import type { AppSettings, EnvironmentSnapshot, SetupCapability } from "../types/contracts";
import { Icon } from "./Icon";
import { Button, IconButton } from "./ui";

export function Header({
  capability,
  settings,
  environment,
  onTheme,
  onSetup,
  onOpenOutput,
  onAbout,
  onSettings,
}: {
  capability: SetupCapability | null;
  settings: AppSettings | null;
  environment: EnvironmentSnapshot | null;
  onTheme: () => void;
  onSetup: () => void;
  onOpenOutput: () => void;
  onAbout: () => void;
  onSettings: () => void;
}) {
  const theme = settings?.theme ?? "light";
  const capabilityStatus = capability && environment ? environment.capabilities[capability] : null;
  const setupReady = capability ? Boolean(capabilityStatus?.ready) : Boolean(environment?.setup_complete);
  const setupStatus = !environment
    ? "Checking setup"
    : !capability
      ? environment.setup_complete ? "Ready" : "Review"
      : capabilityStatus?.ready
        ? "Ready"
        : `${capabilityStatus?.blockers.length ?? 1} to fix`;
  const setupTitle = !capability
    ? "Open setup center"
    : capabilityStatus?.ready
      ? `${capabilityLabel(capability)} is ready`
      : `Open setup for ${capabilityLabel(capability).toLowerCase()}`;
  return (
    <header className="app-header">
      <div className="brand-cluster">
        <div aria-hidden="true" className="brand-mark">
          <Icon name="audio-lines" size={20} />
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
        <button
          className={`setup-indicator ${setupReady ? "is-ready" : environment ? "needs-attention" : ""}`}
          onClick={onSetup}
          title={setupTitle}
          type="button"
        >
          <Icon name={setupReady ? "shield" : "wrench"} size={15} />
          <span className="setup-label">Setup</span>
          <span className="setup-status">{setupStatus}</span>
        </button>
        <Button icon="folder" onClick={onOpenOutput} size="sm" variant="ghost">
          Open output
        </Button>
        <IconButton icon="help" label="Help & About" onClick={onAbout} />
        <IconButton icon="settings" label="Settings" onClick={onSettings} />
      </nav>
    </header>
  );
}
