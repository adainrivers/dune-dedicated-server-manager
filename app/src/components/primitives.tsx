import { CheckCircle2, MinusCircle, XCircle } from "lucide-react";

export function statusTone(value?: string | boolean | null) {
  const text = String(value ?? "").toLowerCase();
  if (
    value === true ||
    [
      "running",
      "ready",
      "healthy",
      "available",
      "connected",
      "online",
      "operating normally",
      "active",
      "succeeded",
      "ok"
    ].includes(text)
  ) {
    return "good";
  }
  if (value === false || ["stopped", "suspended", "disabled", "offline", "error", "failed"].includes(text)) {
    return "bad";
  }
  return "warn";
}

export function StatusPill({ value }: { value?: string | boolean | null }) {
  const label = typeof value === "boolean" ? (value ? "Yes" : "No") : value || "Unknown";
  return <span className={`pill ${statusTone(value)}`}>{label}</span>;
}

export function StatusLamp({ value, label }: { value?: string | boolean | null; label: string }) {
  const tone = statusTone(value);
  const Icon = tone === "good" ? CheckCircle2 : tone === "bad" ? XCircle : MinusCircle;
  const text = typeof value === "boolean" ? (value ? "Ready" : "Unavailable") : value || "Unknown";
  return (
    <span className={`status-lamp ${tone}`} title={`${label}: ${text}`} aria-label={`${label}: ${text}`}>
      <Icon size={18} />
    </span>
  );
}

export function InfoRow({ label, value }: { label: string; value?: string | number | null }) {
  return (
    <div className="info-row">
      <span>{label}</span>
      <strong>{value || "Unknown"}</strong>
    </div>
  );
}

export function StatusInfoRow({ label, value }: { label: string; value?: string | boolean | null }) {
  return (
    <div className="info-row">
      <span>{label}</span>
      <strong>
        <StatusPill value={value} />
      </strong>
    </div>
  );
}

export function EmptyState({ text }: { text: string }) {
  return <div className="empty-state">{text}</div>;
}

export function Metric({ label, value }: { label: string; value?: string | number | null }) {
  return (
    <div className="metric">
      <strong>{value ?? "Unknown"}</strong>
      <span>{label}</span>
    </div>
  );
}
