import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { Chain } from "@/bindings/Chain";
import type { TypedError } from "@/bindings/TypedError";
import type { WalletInscriptionEntry } from "@/bindings/WalletInscriptionEntry";

/**
 * The wallet's inscriptions (docs/SPEC.md item 3's gallery), backed by
 * the real `wallet_inscriptions` Tauri command. Checked once per
 * mount/chain change, not polled -- same reasoning as
 * `useWalletExists`: nothing changes it outside of a send/receive the
 * user just did, and the caller can `refresh` after those.
 */
export function useWalletInscriptions(chain: Chain) {
  // null = not fetched yet (the loading state).
  const [inscriptions, setInscriptions] = useState<WalletInscriptionEntry[] | null>(null);
  const [error, setError] = useState<TypedError | null>(null);

  const refresh = useCallback(() => {
    return invoke<WalletInscriptionEntry[]>("wallet_inscriptions", { chain }).then(
      (result) => {
        setInscriptions(result);
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

  return { inscriptions, error, loading: inscriptions === null && !error, refresh };
}
