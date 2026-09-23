import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Input } from "@/components/ui/input";
import { StatusBadge, type StatusVariant } from "@/components/StatusBadge";
import {
  filteredEntries,
  type MonitorEntry,
  type MonitorStatus,
  useMonitorStore,
} from "@/store/monitor";

const STATUS_VARIANT: Record<MonitorStatus, StatusVariant> = {
  running: "neutral",
  success: "success",
  error: "danger",
};

/**
 * The Live Command Monitor (docs/SPEC.md item 7): a resizable bottom
 * drawer showing every command run through the central executor, live.
 * Fed by `exec-event` (the Tauri event bridge, wired in `run()`) plus
 * persisted history on open, both already redacted/placeholder'd by
 * nk-exec before they ever reach this component.
 *
 * Not implemented yet, tracked in PROGRESS.md: "Open in console"
 * per-entry action (there's no console to pre-fill until Phase 7 builds
 * one) and popping the panel out into its own OS window (needs a
 * routing split and a capabilities change neither built nor verified
 * yet).
 */
export function LiveCommandMonitor() {
  const { t } = useTranslation();
  const init = useMonitorStore((s) => s.init);
  const visible = useMonitorStore((s) => s.visible);
  const toggleVisible = useMonitorStore((s) => s.toggleVisible);
  const heightPx = useMonitorStore((s) => s.heightPx);
  const setHeightPx = useMonitorStore((s) => s.setHeightPx);
  const entries = useMonitorStore((s) => s.entries);
  const hasActivityWhileHidden = useMonitorStore((s) => s.hasActivityWhileHidden);
  const autoScroll = useMonitorStore((s) => s.autoScroll);
  const toggleAutoScroll = useMonitorStore((s) => s.toggleAutoScroll);
  const clear = useMonitorStore((s) => s.clear);
  const filters = useMonitorStore((s) => s.filters);
  const setFilter = useMonitorStore((s) => s.setFilter);

  useEffect(() => {
    void init();
  }, [init]);

  // docs/SPEC.md item 7: "a keyboard shortcut (e.g. Ctrl/Cmd + `)".
  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === "`" && (e.ctrlKey || e.metaKey)) {
        e.preventDefault();
        toggleVisible();
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [toggleVisible]);

  const visibleEntries = useMemo(() => filteredEntries(entries, filters), [entries, filters]);

  const environments = useMemo(
    () => Array.from(new Set(entries.map((e) => e.environment))).sort(),
    [entries],
  );

  return (
    <>
      <MonitorToggle
        visible={visible}
        pulsing={hasActivityWhileHidden}
        onClick={toggleVisible}
        label={t("monitor.toggle")}
      />
      {visible && (
        <div
          className="flex flex-col border-t border-border bg-background"
          style={{ height: heightPx }}
        >
          <ResizeHandle heightPx={heightPx} onResize={setHeightPx} />

          <div className="flex flex-wrap items-center gap-2 border-b border-border p-2">
            <Input
              placeholder={t("monitor.searchPlaceholder")}
              value={filters.text}
              onChange={(e) => setFilter({ text: e.target.value })}
              className="h-7 w-48 text-xs"
            />
            <FilterSelect
              value={filters.environment}
              onChange={(v) => setFilter({ environment: v })}
              options={environments}
              placeholder={t("monitor.allEnvironments")}
            />
            <FilterSelect
              value={filters.source}
              onChange={(v) => setFilter({ source: v })}
              options={["ordcli", "bitcoincli", "rpc"]}
              placeholder={t("monitor.allSources")}
            />
            <FilterSelect
              value={filters.status}
              onChange={(v) => setFilter({ status: v as MonitorStatus | null })}
              options={["running", "success", "error"]}
              placeholder={t("monitor.allStatuses")}
            />
            <label className="flex items-center gap-1 text-xs text-muted-foreground">
              <Checkbox
                checked={filters.showBackgroundPolling}
                onCheckedChange={(checked) => setFilter({ showBackgroundPolling: checked === true })}
              />
              {t("monitor.showBackgroundPolling")}
            </label>

            <div className="ml-auto flex items-center gap-2">
              <Button size="sm" variant="outline" onClick={toggleAutoScroll}>
                {autoScroll ? t("monitor.pauseAutoScroll") : t("monitor.resumeAutoScroll")}
              </Button>
              <Button size="sm" variant="outline" onClick={clear}>
                {t("monitor.clear")}
              </Button>
              <Button size="sm" variant="outline" onClick={() => exportEntries(visibleEntries)}>
                {t("monitor.export")}
              </Button>
            </div>
          </div>

          <EntryList entries={visibleEntries} autoScroll={autoScroll} />
        </div>
      )}
    </>
  );
}

function MonitorToggle({
  visible,
  pulsing,
  onClick,
  label,
}: {
  visible: boolean;
  pulsing: boolean;
  onClick: () => void;
  label: string;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      aria-pressed={visible}
      className="relative flex items-center gap-1.5 border-t border-border px-3 py-1 text-xs font-mono text-muted-foreground hover:bg-muted hover:text-foreground"
    >
      {pulsing && (
        <span className="absolute left-1.5 top-1 h-1.5 w-1.5 animate-pulse rounded-full bg-warning" aria-hidden="true" />
      )}
      <span className={pulsing ? "pl-2" : ""}>{label}</span>
    </button>
  );
}

function FilterSelect({
  value,
  onChange,
  options,
  placeholder,
}: {
  value: string | null;
  onChange: (v: string | null) => void;
  options: string[];
  placeholder: string;
}) {
  return (
    <select
      value={value ?? ""}
      onChange={(e) => onChange(e.target.value === "" ? null : e.target.value)}
      className="h-7 rounded-md border border-border bg-background px-1.5 text-xs"
    >
      <option value="">{placeholder}</option>
      {options.map((o) => (
        <option key={o} value={o}>
          {o}
        </option>
      ))}
    </select>
  );
}

function ResizeHandle({
  heightPx,
  onResize,
}: {
  heightPx: number;
  onResize: (px: number) => void;
}) {
  const dragStart = useRef<{ y: number; height: number } | null>(null);

  useEffect(() => {
    const onMouseMove = (e: MouseEvent) => {
      if (!dragStart.current) return;
      const delta = dragStart.current.y - e.clientY;
      onResize(dragStart.current.height + delta);
    };
    const onMouseUp = () => {
      dragStart.current = null;
    };
    window.addEventListener("mousemove", onMouseMove);
    window.addEventListener("mouseup", onMouseUp);
    return () => {
      window.removeEventListener("mousemove", onMouseMove);
      window.removeEventListener("mouseup", onMouseUp);
    };
  }, [onResize]);

  return (
    <div
      role="separator"
      aria-orientation="horizontal"
      className="h-1 shrink-0 cursor-row-resize bg-transparent hover:bg-primary/30"
      onMouseDown={(e) => {
        dragStart.current = { y: e.clientY, height: heightPx };
      }}
    />
  );
}

function EntryList({ entries, autoScroll }: { entries: MonitorEntry[]; autoScroll: boolean }) {
  const { t } = useTranslation();
  const bottomRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    // jsdom (unit tests) doesn't implement scrollIntoView at all.
    if (autoScroll) bottomRef.current?.scrollIntoView?.({ block: "end" });
  }, [entries, autoScroll]);

  return (
    <div className="flex-1 overflow-y-auto font-mono text-xs">
      {entries.map((entry) => (
        <MonitorEntryRow key={entry.id} entry={entry} />
      ))}
      {entries.length === 0 && (
        <p className="p-3 text-muted-foreground">{t("monitor.empty")}</p>
      )}
      <div ref={bottomRef} />
    </div>
  );
}

function MonitorEntryRow({ entry }: { entry: MonitorEntry }) {
  const { t } = useTranslation();
  const [expanded, setExpanded] = useState(false);

  return (
    <div className="border-b border-border/50 px-2 py-1">
      <button
        type="button"
        onClick={() => setExpanded((v) => !v)}
        className="flex w-full items-center gap-2 text-left"
      >
        <span className="text-muted-foreground">{formatTime(entry.startedAtMs)}</span>
        <span className="rounded bg-muted px-1 text-muted-foreground">{entry.environment}</span>
        <span className="text-muted-foreground">{entry.source}</span>
        <span className="flex-1 truncate">{entry.commandDisplay}</span>
        <StatusBadge label={entry.status} variant={STATUS_VARIANT[entry.status]} />
        {entry.durationMs !== null && (
          <span className="text-muted-foreground">{entry.durationMs}ms</span>
        )}
      </button>
      {expanded && (
        <div className="mt-1 space-y-1 pl-2">
          <p className="text-muted-foreground">{entry.triggeringAction}</p>
          <div className="flex gap-2">
            <button
              type="button"
              className="text-muted-foreground underline hover:text-foreground"
              onClick={() => void navigator.clipboard.writeText(entry.commandDisplay)}
            >
              {t("monitor.copyCommand")}
            </button>
            <button
              type="button"
              className="text-muted-foreground underline hover:text-foreground"
              onClick={() => void navigator.clipboard.writeText(entry.output)}
            >
              {t("monitor.copyOutput")}
            </button>
          </div>
          <pre className="max-h-40 overflow-y-auto whitespace-pre-wrap break-all rounded bg-muted/30 p-1.5">
            {entry.output || t("monitor.noOutput")}
          </pre>
        </div>
      )}
    </div>
  );
}

function formatTime(ms: number): string {
  return new Date(ms).toLocaleTimeString();
}

function exportEntries(entries: MonitorEntry[]) {
  const text = entries
    .map(
      (e) =>
        `[${formatTime(e.startedAtMs)}] ${e.environment} ${e.source} ${e.commandDisplay} (${e.status}${
          e.durationMs !== null ? `, ${e.durationMs}ms` : ""
        })\n${e.output}`,
    )
    .join("\n\n");
  const blob = new Blob([text], { type: "text/plain" });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = `nodekeeper-command-monitor-${Date.now()}.txt`;
  a.click();
  URL.revokeObjectURL(url);
}
