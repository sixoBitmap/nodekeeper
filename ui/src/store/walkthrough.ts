import { create } from "zustand";

/** docs/SPEC.md item 11's 5 guided walkthroughs -- "d" (send an
 * inscription to a second test wallet) isn't implemented: it needs a
 * second wallet in one environment, which every wallet-related Tauri
 * command deliberately doesn't support yet (`DEFAULT_WALLET_NAME` in
 * src-tauri/src/lib.rs; see PROGRESS.md's Backlog section). */
export type WalkthroughId = "a" | "b" | "c" | "e";

interface WalkthroughState {
  activeId: WalkthroughId | null;
  stepIndex: number;
  /** When the current step became current -- lets the banner capture
   * "since this step started" baselines (block height, inscription
   * count, command-history entries) for auto-detected checkpoints. */
  stepStartedAtMs: number;
  /** Free-form context captured once at `start` and reused by every
   * step -- e.g. walkthrough "c"'s target inscription id/sat, picked
   * before the walkthrough begins so every step can refer to "that
   * inscription" without re-picking it. */
  context: Record<string, string | number>;

  start: (id: WalkthroughId, context?: Record<string, string | number>) => void;
  goToStep: (index: number) => void;
  exit: () => void;
}

export const useWalkthroughStore = create<WalkthroughState>()((set) => ({
  activeId: null,
  stepIndex: 0,
  stepStartedAtMs: 0,
  context: {},

  start: (id, context = {}) => set({ activeId: id, stepIndex: 0, stepStartedAtMs: Date.now(), context }),

  goToStep: (index) => set({ stepIndex: index, stepStartedAtMs: Date.now() }),

  exit: () => set({ activeId: null, stepIndex: 0, stepStartedAtMs: 0, context: {} }),
}));
