import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { Chain } from "@/bindings/Chain";
import type { TypedError } from "@/bindings/TypedError";

/**
 * Whether `chain` already has a wallet (docs/SPEC.md item 3), backed by
 * the real `wallet_exists` Tauri command. Checked once per mount/chain
 * change, not polled -- unlike node/ord status, wallet existence
 * doesn't change on its own; `refresh` lets the caller re-check after
 * a create/restore succeeds.
 */
export function useWalletExists(chain: Chain) {
  // null = not checked yet.
  const [exists, setExists] = useState<boolean | null>(null);
  const [error, setError] = useState<TypedError | null>(null);

  const refresh = useCallback(() => {
    return invoke<boolean>("wallet_exists", { chain }).then(
      (result) => {
        setExists(result);
        setError(null);
      },
      (e: TypedError) => setError(e),
    );
  }, [chain]);

  // Relies on the caller keying its element by `chain`, same pattern
  // as `useDashboardStatus`/`useOrdStatus` -- see their comments.
  useEffect(() => {
    void refresh();
  }, [refresh]);

  return { exists, error, refresh };
}
