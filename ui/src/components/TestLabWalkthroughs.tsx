import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import type { InscriptionDetail } from "@/bindings/InscriptionDetail";
import type { WalletInscriptionEntry } from "@/bindings/WalletInscriptionEntry";
import { Button } from "@/components/ui/button";
import { useWalletExists } from "@/hooks/useWalletExists";
import { useWalletInscriptions } from "@/hooks/useWalletInscriptions";
import { WALKTHROUGH_CHAIN } from "@/lib/walkthroughConstants";
import { WALKTHROUGHS, walkthroughById } from "@/lib/walkthroughs";
import { useWalkthroughStore, type WalkthroughId } from "@/store/walkthrough";
import type { Screen } from "@/types/screen";

/**
 * docs/SPEC.md item 11's 5 guided walkthroughs (a, b, c, e -- see
 * `WalkthroughId`'s comment for why "d" is missing), listed here on
 * the Test Lab screen. Starting one hands off to `WalkthroughBanner`
 * (mounted in `App.tsx`, so it survives the screen changes each step
 * causes) -- this component's job is just picking a walkthrough,
 * gating it on its prerequisites, and (for "c") picking which existing
 * inscription "that inscription" refers to.
 */
export function TestLabWalkthroughs({ onNavigate }: { onNavigate: (screen: Screen) => void }) {
  const { t } = useTranslation();
  const { exists: walletExists } = useWalletExists(WALKTHROUGH_CHAIN);
  const { inscriptions } = useWalletInscriptions(WALKTHROUGH_CHAIN);
  const activeId = useWalkthroughStore((s) => s.activeId);
  const activeStepIndex = useWalkthroughStore((s) => s.stepIndex);
  const start = useWalkthroughStore((s) => s.start);

  const startWalkthrough = async (id: WalkthroughId) => {
    const context: Record<string, string | number> = {};
    if (id === "b") {
      // Captured once, here, rather than when the verification step
      // (3) later mounts -- the inscribe action happens on step 1, so
      // a baseline taken at that later step would already include it
      // and never detect the increase (see WalkthroughBanner's
      // "inscriptionsIncreased" comment).
      context.baselineInscriptionCount = (
        await invoke<WalletInscriptionEntry[]>("wallet_inscriptions", { chain: WALKTHROUGH_CHAIN }).catch(
          () => [],
        )
      ).length;
    }
    if (id === "c" && inscriptions && inscriptions.length > 0) {
      const target = inscriptions[0];
      context.targetInscriptionId = target.id;
      try {
        const detail = await invoke<InscriptionDetail>("inscription_detail", {
          chain: WALKTHROUGH_CHAIN,
          id: target.id,
        });
        // `sat` is only absent when the sats index is off (Foundation
        // F) -- Regtest enables it by default, but if it's somehow
        // missing the sat-based checkpoint just falls back to manual
        // (see `WalkthroughBanner`'s `checkpointSatisfied`).
        if (detail.sat !== null) context.targetSat = detail.sat;
      } catch {
        // Not fatal to starting -- same fallback as above.
      }
    }
    start(id, context);
    onNavigate(walkthroughById(id).steps[0].screen);
  };

  const resume = (id: WalkthroughId) => {
    onNavigate(walkthroughById(id).steps[activeStepIndex].screen);
  };

  return (
    <div className="space-y-2 rounded-md border border-border p-3">
      <h3 className="text-sm font-medium">{t("walkthroughs.list.title")}</h3>
      <p className="text-xs text-muted-foreground">{t("walkthroughs.list.intro")}</p>
      <div className="grid gap-2 sm:grid-cols-2">
        {WALKTHROUGHS.map((w) => {
          const isActive = activeId === w.id;
          const disabledReason = isActive
            ? null
            : activeId !== null
              ? t("walkthroughs.list.anotherActive")
              : w.id !== "a" && walletExists !== true
                ? t("walkthroughs.list.needsWallet")
                : w.requiresExistingInscription && (!inscriptions || inscriptions.length === 0)
                  ? t("walkthroughs.list.needsInscription")
                  : null;
          return (
            <div key={w.id} className="space-y-1 rounded-md border border-border p-2">
              <p className="text-sm font-medium">{t(`walkthroughs.${w.id}.title`)}</p>
              <p className="text-xs text-muted-foreground">{t(`walkthroughs.${w.id}.description`)}</p>
              {disabledReason && <p className="text-xs text-warning">{disabledReason}</p>}
              <Button
                size="sm"
                variant="outline"
                disabled={disabledReason !== null}
                onClick={() => (isActive ? resume(w.id) : void startWalkthrough(w.id))}
              >
                {isActive ? t("walkthroughs.list.resume") : t("walkthroughs.list.start")}
              </Button>
            </div>
          );
        })}
      </div>
    </div>
  );
}
