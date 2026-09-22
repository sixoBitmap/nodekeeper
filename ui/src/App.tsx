import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import { Button } from "@/components/ui/button";
import { EnvBanner } from "@/components/EnvBanner";
import { EnvironmentSwitcher } from "@/components/EnvironmentSwitcher";
import { FirstRunDisclaimer } from "@/components/FirstRunDisclaimer";
import { SystemCheckScreen } from "@/components/SystemCheckScreen";
import { selectedEnvironment, useEnvironmentStore } from "@/store/environment";
import { useThemeStore } from "@/store/theme";

const DISCLAIMER_SETTING_KEY = "disclaimer_acknowledged";

function App() {
  const { t } = useTranslation();
  const { theme, toggleTheme } = useThemeStore();
  const loadEnvironments = useEnvironmentStore((s) => s.load);
  const environmentsLoaded = useEnvironmentStore((s) => s.loaded);
  const selected = useEnvironmentStore(selectedEnvironment);

  // null = not checked yet, matches the "loading" state below.
  const [disclaimerAcknowledged, setDisclaimerAcknowledged] = useState<boolean | null>(null);

  useEffect(() => {
    void loadEnvironments();
    invoke<string | null>("get_setting", { key: DISCLAIMER_SETTING_KEY }).then((value) =>
      setDisclaimerAcknowledged(value === "true"),
    );
  }, [loadEnvironments]);

  const acknowledgeDisclaimer = async () => {
    await invoke("set_setting", { key: DISCLAIMER_SETTING_KEY, value: "true" });
    setDisclaimerAcknowledged(true);
  };

  if (disclaimerAcknowledged === null || !environmentsLoaded) {
    return <div className="h-full bg-background" />;
  }

  // docs/SPEC.md item 0: shown once, on first run only.
  if (!disclaimerAcknowledged) {
    return <FirstRunDisclaimer onAcknowledge={() => void acknowledgeDisclaimer()} />;
  }

  return (
    <div className="flex h-full flex-col bg-background text-foreground">
      {selected && <EnvBanner environment={selected} />}
      <header className="flex items-center justify-between border-b border-border px-4 py-2">
        <h1 className="text-sm font-semibold">{t("app.title")}</h1>
        <div className="flex items-center gap-2">
          <EnvironmentSwitcher />
          <Button variant="outline" size="sm" onClick={toggleTheme}>
            {theme === "dark" ? t("theme.toggleToLight") : t("theme.toggleToDark")}
          </Button>
        </div>
      </header>
      <main className="flex-1 overflow-y-auto">
        <SystemCheckScreen onContinue={() => {}} />
      </main>
    </div>
  );
}

export default App;
