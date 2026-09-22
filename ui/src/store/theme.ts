import { create } from "zustand";
import { persist } from "zustand/middleware";

export type Theme = "dark" | "light";

interface ThemeState {
  theme: Theme;
  setTheme: (theme: Theme) => void;
  toggleTheme: () => void;
}

function applyThemeClass(theme: Theme) {
  document.documentElement.classList.toggle("dark", theme === "dark");
}

// Persisted via the webview's localStorage for now (per-window, not
// per-environment — a theme preference isn't environment-scoped). Once
// nk-store's settings table exists, this should move there so it's part
// of the same settings surface as everything else in item 9.
export const useThemeStore = create<ThemeState>()(
  persist(
    (set, get) => ({
      // Dark-mode-first (docs/SPEC.md "DESIGN"): defaults to dark.
      theme: "dark",
      setTheme: (theme) => {
        applyThemeClass(theme);
        set({ theme });
      },
      toggleTheme: () => {
        const next: Theme = get().theme === "dark" ? "light" : "dark";
        get().setTheme(next);
      },
    }),
    {
      name: "nodekeeper-theme",
      onRehydrateStorage: () => (state) => {
        if (state) applyThemeClass(state.theme);
      },
    },
  ),
);

// Apply immediately on module load too, so the correct class is set before
// the store's persist middleware finishes rehydrating (avoids a flash of
// the wrong theme on first paint after a reload).
applyThemeClass(useThemeStore.getState().theme);
