import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";

const SETTING_KEY = "prevent_sleep_during_sync";

interface PreventSleepState {
  /** The user's stored preference -- whether to hold the OS awake at
   * all while syncing, not whether it's currently being held (that
   * also depends on whether anything is actually syncing right now,
   * which only `NotificationWatcher`'s poll loop knows). */
  enabled: boolean;
  loaded: boolean;
  init: () => Promise<void>;
  setEnabled: (enabled: boolean) => void;
}

/** docs/SPEC.md item 8: "Optional 'prevent sleep during sync' setting."
 * A small shared store (not local component state) so the toggle,
 * wherever it's shown, and `NotificationWatcher`'s background poll
 * loop (which actually calls the backend `set_prevent_sleep` command)
 * stay in sync without either needing to know about the other. */
export const usePreventSleepStore = create<PreventSleepState>()((set) => ({
  enabled: false,
  loaded: false,

  init: async () => {
    const value = await invoke<string | null>("get_setting", { key: SETTING_KEY });
    set({ enabled: value === "true", loaded: true });
  },

  setEnabled: (enabled) => {
    set({ enabled });
    void invoke("set_setting", { key: SETTING_KEY, value: String(enabled) });
  },
}));
