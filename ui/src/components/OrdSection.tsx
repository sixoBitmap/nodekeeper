import { useTranslation } from "react-i18next";
import type { Chain } from "@/bindings/Chain";
import type { OrdStatus } from "@/bindings/OrdStatus";
import { Button } from "@/components/ui/button";
import { StatusBadge, type StatusVariant } from "@/components/StatusBadge";
import { ErrorPanel } from "@/components/ErrorPanel";
import { useOrdStatus } from "@/hooks/useOrdStatus";
import { friendlyError } from "@/lib/error-messages";
import { formatUptime } from "@/lib/format";

/**
 * ord's own section of the per-environment dashboard (docs/SPEC.md item
 * 2: "ord: index height vs node height, indexing / caught-up status,
 * and which index options are enabled"). A separate component (not
 * folded into `DashboardScreen`'s own JSX) and a separate hook
 * (`useOrdStatus`, not `useDashboardStatus`) because ord is an
 * independently startable/stoppable service from bitcoind ("Start /
 * stop / restart per service").
 */
export function OrdSection({ chain }: { chain: Chain }) {
  const { t } = useTranslation();
  const { running, status, error, busy, start, stop, restart } = useOrdStatus(chain);

  const statusBadge = statusFor(running, status, t);

  return (
    <div className="space-y-4 border-t border-border pt-4">
      <div className="flex items-center justify-between">
        <h3 className="text-base font-semibold">{t("ord.title")}</h3>
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
        <dl className="grid grid-cols-2 gap-y-2 text-sm sm:grid-cols-3">
          <dt className="text-muted-foreground">{t("ord.indexHeight")}</dt>
          <dd>
            {status.index_height} / {status.node_height}
          </dd>

          <dt className="text-muted-foreground">{t("ord.indexOptions")}</dt>
          <dd>{indexOptionsLabel(status, t)}</dd>

          <dt className="text-muted-foreground">{t("dashboard.uptime")}</dt>
          <dd>{formatUptime(status.uptime_seconds)}</dd>
        </dl>
      ) : (
        running === false && <p className="text-sm text-muted-foreground">{t("ord.notRunning")}</p>
      )}
    </div>
  );
}

function indexOptionsLabel(status: OrdStatus, t: (key: string) => string): string {
  const enabled = [
    status.index_sats && t("ord.indexOption.sats"),
    status.index_runes && t("ord.indexOption.runes"),
    status.index_addresses && t("ord.indexOption.addresses"),
  ].filter((label): label is string => Boolean(label));
  return enabled.length > 0 ? enabled.join(", ") : t("ord.indexOption.none");
}

function statusFor(
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
