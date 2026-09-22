export type StatusVariant = "neutral" | "success" | "warning" | "danger";

// Full class names, not built dynamically -- see the note in
// lib/environment-colors.ts about why Tailwind needs that.
const VARIANT_CLASSES: Record<StatusVariant, string> = {
  neutral: "bg-muted text-muted-foreground",
  success: "bg-success/15 text-success",
  warning: "bg-warning/15 text-warning",
  danger: "bg-danger/15 text-danger",
};

/**
 * A status pill with a text label, never color alone (docs/SPEC.md item
 * 2: "Status badges with text labels: 'Syncing', 'Indexing', 'Ready'").
 */
export function StatusBadge({
  label,
  variant = "neutral",
}: {
  label: string;
  variant?: StatusVariant;
}) {
  return (
    <span
      className={`inline-flex items-center rounded-full px-2.5 py-0.5 text-xs font-medium ${VARIANT_CLASSES[variant]}`}
    >
      {label}
    </span>
  );
}
