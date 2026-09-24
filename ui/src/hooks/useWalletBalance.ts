import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { Chain } from "@/bindings/Chain";
import type { TypedError } from "@/bindings/TypedError";
import type { WalletBalance } from "@/bindings/WalletBalance";

interface FetchedWallet {
  balance: WalletBalance;
  address: string;
}

/** Pure data fetch -- no state updates, same reasoning as
 * `useDashboardStatus`'s `fetchStatus` (avoids tripping
 * `react-hooks/set-state-in-effect`). */
function fetchWallet(chain: Chain): Promise<FetchedWallet> {
  return Promise.all([
    invoke<WalletBalance>("wallet_balance", { chain }),
    invoke<string>("wallet_receive_address", { chain }),
  ]).then(([balance, address]) => ({ balance, address }));
}

/**
 * Balance + receive address for an existing wallet (docs/SPEC.md item
 * 3), backed by the real `wallet_balance`/`wallet_receive_address`
 * Tauri commands. Only meaningful once the wallet exists -- the caller
 * (`WalletScreen`) doesn't render this until `useWalletExists` says so.
 */
export function useWalletBalance(chain: Chain) {
  // null = not fetched yet (the loading state), matching the
  // "null = not checked yet" convention used elsewhere (e.g.
  // useDashboardStatus's `running`).
  const [balance, setBalance] = useState<WalletBalance | null>(null);
  const [address, setAddress] = useState<string | null>(null);
  const [error, setError] = useState<TypedError | null>(null);

  const refresh = useCallback(() => {
    return fetchWallet(chain).then(
      (result) => {
        setBalance(result.balance);
        setAddress(result.address);
        setError(null);
      },
      (e: TypedError) => setError(e),
    );
  }, [chain]);

  // Relies on the caller keying its element by `chain`, same pattern
  // as `useDashboardStatus`/`useOrdStatus` -- see their comments.
  useEffect(() => {
    fetchWallet(chain).then(
      (result) => {
        setBalance(result.balance);
        setAddress(result.address);
        setError(null);
      },
      (e: TypedError) => setError(e),
    );
  }, [chain]);

  return { balance, address, error, loading: balance === null && !error, refresh };
}
