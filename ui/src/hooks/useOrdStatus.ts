import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { Chain } from "@/bindings/Chain";
import type { OrdStatus } from "@/bindings/OrdStatus";
import type { TypedError } from "@/bindings/TypedError";

const POLL_INTERVAL_MS = 3000;

interface FetchedOrdStatus {
  running: boolean;
  status: OrdStatus | null;
}

/** Same "pure data fetch, no state updates" shape as `useDashboardStatus`'s
 * `fetchStatus` -- see its comment for why (`react-hooks/set-state-in-effect`). */
function fetchOrdStatus(chain: Chain): Promise<FetchedOrdStatus> {
  return invoke<boolean>("is_ord_running", { chain }).then((running) => {
    if (!running) return { running, status: null };
    return invoke<OrdStatus>("ord_status", { chain }).then((status) => ({ running, status }));
  });
}

/**
 * Live status for one environment's ord server, backed by the real
 * `is_ord_running`/`ord_status` Tauri commands (docs/SPEC.md item 2).
 * Mirrors `useDashboardStatus` exactly -- a separate hook, not a shared
 * one, because ord and bitcoind are independently startable/stoppable
 * services (item 2: "Start / stop / restart per service").
 */
export function useOrdStatus(chain: Chain) {
  const [running, setRunning] = useState<boolean | null>(null);
  const [status, setStatus] = useState<OrdStatus | null>(null);
  const [error, setError] = useState<TypedError | null>(null);
  const [busy, setBusy] = useState(false);

  const refresh = useCallback(() => {
    return fetchOrdStatus(chain).then(
      (result) => {
        setRunning(result.running);
        setStatus(result.status);
        setError(null);
      },
      (e: TypedError) => {
        setError(e);
      },
    );
  }, [chain]);

  // Relies on the caller keying its element by `chain`, same as
  // `useDashboardStatus` -- see that hook's comment.
  useEffect(() => {
    fetchOrdStatus(chain).then(
      (result) => {
        setRunning(result.running);
        setStatus(result.status);
        setError(null);
      },
      (e: TypedError) => setError(e),
    );
    const id = window.setInterval(() => void refresh(), POLL_INTERVAL_MS);
    return () => window.clearInterval(id);
  }, [chain, refresh]);

  const runAction = useCallback(
    (command: "start_ord" | "stop_ord" | "restart_ord") => {
      setBusy(true);
      setError(null);
      return invoke(command, { chain })
        .then(() => refresh())
        .catch((e: TypedError) => setError(e))
        .finally(() => setBusy(false));
    },
    [chain, refresh],
  );

  return {
    running,
    status,
    error,
    busy,
    start: () => runAction("start_ord"),
    stop: () => runAction("stop_ord"),
    restart: () => runAction("restart_ord"),
    refresh,
  };
}
