import { useEffect, useRef } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import { isPermissionGranted, requestPermission, sendNotification } from "@tauri-apps/plugin-notification";
import type { Chain } from "@/bindings/Chain";
import type { Environment } from "@/bindings/Environment";
import type { NodeStatus } from "@/bindings/NodeStatus";
import type { OrdStatus } from "@/bindings/OrdStatus";
import { LOW_SPACE_WARNING_BYTES } from "@/components/DiskMonitor";
import { isRealTauriRuntime } from "@/hooks/useDragDropFiles";

// docs/SPEC.md item 8: "Your node is fully synced," "ord is ready,"
// disk space warnings -- these are background events, not urgent
// enough to justify the 3s per-screen poll rate every other status
// hook uses (`useDashboardStatus`/`useOrdStatus`), so this watches all
// environments at once with its own, slower interval instead of
// reusing those hooks.
const POLL_INTERVAL_MS = 15_000;

let permissionRequestedThisSession = false;

/** Requests the OS notification permission once per app session, the
 * first time it's actually needed -- not eagerly on every launch, so a
 * user who never triggers a notification is never prompted. */
async function ensurePermission() {
  if (!isRealTauriRuntime() || permissionRequestedThisSession) return;
  permissionRequestedThisSession = true;
  if (!(await isPermissionGranted())) {
    await requestPermission();
  }
}

async function notify(title: string, body: string) {
  if (!isRealTauriRuntime()) return;
  await ensurePermission();
  if (!(await isPermissionGranted())) return; // denied -- respect it silently, no nagging
  sendNotification({ title, body });
}

/**
 * docs/SPEC.md item 8's notifications, mounted unconditionally in
 * `App.tsx` (like `WalkthroughBanner`/`LiveCommandMonitor`) so they
 * fire regardless of which screen the user currently has open -- a
 * node that finishes syncing while the user is on the Wallet screen
 * should still notify them.
 */
export function NotificationWatcher({ environments }: { environments: Environment[] }) {
  return (
    <>
      {environments.map((env) => (
        <EnvironmentNotificationWatcher key={env.chain} chain={env.chain} environmentName={env.name} />
      ))}
    </>
  );
}

function EnvironmentNotificationWatcher({
  chain,
  environmentName,
}: {
  chain: Chain;
  environmentName: string;
}) {
  const { t } = useTranslation();
  // Only fires "ready"/"synced" on a genuine transition observed this
  // session -- never just because the environment happens to already
  // be caught up the first time this polls (e.g. it finished syncing
  // before the app was even opened).
  const wasSyncing = useRef(false);
  const wasIndexing = useRef(false);
  const warnedDisk = useRef(false);

  useEffect(() => {
    let cancelled = false;

    const poll = async () => {
      const [nodeRunning, ordRunning] = await Promise.all([
        invoke<boolean>("is_node_running", { chain }),
        invoke<boolean>("is_ord_running", { chain }),
      ]);
      if (cancelled) return;

      if (nodeRunning) {
        const status = await invoke<NodeStatus>("node_status", { chain }).catch(() => null);
        if (cancelled || !status) return;

        if (status.initial_block_download) {
          wasSyncing.current = true;
        } else if (wasSyncing.current) {
          wasSyncing.current = false;
          void notify(
            t("notifications.nodeSynced.title"),
            t("notifications.nodeSynced.body", { name: environmentName }),
          );
        }

        const diskLow =
          status.disk.free_on_volume_bytes !== null &&
          status.disk.free_on_volume_bytes < LOW_SPACE_WARNING_BYTES;
        if (diskLow && !warnedDisk.current) {
          warnedDisk.current = true;
          void notify(
            t("notifications.diskLow.title"),
            t("notifications.diskLow.body", { name: environmentName }),
          );
        } else if (!diskLow) {
          warnedDisk.current = false;
        }
      } else {
        wasSyncing.current = false;
      }

      if (ordRunning) {
        const status = await invoke<OrdStatus>("ord_status", { chain }).catch(() => null);
        if (cancelled || !status) return;

        if (!status.caught_up) {
          wasIndexing.current = true;
        } else if (wasIndexing.current) {
          wasIndexing.current = false;
          void notify(
            t("notifications.ordReady.title"),
            t("notifications.ordReady.body", { name: environmentName }),
          );
        }
      } else {
        wasIndexing.current = false;
      }
    };

    void poll();
    const id = window.setInterval(() => void poll(), POLL_INTERVAL_MS);
    return () => {
      cancelled = true;
      window.clearInterval(id);
    };
  }, [chain, environmentName, t]);

  return null;
}
