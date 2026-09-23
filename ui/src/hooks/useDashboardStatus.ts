import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { Chain } from "@/bindings/Chain";
import type { NodeStatus } from "@/bindings/NodeStatus";
import type { TypedError } from "@/bindings/TypedError";

const POLL_INTERVAL_MS = 3000;

interface FetchedStatus {
  running: boolean;
  status: NodeStatus | null;
}

/** Pure data fetch -- no state updates, so it's safe to call from
 * anywhere (an effect, an event handler, another fetch) without
 * tripping `react-hooks/set-state-in-effect`, which flags setState
 * calls reachable synchronously from an effect body. */
function fetchStatus(chain: Chain): Promise<FetchedStatus> {
  return invoke<boolean>("is_node_running", { chain }).then((running) => {
    if (!running) return { running, status: null };
    return invoke<NodeStatus>("node_status", { chain }).then((status) => ({ running, status }));
  });
}

/**
 * Live status for one environment's node, backed by the real
 * `is_node_running`/`node_status` Tauri commands (docs/SPEC.md item 2).
 * Polls while mounted; also exposes start/stop/restart wired to the
 * same backend so a screen doesn't have to duplicate this wiring.
 */
export function useDashboardStatus(chain: Chain) {
  // null = not checked yet (the initial-load state, distinct from
  // "checked and confirmed not running").
  const [running, setRunning] = useState<boolean | null>(null);
  const [status, setStatus] = useState<NodeStatus | null>(null);
  const [error, setError] = useState<TypedError | null>(null);
  const [busy, setBusy] = useState(false);

  const refresh = useCallback(() => {
    return fetchStatus(chain).then(
      (result) => {
        setRunning(result.running);
        setStatus(result.status);
        setError(null);
      },
      (e: TypedError) => {
        // A status fetch can race a node that just stopped -- surface
        // the error but don't clear a still-useful last-known status.
        setError(e);
      },
    );
  }, [chain]);

  // Relies on the caller keying its element by `chain` (e.g. `key={chain}`)
  // to get fresh initial state on an environment switch, rather than
  // resetting state synchronously here -- see the note on `LogViewer`
  // for the same pattern and why.
  useEffect(() => {
    fetchStatus(chain).then(
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
    (command: "start_node" | "stop_node" | "restart_node") => {
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
    start: () => runAction("start_node"),
    stop: () => runAction("stop_node"),
    restart: () => runAction("restart_node"),
    refresh,
  };
}
