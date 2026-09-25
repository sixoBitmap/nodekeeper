import { useEffect, useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import type { CommandHistoryEntry } from "@/bindings/CommandHistoryEntry";
import type { NodeStatus } from "@/bindings/NodeStatus";
import type { WalletBalance } from "@/bindings/WalletBalance";
import type { WalletInscriptionEntry } from "@/bindings/WalletInscriptionEntry";
import { Button } from "@/components/ui/button";
import { WALKTHROUGH_CHAIN, WALKTHROUGH_ENVIRONMENT_NAME } from "@/lib/walkthroughConstants";
import { walkthroughById, type CheckpointKind, type WalkthroughStepDef } from "@/lib/walkthroughs";
import { useMonitorStore } from "@/store/monitor";
import { useEnvironmentStore } from "@/store/environment";
import { useWalkthroughStore } from "@/store/walkthrough";
import type { Screen } from "@/types/screen";

const POLL_INTERVAL_MS = 2000;

const SCREEN_LABEL_KEY: Record<Screen, string> = {
  overview: "nav.overview",
  dashboard: "nav.dashboard",
  wallet: "nav.wallet",
  inscribe: "nav.inscribe",
  explorer: "nav.explorer",
  console: "nav.console",
  scripts: "nav.scripts",
  testLab: "nav.testLab",
};

/** Evaluates whether `kind` is currently satisfied. `baseline` is
 * whatever the checkpoint needed to snapshot when its step started --
 * `undefined` for kinds that don't need one. Every check reads real
 * Regtest state; there's nothing to fake here even in the browser dev
 * preview. */
async function checkpointSatisfied(
  kind: CheckpointKind,
  baseline: number | undefined,
  context: Record<string, string | number>,
): Promise<boolean> {
  switch (kind) {
    case "none":
      return true;
    case "walletExists":
      return invoke<boolean>("wallet_exists", { chain: WALKTHROUGH_CHAIN });
    case "balancePositive":
      return invoke<WalletBalance>("wallet_balance", { chain: WALKTHROUGH_CHAIN })
        .then((b) => b.total > 0)
        .catch(() => false);
    case "blocksIncreased": {
      if (baseline === undefined) return false;
      const running = await invoke<boolean>("is_node_running", { chain: WALKTHROUGH_CHAIN });
      if (!running) return false;
      const status = await invoke<NodeStatus>("node_status", { chain: WALKTHROUGH_CHAIN });
      return status.blocks > baseline;
    }
    case "inscriptionsIncreased": {
      // Deliberately reads its baseline from `context`, captured once
      // when the walkthrough *starts* (`TestLabWalkthroughs`), not
      // from this step's own mount: the inscribe action happens on an
      // earlier step, so by the time this (verification) step mounts,
      // the count has already risen relative to nothing -- comparing
      // against a per-step baseline here would never detect it.
      const walkthroughBaseline = context.baselineInscriptionCount;
      if (typeof walkthroughBaseline !== "number") return false;
      return invoke<WalletInscriptionEntry[]>("wallet_inscriptions", { chain: WALKTHROUGH_CHAIN })
        .then((list) => list.length > walkthroughBaseline)
        .catch(() => false);
    }
    case "satHasTwoInscriptions": {
      const sat = context.targetSat;
      if (typeof sat !== "number") return false;
      return invoke<string[]>("sat_inscriptions", { chain: WALKTHROUGH_CHAIN, sat })
        .then((ids) => ids.length >= 2)
        .catch(() => false);
    }
    case "consoleRanSince":
    case "scriptRanSince": {
      if (baseline === undefined) return false;
      const history = await invoke<CommandHistoryEntry[]>("list_command_history", {
        environment: WALKTHROUGH_ENVIRONMENT_NAME,
      });
      return history.some(
        (h) =>
          h.started_at_ms >= baseline &&
          (kind === "consoleRanSince"
            ? h.triggering_action === "console"
            : h.triggering_action.startsWith("run script ")),
      );
    }
  }
}

/** Fetches the baseline a step's checkpoint needs to compare against
 * (a block height at the moment the step became current) --
 * `undefined` for kinds that don't need a fresh-per-step baseline:
 * "inscriptionsIncreased" uses a baseline captured once at walkthrough
 * start instead (see `checkpointSatisfied`'s comment), and
 * "consoleRanSince"/"scriptRanSince" use the step's own start time
 * directly. */
async function fetchBaseline(kind: CheckpointKind): Promise<number | undefined> {
  switch (kind) {
    case "blocksIncreased": {
      const running = await invoke<boolean>("is_node_running", { chain: WALKTHROUGH_CHAIN });
      if (!running) return undefined;
      const status = await invoke<NodeStatus>("node_status", { chain: WALKTHROUGH_CHAIN });
      return status.blocks;
    }
    default:
      return undefined;
  }
}

/**
 * Owns one step's checkpoint polling. Mounted fresh (via a `key` on
 * `${activeId}-${stepIndex}` in `WalkthroughBanner`) every time the
 * step changes, so there's no stale state to reset between steps --
 * `checkpointMet` simply starts `false` again on every mount, rather
 * than an effect resetting it (which `react-hooks/set-state-in-effect`
 * flags, and which would race the fetch below anyway).
 */
function StepCheckpoint({
  step,
  context,
  children,
}: {
  step: WalkthroughStepDef;
  context: Record<string, string | number>;
  children: (checkpointMet: boolean) => ReactNode;
}) {
  const [checkpointMet, setCheckpointMet] = useState(false);

  useEffect(() => {
    let cancelled = false;
    let intervalId: number | undefined;
    void fetchBaseline(step.checkpoint).then((baseline) => {
      if (cancelled) return;
      const check = () => {
        void checkpointSatisfied(step.checkpoint, baseline, context).then((met) => {
          if (!cancelled) setCheckpointMet(met);
        });
      };
      check();
      intervalId = window.setInterval(check, POLL_INTERVAL_MS);
    });
    return () => {
      cancelled = true;
      if (intervalId !== undefined) window.clearInterval(intervalId);
    };
    // `step`/`context` are fixed for this component's whole lifetime --
    // a new step means a new `key` and a fresh mount, not a re-run of
    // this effect.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return children(checkpointMet);
}

/**
 * docs/SPEC.md item 11: "Guided walkthroughs with checkpoints, with
 * the Live Command Monitor opened automatically." Rendered
 * unconditionally in `App.tsx` (between the header and the screen
 * dispatch) so it survives navigating to whichever screen each step
 * sends the user to -- a walkthrough started from `TestLabScreen`
 * would otherwise be torn down the moment the user leaves it, since
 * every screen unmounts when `screen` changes.
 *
 * Always operates against Regtest regardless of which environment the
 * switcher currently has selected (`WALKTHROUGH_CHAIN`) -- `onNavigate`
 * only changes the screen, not the environment, so this re-selects
 * Regtest on every "Go to" click to guarantee the step always lands on
 * the right environment's data, even if the user wandered off to
 * another one mid-walkthrough.
 */
export function WalkthroughBanner({ onNavigate }: { onNavigate: (screen: Screen) => void }) {
  const { t } = useTranslation();
  const activeId = useWalkthroughStore((s) => s.activeId);
  const stepIndex = useWalkthroughStore((s) => s.stepIndex);
  const context = useWalkthroughStore((s) => s.context);
  const goToStep = useWalkthroughStore((s) => s.goToStep);
  const exit = useWalkthroughStore((s) => s.exit);
  const selectEnvironment = useEnvironmentStore((s) => s.select);
  const showMonitor = useMonitorStore((s) => s.show);

  // Opens the Live Command Monitor once per walkthrough start, not on
  // every step -- matches the spec's wording ("opened automatically"
  // describing the walkthrough as a whole).
  useEffect(() => {
    if (activeId) showMonitor();
    // eslint-disable-next-line react-hooks/exhaustive-deps -- fires once when a walkthrough starts, not on every showMonitor identity change
  }, [activeId]);

  const def = activeId ? walkthroughById(activeId) : null;
  const step = def ? def.steps[stepIndex] : null;

  if (!activeId || !def || !step) return null;

  const isLastStep = stepIndex === def.steps.length - 1;

  const goTo = (screen: Screen) => {
    selectEnvironment(WALKTHROUGH_CHAIN);
    onNavigate(screen);
  };

  const advance = () => {
    if (isLastStep) {
      exit();
    } else {
      goToStep(stepIndex + 1);
    }
  };

  return (
    <StepCheckpoint key={`${activeId}-${stepIndex}`} step={step} context={context}>
      {(checkpointMet) => {
        const canContinue = step.checkpoint === "none" || checkpointMet;
        return (
          <div className="border-b border-primary/30 bg-primary/5 px-4 py-3">
            <div className="flex flex-wrap items-start justify-between gap-3">
              <div className="min-w-0 flex-1 space-y-1">
                <p className="text-xs font-medium uppercase tracking-wide text-primary">
                  {t("walkthroughs.list.title")} · {t(`walkthroughs.${activeId}.title`)} ·{" "}
                  {t("walkthroughs.banner.stepLabel", { current: stepIndex + 1, total: def.steps.length })}
                </p>
                <p className="text-sm">
                  {t(`walkthroughs.${activeId}.steps.${stepIndex}.instructions`, context)}
                </p>
                <p className="text-xs text-muted-foreground">
                  {step.checkpoint === "none"
                    ? t("walkthroughs.banner.noCheckpoint")
                    : checkpointMet
                      ? t("walkthroughs.banner.checkpointDone")
                      : t("walkthroughs.banner.checkpointPending")}
                </p>
              </div>
              <div className="flex shrink-0 flex-wrap items-center gap-2">
                <Button size="sm" variant="outline" onClick={() => goTo(step.screen)}>
                  {t("walkthroughs.banner.goTo", { screen: t(SCREEN_LABEL_KEY[step.screen]) })}
                </Button>
                <Button size="sm" disabled={!canContinue} onClick={advance}>
                  {isLastStep ? t("walkthroughs.banner.finish") : t("walkthroughs.banner.continue")}
                </Button>
                {!isLastStep && (
                  <Button size="sm" variant="ghost" onClick={advance}>
                    {t("walkthroughs.banner.skipStep")}
                  </Button>
                )}
                <Button size="sm" variant="ghost" onClick={exit} aria-label={t("walkthroughs.banner.exit")}>
                  {t("walkthroughs.banner.exit")}
                </Button>
              </div>
            </div>
          </div>
        );
      }}
    </StepCheckpoint>
  );
}
