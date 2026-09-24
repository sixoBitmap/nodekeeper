import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { Chain } from "@/bindings/Chain";
import type { TypedError } from "@/bindings/TypedError";
import type { WalletTransactionEntry } from "@/bindings/WalletTransactionEntry";

/**
 * The wallet's transaction history (docs/SPEC.md item 3), backed by
 * the real `wallet_transaction_history` Tauri command. Checked once
 * per mount/chain change, not polled -- same reasoning as
 * `useWalletInscriptions`/`useWalletExists`.
 */
export function useWalletTransactionHistory(chain: Chain) {
  // null = not fetched yet (the loading state).
  const [transactions, setTransactions] = useState<WalletTransactionEntry[] | null>(null);
  const [error, setError] = useState<TypedError | null>(null);

  const refresh = useCallback(() => {
    return invoke<WalletTransactionEntry[]>("wallet_transaction_history", { chain }).then(
      (result) => {
        setTransactions(result);
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

  return { transactions, error, loading: transactions === null && !error, refresh };
}
