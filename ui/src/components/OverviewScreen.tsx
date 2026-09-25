import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import type { Environment } from "@/bindings/Environment";
import type { NodeStatus } from "@/bindings/NodeStatus";
import type { OrdStatus } from "@/bindings/OrdStatus";
import type { SystemCheck } from "@/bindings/SystemCheck";
import type { TypedError } from "@/bindings/TypedError";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { ErrorPanel } from "@/components/ErrorPanel";
import { StatusBadge, type StatusVariant } from "@/components/StatusBadge";
import { useDashboardStatus } from "@/hooks/useDashboardStatus";
import { useOrdStatus } from "@/hooks/useOrdStatus";
import { chainBgClass, chainTextClass } from "@/lib/environment-colors";
import { friendlyError } from "@/lib/error-messages";
import { formatBytes } from "@/lib/format";
import { usePreventSleepStore } from "@/store/preventSleep";

const POLL_INTERVAL_MS = 3000;

/**
 * "All environments" overview (docs/SPEC.md item 10): status side by
 * side, start/stop per environment, "stop all". Not scoped to the
 * currently-selected environment the way every other screen is --
 * this is the one place that intentionally shows all of them at once.
 */
export function OverviewScreen({ environments }: { environments: Environment[] }) {
  const { t } = useTranslation();
  const [systemCheck, setSystemCheck] = useState<SystemCheck | null>(null);
  const [runningCount, setRunningCount] = useState(0);
  const [stoppingAll, setStoppingAll] = useState(false);
  const preventSleepEnabled = usePreventSleepStore((s) => s.enabled);
  const preventSleepLoaded = usePreventSleepStore((s) => s.loaded);
  const setPreventSleepEnabled = usePreventSleepStore((s) => s.setEnabled);
  const [portable, setPortable] = useState(false);
  const [ejecting, setEjecting] = useState(false);
  const [ejected, setEjected] = useState(false);
  const [ejectError, setEjectError] = useState<TypedError | null>(null);

  useEffect(() => {
    void invoke<boolean>("is_portable_mode").then(setPortable);
  }, []);

  const safeEject = async () => {
    setEjecting(true);
    setEjectError(null);
    setEjected(false);
    try {
      await invoke("safe_eject");
      setEjected(true);
      // `safe_eject` succeeding already confirms nothing is running --
      // set this immediately rather than waiting for the next 3s poll
      // tick to agree, so "safe to unplug" isn't briefly hidden behind
      // a stale `runningCount`.
      setRunningCount(0);
    } catch (e) {
      setEjectError(e as TypedError);
    } finally {
      setEjecting(false);
    }
  };

  useEffect(() => {
    let cancelled = false;
    const poll = () => {
      void invoke<string>("get_environment_data_root").then((dir) =>
        invoke<SystemCheck>("system_check", { dataDir: dir }).then((check) => {
          if (!cancelled) setSystemCheck(check);
        }),
      );
      void Promise.all(
        environments.flatMap((env) => [
          invoke<boolean>("is_node_running", { chain: env.chain }),
          invoke<boolean>("is_ord_running", { chain: env.chain }),
        ]),
      ).then((flags) => {
        if (!cancelled) setRunningCount(flags.filter(Boolean).length);
      });
    };
    poll();
    const id = window.setInterval(poll, POLL_INTERVAL_MS);
    return () => {
      cancelled = true;
      window.clearInterval(id);
    };
  }, [environments]);

  const stopAll = async () => {
    setStoppingAll(true);
    try {
      await Promise.allSettled(
        environments.flatMap((env) => [
          invoke("stop_node", { chain: env.chain }),
          invoke("stop_ord", { chain: env.chain }),
        ]),
      );
    } finally {
      setStoppingAll(false);
    }
  };

  // "Safe to unplug" only shows while nothing has started running
  // again since the eject -- derived at render time (not reset via a
  // separate effect) so it can never lag a poll tick behind reality;
  // showing it after something restarted would be actively misleading
  // for a portable drive.
  const showSafeToUnplug = ejected && runningCount === 0;

  // Resource-warning threshold: a precise per-process RAM figure needs
  // new backend instrumentation (tracking each spawned bitcoind/ord
  // PID's own memory via sysinfo) not built yet -- this uses
  // system-wide available RAM plus how many services are running as
  // the signal instead (docs/SPEC.md item 10, scoped down; tracked in
  // PROGRESS.md).
  const lowMemory = systemCheck ? systemCheck.available_memory_bytes < 2 * 1024 ** 3 : false;
  const showResourceWarning = runningCount >= 4 && lowMemory;

  return (
    <div className="space-y-4 p-4">
      <div className="flex items-center justify-between">
        <h2 className="text-lg font-semibold">{t("overview.title")}</h2>
        <Button variant="outline" size="sm" disabled={stoppingAll} onClick={() => void stopAll()}>
          {t("overview.stopAll")}
        </Button>
      </div>

      {portable && (
        <div className="space-y-2 rounded-md border-2 border-primary/40 bg-primary/5 p-3">
          <h3 className="text-sm font-semibold">{t("overview.safeEject.title")}</h3>
          <p className="text-xs text-muted-foreground">{t("overview.safeEject.hint")}</p>
          {ejectError &&
            (() => {
              const friendly = friendlyError(ejectError);
              return (
                <ErrorPanel
                  title={friendly.title}
                  message={friendly.whatToDo ? `${friendly.message} ${friendly.whatToDo}` : friendly.message}
                  technicalDetails={ejectError.message}
                />
              );
            })()}
          {showSafeToUnplug ? (
            <p className="text-sm font-medium text-success">{t("overview.safeEject.safeToUnplug")}</p>
          ) : (
            <Button size="sm" disabled={ejecting} onClick={() => void safeEject()}>
              {ejecting ? t("overview.safeEject.ejecting") : t("overview.safeEject.button")}
            </Button>
          )}
        </div>
      )}

      {systemCheck && (
        <div className="rounded-md border border-border p-3 text-sm">
          <p className="text-muted-foreground">
            {t("overview.resourceSummary", {
              running: runningCount,
              available: formatBytes(systemCheck.available_memory_bytes),
            })}
          </p>
          {showResourceWarning && (
            <p className="mt-1 text-danger">{t("overview.resourceWarning")}</p>
          )}
        </div>
      )}

      <label className="flex items-center gap-2 text-sm">
        <Checkbox
          checked={preventSleepEnabled}
          disabled={!preventSleepLoaded}
          onCheckedChange={(v) => setPreventSleepEnabled(v === true)}
        />
        {t("overview.preventSleep")}
      </label>
      <p className="-mt-2 text-xs text-muted-foreground">{t("overview.preventSleepHint")}</p>

      <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
        {environments.map((env) => (
          <EnvironmentOverviewCard key={env.chain} environment={env} />
        ))}
      </div>
    </div>
  );
}

function EnvironmentOverviewCard({ environment }: { environment: Environment }) {
  const { t } = useTranslation();
  const node = useDashboardStatus(environment.chain);
  const ord = useOrdStatus(environment.chain);

  const nodeBadge = nodeStatusBadge(node.running, node.status, t);
  const ordBadge = ordStatusBadge(ord.running, ord.status, t);

  return (
    <div className="space-y-3 rounded-md border border-border p-3">
      <div className="flex items-center gap-2">
        <span className={`h-2.5 w-2.5 rounded-full ${chainBgClass(environment.chain)}`} aria-hidden="true" />
        <h3 className={`text-sm font-semibold ${chainTextClass(environment.chain)}`}>{environment.name}</h3>
      </div>

      <div className="space-y-1.5">
        <div className="flex items-center justify-between">
          <span className="text-xs text-muted-foreground">{t("dashboard.title")}</span>
          {nodeBadge && <StatusBadge label={nodeBadge.label} variant={nodeBadge.variant} />}
        </div>
        <div className="flex gap-2">
          <Button size="sm" variant="outline" disabled={node.busy || node.running === true} onClick={() => void node.start()}>
            {t("dashboard.start")}
          </Button>
          <Button size="sm" variant="outline" disabled={node.busy || !node.running} onClick={() => void node.stop()}>
            {t("dashboard.stop")}
          </Button>
        </div>
        {node.status && (
          <p className="text-xs text-muted-foreground">
            {t("overview.diskUsed", { size: formatBytes(node.status.disk.used_by_data_bytes) })}
          </p>
        )}
      </div>

      <div className="space-y-1.5 border-t border-border pt-2">
        <div className="flex items-center justify-between">
          <span className="text-xs text-muted-foreground">{t("ord.title")}</span>
          {ordBadge && <StatusBadge label={ordBadge.label} variant={ordBadge.variant} />}
        </div>
        <div className="flex gap-2">
          <Button size="sm" variant="outline" disabled={ord.busy || ord.running === true} onClick={() => void ord.start()}>
            {t("dashboard.start")}
          </Button>
          <Button size="sm" variant="outline" disabled={ord.busy || !ord.running} onClick={() => void ord.stop()}>
            {t("dashboard.stop")}
          </Button>
        </div>
      </div>
    </div>
  );
}

function nodeStatusBadge(
  running: boolean | null,
  status: NodeStatus | null,
  t: (key: string) => string,
): { label: string; variant: StatusVariant } | null {
  if (running === null) return null;
  if (!running) return { label: t("dashboard.status.stopped"), variant: "neutral" };
  if (!status) return { label: t("dashboard.status.starting"), variant: "neutral" };
  if (status.initial_block_download) return { label: t("dashboard.status.syncing"), variant: "warning" };
  return { label: t("dashboard.status.ready"), variant: "success" };
}

function ordStatusBadge(
  running: boolean | null,
  status: OrdStatus | null,
  t: (key: string) => string,
): { label: string; variant: StatusVariant } | null {
  if (running === null) return null;
  if (!running) return { label: t("ord.status.stopped"), variant: "neutral" };
  if (!status) return { label: t("ord.status.starting"), variant: "neutral" };
  if (!status.caught_up) return { label: t("ord.status.indexing"), variant: "warning" };
  return { label: t("ord.status.caughtUp"), variant: "success" };
}
