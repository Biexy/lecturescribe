import type { ReactNode } from "react";

export function SettingsRow({ label, description, children }: { label: string; description?: string; children: ReactNode }) {
  return <div className="settings-row"><div className="settings-row-copy"><strong>{label}</strong>{description && <small>{description}</small>}</div><div className="settings-row-control">{children}</div></div>;
}
