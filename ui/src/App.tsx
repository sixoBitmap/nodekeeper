import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import { Moon, Sun } from "lucide-react";
import { Button } from "@/components/ui/button";
import { BinarySetupScreen } from "@/components/BinarySetupScreen";
import { DashboardScreen } from "@/components/DashboardScreen";
import { EnvBanner } from "@/components/EnvBanner";
import { EnvironmentSwitcher } from "@/components/EnvironmentSwitcher";
import { FirstRunDisclaimer } from "@/components/FirstRunDisclaimer";
import { IndexOptionsScreen } from "@/components/IndexOptionsScreen";
import { InscribeStudioScreen } from "@/components/InscribeStudioScreen";
import { LiveCommandMonitor } from "@/components/LiveCommandMonitor";
import { SystemCheckScreen } from "@/components/SystemCheckScreen";
import { WalletScreen } from "@/components/WalletScreen";
import { selectedEnvironment, useEnvironmentStore } from "@/store/environment";
import { useThemeStore } from "@/store/theme";

type Screen = "dashboard" | "wallet" | "inscribe";
type WizardStep = "systemCheck" | "binarySetup" | "indexOptions" | "done";

const DISCLAIMER_SETTING_KEY = "disclaimer_acknowledged";

function App() {
  const { t } = useTranslation();
  const { theme, toggleTheme } = useThemeStore();
  const loadEnvironments = useEnvironmentStore((s) => s.load);
  const environmentsLoaded = useEnvironmentStore((s) => s.loaded);
  const selected = useEnvironmentStore(selectedEnvironment);

  // null = not checked yet, matches the "loading" state below.
  const [disclaimerAcknowledged, setDisclaimerAcknowledged] = useState<boolean | null>(null);
  // Not persisted as its own flag: the system check is informational, so
  // it's fine to show it again on every launch. The binary setup and
  // index options steps effectively *are* persisted, just via their own
  // settings keys (`bitcoind_path`/`ord_path`, `index_options_<chain>`)
  // -- each screen skips straight past itself once already configured,
  // rather than a separate "wizard done" flag.
  const [wizardStep, setWizardStep] = useState<WizardStep>("systemCheck");
  const [screen, setScreen] = useState<Screen>("dashboard");

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
        <div className="flex items-center gap-4">
          <h1 className="text-sm font-semibold">{t("app.title")}</h1>
          {wizardStep === "done" && (
            <nav className="flex gap-1">
              <Button
                variant={screen === "dashboard" ? "secondary" : "ghost"}
                size="sm"
                onClick={() => setScreen("dashboard")}
              >
                {t("nav.dashboard")}
              </Button>
              <Button
                variant={screen === "wallet" ? "secondary" : "ghost"}
                size="sm"
                onClick={() => setScreen("wallet")}
              >
                {t("nav.wallet")}
              </Button>
              <Button
                variant={screen === "inscribe" ? "secondary" : "ghost"}
                size="sm"
                onClick={() => setScreen("inscribe")}
              >
                {t("nav.inscribe")}
              </Button>
            </nav>
          )}
        </div>
        <div className="flex items-center gap-2">
          <EnvironmentSwitcher />
          <Button
            variant="outline"
            size="icon-sm"
            onClick={toggleTheme}
            aria-label={theme === "dark" ? t("theme.toggleToLight") : t("theme.toggleToDark")}
            title={theme === "dark" ? t("theme.toggleToLight") : t("theme.toggleToDark")}
          >
            {theme === "dark" ? <Sun /> : <Moon />}
          </Button>
        </div>
      </header>
      <main className="flex-1 overflow-y-auto">
        {wizardStep === "done" && selected ? (
          // Keyed by chain (and now screen): switching environments or
          // screens should remount with fresh state, not carry over
          // the previous one's status/log-viewer state (see
          // useDashboardStatus/LogViewer).
          screen === "dashboard" ? (
            <DashboardScreen key={selected.chain} environment={selected} />
          ) : screen === "wallet" ? (
            <WalletScreen key={selected.chain} environment={selected} />
          ) : (
            <InscribeStudioScreen key={selected.chain} environment={selected} />
          )
        ) : wizardStep === "indexOptions" ? (
          <IndexOptionsScreen
            onContinue={() => {
              // Refresh so the Dashboard/Wallet reflect the index
              // options just saved, not the defaults fetched at mount.
              void loadEnvironments();
              setWizardStep("done");
            }}
          />
        ) : wizardStep === "binarySetup" ? (
          <BinarySetupScreen onContinue={() => setWizardStep("indexOptions")} />
        ) : (
          <SystemCheckScreen onContinue={() => setWizardStep("binarySetup")} />
        )}
      </main>
      <LiveCommandMonitor />
    </div>
  );
}

export default App;
