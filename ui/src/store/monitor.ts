import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { ExecEvent } from "@/bindings/ExecEvent";
import type { CommandHistoryEntry } from "@/bindings/CommandHistoryEntry";

export type MonitorStatus = "running" | "success" | "error";

export interface MonitorEntry {
  id: string;
  environment: string;
  source: string;
  triggeringAction: string;
  commandDisplay: string;
  startedAtMs: number;
  status: MonitorStatus;
  exitCode: number | null;
  durationMs: number | null;
  output: string;
}

export interface MonitorFilters {
  environment: string | null;
  source: string | null;
  status: MonitorStatus | null;
  text: string;
  /** docs/SPEC.md item 7: "Background polling is hidden by default
   * with a Show background polling toggle." Present for that reason,
   * but currently has nothing to filter -- nothing in the app tags a
   * command as background yet (no periodic poller goes through the
   * central executor today; the Dashboard's own polling calls
   * `node_status` directly, not via nk-exec). A real, not decorative,
   * gap -- left for whenever something produces background-tagged
   * commands, tracked in PROGRESS.md. */
  showBackgroundPolling: boolean;
}

const VISIBLE_SETTING_KEY = "monitor_visible";
const HEIGHT_SETTING_KEY = "monitor_height_px";
const DEFAULT_HEIGHT_PX = 280;
const MIN_HEIGHT_PX = 120;
const MAX_HEIGHT_PX = 700;
const HISTORY_LIMIT = 500;

interface MonitorState {
  initialized: boolean;
  entries: MonitorEntry[]; // oldest first
  visible: boolean;
  heightPx: number;
  autoScroll: boolean;
  /** Pulses the show/hide toggle (docs/SPEC.md item 7: "An activity
   * indicator on the toggle pulses when commands run while the panel
   * is hidden"). Cleared whenever the panel is shown. */
  hasActivityWhileHidden: boolean;
  filters: MonitorFilters;

  init: () => Promise<void>;
  toggleVisible: () => void;
  setHeightPx: (px: number) => void;
  toggleAutoScroll: () => void;
  clear: () => void;
  setFilter: (patch: Partial<MonitorFilters>) => void;
}

function historyEntryToMonitorEntry(h: CommandHistoryEntry): MonitorEntry {
  return {
    id: h.id,
    environment: h.environment,
    source: h.source,
    triggeringAction: h.triggering_action,
    commandDisplay: h.command_display,
    startedAtMs: h.started_at_ms,
    status: h.status,
    exitCode: h.exit_code,
    durationMs: h.duration_ms,
    output: h.output,
  };
}

let unlisten: UnlistenFn | null = null;

export const useMonitorStore = create<MonitorState>()((set, get) => ({
  initialized: false,
  entries: [],
  visible: false,
  heightPx: DEFAULT_HEIGHT_PX,
  autoScroll: true,
  hasActivityWhileHidden: false,
  filters: { environment: null, source: null, status: null, text: "", showBackgroundPolling: false },

  init: async () => {
    if (get().initialized) return;
    set({ initialized: true });

    const [visibleSetting, heightSetting, history] = await Promise.all([
      invoke<string | null>("get_setting", { key: VISIBLE_SETTING_KEY }),
      invoke<string | null>("get_setting", { key: HEIGHT_SETTING_KEY }),
      invoke<CommandHistoryEntry[]>("list_command_history", { environment: null }),
    ]);

    set({
      visible: visibleSetting === "true",
      heightPx: heightSetting ? Number(heightSetting) : DEFAULT_HEIGHT_PX,
      // list_command_history returns newest first; the panel renders
      // oldest first, like a terminal scrollback.
      entries: history.slice(0, HISTORY_LIMIT).reverse().map(historyEntryToMonitorEntry),
    });

    if (unlisten) return; // a second init() call (e.g. React StrictMode) reuses the one listener
    try {
      unlisten = await listen<ExecEvent>("exec-event", (event) => {
        applyExecEvent(set, get, event.payload);
      });
    } catch (e) {
      // Live updates just won't arrive (e.g. the dev-browser preview,
      // which doesn't mock Tauri's event-system internals) -- history
      // above already loaded, so the panel is still useful.
      console.warn("monitor: failed to subscribe to exec-event", e);
    }
  },

  toggleVisible: () => {
    const next = !get().visible;
    set({ visible: next, ...(next ? { hasActivityWhileHidden: false } : {}) });
    void invoke("set_setting", { key: VISIBLE_SETTING_KEY, value: String(next) });
  },

  setHeightPx: (px) => {
    const clamped = Math.min(MAX_HEIGHT_PX, Math.max(MIN_HEIGHT_PX, px));
    set({ heightPx: clamped });
    void invoke("set_setting", { key: HEIGHT_SETTING_KEY, value: String(clamped) });
  },

  toggleAutoScroll: () => set((s) => ({ autoScroll: !s.autoScroll })),

  clear: () => set({ entries: [] }),

  setFilter: (patch) => set((s) => ({ filters: { ...s.filters, ...patch } })),
}));

function applyExecEvent(
  set: (partial: Partial<MonitorState> | ((s: MonitorState) => Partial<MonitorState>)) => void,
  get: () => MonitorState,
  event: ExecEvent,
) {
  const markActivityIfHidden = () => {
    if (!get().visible) set({ hasActivityWhileHidden: true });
  };

  if (event.type === "Started") {
    markActivityIfHidden();
    const entry: MonitorEntry = {
      id: event.id,
      environment: event.environment,
      source: event.source,
      triggeringAction: event.triggering_action,
      commandDisplay: event.command_display,
      startedAtMs: Date.now(),
      status: "running",
      exitCode: null,
      durationMs: null,
      output: "",
    };
    set((s) => ({ entries: [...s.entries, entry] }));
    return;
  }

  if (event.type === "Output") {
    set((s) => ({
      entries: s.entries.map((e) => (e.id === event.id ? { ...e, output: e.output + event.chunk } : e)),
    }));
    return;
  }

  // Finished
  markActivityIfHidden();
  set((s) => ({
    entries: s.entries.map((e) =>
      e.id === event.id
        ? {
            ...e,
            status: event.exit_code === 0 ? "success" : "error",
            exitCode: event.exit_code,
            durationMs: event.duration_ms,
          }
        : e,
    ),
  }));
}

export function filteredEntries(entries: MonitorEntry[], filters: MonitorFilters): MonitorEntry[] {
  const text = filters.text.trim().toLowerCase();
  return entries.filter((e) => {
    if (filters.environment && e.environment !== filters.environment) return false;
    if (filters.source && e.source !== filters.source) return false;
    if (filters.status && e.status !== filters.status) return false;
    if (text && !e.commandDisplay.toLowerCase().includes(text)) return false;
    return true;
  });
}
