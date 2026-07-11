import { useEffect, useId, useRef, type ReactNode } from "react";
import { Icon, type IconName } from "./Icon";
import type { ToastMessage } from "../state/app-state";

export function Button({
  children,
  icon,
  variant = "secondary",
  size = "md",
  className = "",
  disabled = false,
  title,
  type = "button",
  onClick,
}: {
  children: ReactNode;
  icon?: IconName;
  variant?: "primary" | "secondary" | "ghost" | "danger";
  size?: "sm" | "md";
  className?: string;
  disabled?: boolean;
  title?: string;
  type?: "button" | "submit";
  onClick?: (event: MouseEvent) => void;
}) {
  return (
    <button
      className={`button button-${variant} button-${size} ${className}`}
      disabled={disabled}
      onClick={onClick}
      title={title}
      type={type}
    >
      {icon && <Icon name={icon} size={size === "sm" ? 15 : 17} />}
      <span>{children}</span>
    </button>
  );
}

export function IconButton({
  icon,
  label,
  onClick,
  disabled = false,
  active = false,
  danger = false,
  size = "md",
}: {
  icon: IconName;
  label: string;
  onClick?: (event: MouseEvent) => void;
  disabled?: boolean;
  active?: boolean;
  danger?: boolean;
  size?: "sm" | "md";
}) {
  return (
    <button
      aria-label={label}
      className={`icon-button icon-button-${size} ${active ? "is-active" : ""} ${danger ? "is-danger" : ""}`}
      disabled={disabled}
      onClick={onClick}
      title={label}
      type="button"
    >
      <Icon name={icon} size={size === "sm" ? 15 : 17} />
    </button>
  );
}

export function StatusPill({
  tone,
  children,
  title,
}: {
  tone: "neutral" | "info" | "success" | "warning" | "danger";
  children: ReactNode;
  title?: string;
}) {
  return <span className={`status-pill status-${tone}`} title={title}>{children}</span>;
}

export function SegmentedControl<T extends string>({
  value,
  options,
  onChange,
  label,
}: {
  value: T;
  options: Array<{ value: T; label: string; hint?: string }>;
  onChange: (value: T) => void;
  label: string;
}) {
  return (
    <div aria-label={label} className="segmented" role="radiogroup">
      {options.map((option) => (
        <button
          aria-checked={value === option.value}
          className={value === option.value ? "is-selected" : ""}
          key={option.value}
          onClick={() => onChange(option.value)}
          role="radio"
          title={option.hint}
          type="button"
        >
          {option.label}
        </button>
      ))}
    </div>
  );
}

export function ProgressBar({ value, label }: { value: number; label: string }) {
  const safe = Math.max(0, Math.min(100, value));
  return (
    <div
      aria-label={label}
      aria-valuemax="100"
      aria-valuemin="0"
      aria-valuenow={Math.round(safe)}
      className="progress-track"
      role="progressbar"
    >
      <span className="progress-fill" style={{ transform: `scaleX(${safe / 100})` }} />
    </div>
  );
}

export function Modal({
  open,
  title,
  description,
  children,
  footer,
  size = "md",
  onClose,
}: {
  open: boolean;
  title: string;
  description?: string;
  children: ReactNode;
  footer?: ReactNode;
  size?: "sm" | "md" | "lg";
  onClose: () => void;
}) {
  const dialogRef = useRef<HTMLDialogElement | null>(null);
  const titleId = useId();
  const descriptionId = useId();

  useEffect(() => {
    const dialog = dialogRef.current;
    if (!dialog) return;
    if (open && !dialog.open) dialog.showModal();
    if (!open && dialog.open) dialog.close();
  }, [open]);

  return (
    <dialog
      aria-describedby={description ? descriptionId : undefined}
      aria-labelledby={titleId}
      className={`modal modal-${size}`}
      onCancel={(event: Event) => {
        event.preventDefault();
        onClose();
      }}
      onClose={onClose}
      ref={dialogRef}
    >
      <header className="modal-header">
        <div>
          <h2 id={titleId}>{title}</h2>
          {description && <p id={descriptionId}>{description}</p>}
        </div>
        <IconButton icon="x" label={`Close ${title}`} onClick={onClose} />
      </header>
      <div className="modal-body">{children}</div>
      {footer && <footer className="modal-footer">{footer}</footer>}
    </dialog>
  );
}

export function Drawer({
  open,
  title,
  children,
  onClose,
}: {
  open: boolean;
  title: string;
  children: ReactNode;
  onClose: () => void;
}) {
  const dialogRef = useRef<HTMLDialogElement | null>(null);
  const titleId = useId();
  useEffect(() => {
    const dialog = dialogRef.current;
    if (!dialog) return;
    if (open && !dialog.open) dialog.showModal();
    if (!open && dialog.open) dialog.close();
  }, [open]);
  return (
    <dialog
      aria-labelledby={titleId}
      className="drawer"
      onCancel={(event: Event) => {
        event.preventDefault();
        onClose();
      }}
      onClose={onClose}
      ref={dialogRef}
    >
      <header className="drawer-header">
        <h2 id={titleId}>{title}</h2>
        <IconButton icon="x" label={`Close ${title}`} onClick={onClose} />
      </header>
      <div className="drawer-body">{children}</div>
    </dialog>
  );
}

export function Field({
  label,
  hint,
  error,
  children,
}: {
  label: string;
  hint?: string;
  error?: string;
  children: ReactNode;
}) {
  return (
    <label className="field">
      <span className="field-label">{label}</span>
      {children}
      {hint && <span className="field-hint">{hint}</span>}
      {error && <span className="field-error" role="alert">{error}</span>}
    </label>
  );
}

export function Toggle({
  checked,
  label,
  description,
  onChange,
}: {
  checked: boolean;
  label: string;
  description?: string;
  onChange: (checked: boolean) => void;
}) {
  return (
    <label className="toggle-row">
      <span>
        <strong>{label}</strong>
        {description && <small>{description}</small>}
      </span>
      <input
        checked={checked}
        onChange={(event: Event) => onChange((event.currentTarget as HTMLInputElement).checked)}
        type="checkbox"
      />
    </label>
  );
}

export function ToastRegion({
  toasts,
  onDismiss,
  onAction,
}: {
  toasts: ToastMessage[];
  onDismiss: (id: string) => void;
  onAction: (toast: ToastMessage) => void;
}) {
  useEffect(() => {
    if (toasts.length === 0) return;
    const timers = toasts.map((toast) => window.setTimeout(() => onDismiss(toast.id), 5000));
    return () => timers.forEach(window.clearTimeout);
  }, [toasts, onDismiss]);

  return (
    <div aria-live="polite" aria-relevant="additions" className="toast-region">
      {toasts.map((toast) => (
        <div className={`toast toast-${toast.tone}`} key={toast.id} role="status">
          <Icon name={toast.tone === "success" ? "check" : toast.tone === "error" ? "alert" : "info"} size={17} />
          <div>
            <strong>{toast.title}</strong>
            <p>{toast.message}</p>
          </div>
          {toast.action_label && <button onClick={() => onAction(toast)} type="button">{toast.action_label}</button>}
          <IconButton icon="x" label="Dismiss notification" onClick={() => onDismiss(toast.id)} size="sm" />
        </div>
      ))}
    </div>
  );
}
