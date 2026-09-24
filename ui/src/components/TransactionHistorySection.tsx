import { useTranslation } from "react-i18next";
import type { Chain } from "@/bindings/Chain";
import { ErrorPanel } from "@/components/ErrorPanel";
import { useWalletTransactionHistory } from "@/hooks/useWalletTransactionHistory";
import { friendlyError } from "@/lib/error-messages";
import { formatSats } from "@/lib/format";

/** docs/SPEC.md item 3: "Transaction history." */
export function TransactionHistorySection({ chain }: { chain: Chain }) {
  const { t } = useTranslation();
  const { transactions, error, loading } = useWalletTransactionHistory(chain);

  return (
    <div className="space-y-2">
      <h3 className="text-sm font-medium">{t("wallet.history.title")}</h3>

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

      {loading && <p className="text-sm text-muted-foreground">{t("wallet.history.loading")}</p>}

      {!loading && !error && transactions && transactions.length === 0 && (
        <p className="text-sm text-muted-foreground">{t("wallet.history.none")}</p>
      )}

      {!loading && !error && transactions && transactions.length > 0 && (
        <ul className="divide-y divide-border rounded-md border border-border">
          {transactions.map((tx) => (
            <li key={tx.txid} className="flex items-center justify-between gap-4 px-3 py-2 text-sm">
              <div className="min-w-0">
                <code className="block truncate text-xs text-muted-foreground">{tx.txid}</code>
                <span className="text-xs text-muted-foreground">
                  {new Date(tx.time * 1000).toLocaleString()}
                  {tx.generated && ` · ${t("wallet.history.mined")}`}
                  {tx.confirmations <= 0 && ` · ${t("wallet.history.pending")}`}
                </span>
              </div>
              <span
                className={tx.amount_sats < 0 ? "text-foreground" : "text-success"}
              >
                {tx.amount_sats > 0 ? "+" : ""}
                {formatSats(tx.amount_sats)}
              </span>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
