import { useTranslation } from "react-i18next";
import { useThemeStore } from "./store/theme";

function App() {
  const { t } = useTranslation();
  const { theme, toggleTheme } = useThemeStore();

  return (
    <main className="flex h-full flex-col items-center justify-center gap-4 bg-[var(--color-bg)] text-[var(--color-text)]">
      <h1 className="text-2xl font-semibold">{t("app.title")}</h1>
      <button
        type="button"
        onClick={toggleTheme}
        className="rounded-md border border-[var(--color-border)] bg-[var(--color-surface)] px-3 py-1.5 text-sm hover:bg-[var(--color-surface-raised)]"
      >
        {theme === "dark" ? t("theme.toggleToLight") : t("theme.toggleToDark")}
      </button>
    </main>
  );
}

export default App;
