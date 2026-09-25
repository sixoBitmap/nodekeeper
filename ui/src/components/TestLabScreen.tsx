import { useState } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import type { Environment } from "@/bindings/Environment";
import type { TypedError } from "@/bindings/TypedError";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { ConfirmDialog } from "@/components/ConfirmDialog";
import { ErrorPanel } from "@/components/ErrorPanel";
import { useDashboardStatus } from "@/hooks/useDashboardStatus";
import { useOrdStatus } from "@/hooks/useOrdStatus";
import { useWalletExists } from "@/hooks/useWalletExists";
import { friendlyError } from "@/lib/error-messages";

/**
 * Regtest Test Lab (docs/SPEC.md item 11): safe practice mode. Only
 * meaningful on Regtest -- `mine_blocks`/`reset_test_lab` refuse
 * outright on any other chain (real backend enforcement, not just a UI
 * gate), so this screen shows a plain redirect message instead of
 * controls when a different environment is selected.
 *
 * Deliberately doesn't reimplement wallet creation inline: that flow
 * already exists on the Wallet screen with the real sensitive-output
 * handling (`SensitiveSeedView`), and duplicating it here would mean a
 * second, less-reviewed path that touches a mnemonic. "One-click
 * setup" starts services and mines blocks; if no wallet exists yet, it
 * points at the Wallet screen instead of trying to create one itself.
 *
 * The 5 guided walkthroughs are a separate, larger, not-yet-built
 * piece (PROGRESS.md) -- this covers the setup/mining/reset controls
 * they'll eventually sit alongside.
 */
export function TestLabScreen({ environment }: { environment: Environment }) {
  const { t } = useTranslation();
  const isRegtest = environment.chain === "regtest";
  const node = useDashboardStatus(environment.chain);
  const ord = useOrdStatus(environment.chain);
  const { exists: walletExists, refresh: refreshWalletExists } = useWalletExists(environment.chain);

  const [mineCount, setMineCount] = useState("1");
  const [busyAction, setBusyAction] = useState<string | null>(null);
  const [actionError, setActionError] = useState<TypedError | null>(null);
  const [lastMined, setLastMined] = useState<string[] | null>(null);
  const [resetConfirmOpen, setResetConfirmOpen] = useState(false);

  const mine = async (count: number) => {
    setBusyAction("mine");
    setActionError(null);
    try {
      const hashes = await invoke<string[]>("mine_blocks", { chain: environment.chain, count });
      setLastMined(hashes);
      await node.refresh();
    } catch (e) {
      setActionError(e as TypedError);
    } finally {
      setBusyAction(null);
    }
  };

  const oneClickSetup = async () => {
    setBusyAction("setup");
    setActionError(null);
    try {
      if (!node.running) await node.start();
      if (!ord.running) await ord.start();
      await mine(101);
    } catch (e) {
      setActionError(e as TypedError);
    } finally {
      setBusyAction(null);
    }
  };

  const resetTestLab = async () => {
    setResetConfirmOpen(false);
    setBusyAction("reset");
    setActionError(null);
    try {
      await invoke("reset_test_lab");
      await Promise.all([node.refresh(), ord.refresh(), refreshWalletExists()]);
      setLastMined(null);
    } catch (e) {
      setActionError(e as TypedError);
    } finally {
      setBusyAction(null);
    }
  };

  if (!isRegtest) {
    return (
      <div className="p-4">
        <p className="text-sm text-muted-foreground">{t("testLab.wrongEnvironment")}</p>
      </div>
    );
  }

  return (
    <div className="space-y-4 p-4">
      <div>
        <h2 className="text-lg font-semibold">{t("testLab.title")}</h2>
        <p className="mt-1 text-sm text-muted-foreground">{t("testLab.intro")}</p>
      </div>

      {actionError &&
        (() => {
          const friendly = friendlyError(actionError);
          return (
            <ErrorPanel
              title={friendly.title}
              message={friendly.whatToDo ? `${friendly.message} ${friendly.whatToDo}` : friendly.message}
              technicalDetails={actionError.message}
            />
          );
        })()}

      <div className="space-y-2 rounded-md border border-border p-3">
        <h3 className="text-sm font-medium">{t("testLab.oneClickSetup")}</h3>
        <p className="text-xs text-muted-foreground">{t("testLab.oneClickSetupHint")}</p>
        {walletExists === false ? (
          <p className="text-xs text-danger">{t("testLab.needsWalletFirst")}</p>
        ) : (
          <Button size="sm" disabled={busyAction !== null} onClick={() => void oneClickSetup()}>
            {busyAction === "setup" ? t("testLab.settingUp") : t("testLab.oneClickSetup")}
          </Button>
        )}
      </div>

      <div className="space-y-2 rounded-md border border-border p-3">
        <h3 className="text-sm font-medium">{t("testLab.mineBlocks")}</h3>
        <div className="flex items-center gap-2">
          <Input
            type="number"
            min={1}
            value={mineCount}
            onChange={(e) => setMineCount(e.target.value)}
            className="w-24"
            disabled={busyAction !== null || walletExists !== true}
          />
          <Button
            size="sm"
            variant="outline"
            disabled={busyAction !== null || walletExists !== true || Number(mineCount) < 1}
            onClick={() => void mine(Number(mineCount))}
          >
            {busyAction === "mine" ? t("testLab.mining") : t("testLab.mine")}
          </Button>
          <Button
            size="sm"
            variant="outline"
            disabled={busyAction !== null || walletExists !== true}
            onClick={() => void mine(1)}
          >
            {t("testLab.getTestCoins")}
          </Button>
        </div>
        {walletExists === false && (
          <p className="text-xs text-muted-foreground">{t("testLab.needsWalletFirst")}</p>
        )}
        {lastMined && (
          <p className="text-xs text-muted-foreground">
            {t("testLab.minedBlocks", { count: lastMined.length })}
          </p>
        )}
      </div>

      <div className="space-y-2 rounded-md border border-danger/30 p-3">
        <h3 className="text-sm font-medium">{t("testLab.reset")}</h3>
        <p className="text-xs text-muted-foreground">{t("testLab.resetHint")}</p>
        <Button
          size="sm"
          variant="outline"
          disabled={busyAction !== null}
          onClick={() => setResetConfirmOpen(true)}
        >
          {busyAction === "reset" ? t("testLab.resetting") : t("testLab.reset")}
        </Button>
      </div>

      <ConfirmDialog
        open={resetConfirmOpen}
        onOpenChange={setResetConfirmOpen}
        environment={environment}
        title={t("testLab.resetConfirmTitle")}
        description={t("testLab.resetConfirmDescription")}
        onConfirm={() => void resetTestLab()}
        confirmLabel={t("testLab.reset")}
      />
    </div>
  );
}
