import { QRCodeSVG } from "qrcode.react";
import { useTranslation } from "react-i18next";
import type { Chain } from "@/bindings/Chain";
import { ErrorPanel } from "@/components/ErrorPanel";
import { useWalletBalance } from "@/hooks/useWalletBalance";
import { friendlyError } from "@/lib/error-messages";
import { formatSats } from "@/lib/format";

/**
 * docs/SPEC.md item 3: "Balance: cardinal vs inscribed sats" and
 * "Receive: address with a QR code."
 */
export function WalletBalanceSection({ chain }: { chain: Chain }) {
  const { t } = useTranslation();
  const { balance, address, error, loading } = useWalletBalance(chain);

  if (error) {
    const friendly = friendlyError(error);
    return (
      <ErrorPanel
        title={friendly.title}
        message={friendly.whatToDo ? `${friendly.message} ${friendly.whatToDo}` : friendly.message}
        technicalDetails={error.message}
      />
    );
  }

  if (loading || !balance || !address) {
    return <p className="text-sm text-muted-foreground">{t("wallet.loadingBalance")}</p>;
  }

  return (
    <div className="grid gap-6 sm:grid-cols-2">
      <dl className="grid grid-cols-2 gap-y-2 text-sm">
        <dt className="text-muted-foreground">{t("wallet.cardinalBalance")}</dt>
        <dd>{formatSats(balance.cardinal)}</dd>

        <dt className="text-muted-foreground">{t("wallet.inscribedBalance")}</dt>
        <dd>{formatSats(balance.ordinal)}</dd>

        <dt className="text-muted-foreground">{t("wallet.totalBalance")}</dt>
        <dd className="font-medium">{formatSats(balance.total)}</dd>
      </dl>

      <div className="flex flex-col items-start gap-2">
        <p className="text-sm text-muted-foreground">{t("wallet.receiveAddress")}</p>
        <div className="rounded-md border border-border bg-card p-3">
          <QRCodeSVG value={address} size={144} />
        </div>
        <code className="break-all rounded bg-muted px-2 py-1 text-xs">{address}</code>
      </div>

      {/* docs/SPEC.md item 3: rune balances only shown when the running
          ord server's runes index is enabled (Foundation F) -- `null`
          means the index is off, not that the wallet owns none. */}
      {balance.runes !== null && (
        <div className="space-y-1 border-t border-border pt-3 sm:col-span-2">
          <p className="text-sm text-muted-foreground">{t("wallet.runes.title")}</p>
          {balance.runes.length === 0 ? (
            <p className="text-sm text-muted-foreground">{t("wallet.runes.none")}</p>
          ) : (
            <ul className="space-y-1 text-sm">
              {balance.runes.map((rune) => (
                <li key={rune.name} className="flex justify-between gap-4">
                  <span>{rune.name}</span>
                  <code className="text-xs text-muted-foreground">{rune.raw}</code>
                </li>
              ))}
            </ul>
          )}
          <p className="text-xs text-muted-foreground">{t("wallet.runes.viewOnlyNotice")}</p>
        </div>
      )}
    </div>
  );
}
