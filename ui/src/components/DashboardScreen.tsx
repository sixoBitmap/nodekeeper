import { useTranslation } from "react-i18next";
import type { Environment } from "@/bindings/Environment";
import type { NodeStatus } from "@/bindings/NodeStatus";
import { Button } from "@/components/ui/button";
import { StatusBadge, type StatusVariant } from "@/components/StatusBadge";
import { DiskMonitor } from "@/components/DiskMonitor";
import { LogViewer } from "@/components/LogViewer";
import { ErrorPanel } from "@/components/ErrorPanel";
import { OrdSection } from "@/components/OrdSection";
import { useDashboardStatus } from "@/hooks/useDashboardStatus";
import { friendlyError } from "@/lib/error-messages";
import { formatBytes, formatUptime } from "@/lib/format";

/**
 * Per-environment dashboard (docs/SPEC.md item 2). Start/stop/restart
 * aren't fund-moving actions, so they don't go through `ConfirmDialog`'s
 * mainnet extra step (item 3/4/10 scope that to actions that move
 * funds) — plain buttons are enough here.
 */
export function DashboardScreen({ environment }: { environment: Environment }) {
  const { t } = useTranslation();
  const { running, status, error, busy, start, stop, restart } = useDashboardStatus(
    environment.chain,
  );

  const statusBadge = statusFor(running, status, t);

  return (
    <div className="space-y-4 p-4">
      <div className="flex items-center justify-between">
        <h2 className="text-lg font-semibold">{t("dashboard.title")}</h2>
        {statusBadge && <StatusBadge label={statusBadge.label} variant={statusBadge.variant} />}
      </div>

      {error &&
        (() => {
          const friendly = friendlyError(error);
          return (
            <ErrorPanel
              title={friendly.title}
              message={friendly.whatToDo ? `${friendly.message} ${friendly.whatToDo}` : friendly.message}
              technicalDetails={error.message}
            />
          );
        })()}

      <div className="flex gap-2">
        <Button disabled={busy || running === true} onClick={() => void start()}>
          {t("dashboard.start")}
        </Button>
        <Button variant="outline" disabled={busy || !running} onClick={() => void stop()}>
          {t("dashboard.stop")}
        </Button>
        <Button variant="outline" disabled={busy || !running} onClick={() => void restart()}>
          {t("dashboard.restart")}
        </Button>
      </div>

      {status ? (
        <>
          <dl className="grid grid-cols-2 gap-y-2 text-sm sm:grid-cols-3">
            <dt className="text-muted-foreground">{t("dashboard.blockHeight")}</dt>
            <dd>
              {status.blocks} / {status.headers}
            </dd>

            <dt className="text-muted-foreground">{t("dashboard.verificationProgress")}</dt>
            <dd>{(status.verification_progress * 100).toFixed(1)}%</dd>

            <dt className="text-muted-foreground">{t("dashboard.peers")}</dt>
            <dd>{status.peers}</dd>

            <dt className="text-muted-foreground">{t("dashboard.mempool")}</dt>
            <dd>
              {t("dashboard.mempoolValue", {
                count: status.mempool_transactions,
                size: formatBytes(status.mempool_bytes),
              })}
            </dd>

            <dt className="text-muted-foreground">{t("dashboard.uptime")}</dt>
            <dd>{formatUptime(status.uptime_seconds)}</dd>
          </dl>

          <DiskMonitor disk={status.disk} />
        </>
      ) : (
        running === false && <p className="text-sm text-muted-foreground">{t("dashboard.notRunning")}</p>
      )}

      <OrdSection chain={environment.chain} />

      <LogViewer chain={environment.chain} />
    </div>
  );
}

function statusFor(
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
