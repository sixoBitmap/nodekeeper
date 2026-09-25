import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import type { Chain } from "@/bindings/Chain";
import type { Environment } from "@/bindings/Environment";
import type { TypedError } from "@/bindings/TypedError";
import type { WalletSendResult } from "@/bindings/WalletSendResult";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { ConfirmDialog } from "@/components/ConfirmDialog";
import { ErrorPanel } from "@/components/ErrorPanel";
import { RegtestMineOffer } from "@/components/RegtestMineOffer";
import { friendlyError } from "@/lib/error-messages";
import { formatSats } from "@/lib/format";

/** Regtest has no real fee market (docs/SPEC.md item 3, confirmed live
 * DECISIONS.md Phase 5 VERIFY) -- a small, clearly-labeled fallback so
 * regtest sends aren't blocked on a number that will never arrive. */
const REGTEST_FALLBACK_FEE_RATE = 1;

/** Absurd-fee guard thresholds (docs/SPEC.md item 3: "warn when the fee
 * is unusually high in sat/vB, in absolute terms, or as a share of the
 * amount sent"). No spec-given numbers -- reasonable, clearly-labeled
 * defaults, not yet user-configurable. */
const HIGH_FEE_RATE_SAT_VB = 200;
const HIGH_ABSOLUTE_FEE_SATS = 50_000;
const HIGH_FEE_SHARE_OF_AMOUNT = 0.05;

export function WalletSendForm({
  environment,
  onSent,
  onCancel,
}: {
  environment: Environment;
  onSent: () => void;
  onCancel: () => void;
}) {
  const { t } = useTranslation();
  const chain: Chain = environment.chain;

  const [estimatedFeeRate, setEstimatedFeeRate] = useState<number | null>(null);
  useEffect(() => {
    invoke<number | null>("wallet_fee_estimate", { chain, confTarget: 6 }).then(
      setEstimatedFeeRate,
      () => setEstimatedFeeRate(null),
    );
  }, [chain]);

  const [address, setAddress] = useState("");
  const [amountBtc, setAmountBtc] = useState("");
  const [feeRate, setFeeRate] = useState("");

  const [preview, setPreview] = useState<WalletSendResult | null>(null);
  const [previewError, setPreviewError] = useState<TypedError | null>(null);
  const [previewing, setPreviewing] = useState(false);

  const [confirmOpen, setConfirmOpen] = useState(false);
  const [needsPassphrase, setNeedsPassphrase] = useState(false);
  const [passphraseValue, setPassphraseValue] = useState("");
  const [rememberPassphrase, setRememberPassphrase] = useState(false);
  const [passphraseError, setPassphraseError] = useState<string | undefined>(undefined);
  const [sendError, setSendError] = useState<TypedError | null>(null);
  const [sending, setSending] = useState(false);
  const [sent, setSent] = useState<WalletSendResult | null>(null);

  const effectiveFeeRate =
    feeRate !== "" ? Number(feeRate) : (estimatedFeeRate ?? (chain === "mainnet" ? null : REGTEST_FALLBACK_FEE_RATE));

  const runPreview = () => {
    if (effectiveFeeRate === null) return;
    setPreviewing(true);
    setPreviewError(null);
    setPreview(null);
    invoke<WalletSendResult>("wallet_send_dry_run", {
      chain,
      address,
      asset: `${amountBtc}btc`,
      feeRate: effectiveFeeRate,
    })
      .then(setPreview)
      .catch((e: TypedError) => setPreviewError(e))
      .finally(() => setPreviewing(false));
  };

  const openConfirm = () => {
    setNeedsPassphrase(false);
    setPassphraseValue("");
    setPassphraseError(undefined);
    setSendError(null);
    setConfirmOpen(true);
  };

  const attemptSend = (passphraseToUse: string | null) => {
    if (effectiveFeeRate === null) return;
    setSending(true);
    setPassphraseError(undefined);
    setSendError(null);
    invoke<WalletSendResult>("wallet_send", {
      chain,
      address,
      asset: `${amountBtc}btc`,
      feeRate: effectiveFeeRate,
      passphrase: passphraseToUse,
      remember: rememberPassphrase,
    })
      .then((result) => {
        setConfirmOpen(false);
        setSent(result);
      })
      .catch((e: TypedError) => {
        // Only the "wallet is locked" case (the first attempt, with no
        // passphrase yet) or a retry already in the passphrase step
        // (most likely a wrong passphrase) re-shows the passphrase
        // field -- any other first-attempt failure (e.g. insufficient
        // funds) is a real error unrelated to unlocking, so it's shown
        // plainly instead of implying a passphrase would fix it.
        if (needsPassphrase || e.code === "WALLET_LOCKED") {
          setNeedsPassphrase(true);
          setPassphraseError(e.message);
        } else {
          setConfirmOpen(false);
          setSendError(e);
        }
      })
      .finally(() => setSending(false));
  };

  if (sent) {
    return (
      <div className="space-y-4 rounded-md border border-border p-4">
        <div className="space-y-2 rounded-md border border-border bg-card p-4 text-sm">
          <p className="font-medium text-success">{t("wallet.send.done")}</p>
          <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1">
            <dt className="text-muted-foreground">{t("wallet.send.doneTxid")}</dt>
            <dd className="break-all font-mono text-xs">{sent.txid}</dd>
            <dt className="text-muted-foreground">{t("wallet.send.previewFee")}</dt>
            <dd>{formatSats(sent.fee)}</dd>
          </dl>
        </div>
        <RegtestMineOffer chain={chain} />
        <Button onClick={onSent}>{t("wallet.send.backToWallet")}</Button>
      </div>
    );
  }

  const fee = preview?.fee ?? 0;
  const amountSats = Math.round((Number(amountBtc) || 0) * 100_000_000);
  const feeWarnings = preview
    ? [
        effectiveFeeRate !== null &&
          effectiveFeeRate > HIGH_FEE_RATE_SAT_VB &&
          t("wallet.send.feeWarningRate", { rate: effectiveFeeRate }),
        fee > HIGH_ABSOLUTE_FEE_SATS && t("wallet.send.feeWarningAbsolute", { fee: formatSats(fee) }),
        amountSats > 0 &&
          fee / amountSats > HIGH_FEE_SHARE_OF_AMOUNT &&
          t("wallet.send.feeWarningShare", { percent: Math.round((fee / amountSats) * 100) }),
      ].filter((w): w is string => Boolean(w))
    : [];

  return (
    <div className="space-y-4 rounded-md border border-border p-4">
      <h3 className="text-sm font-semibold">{t("wallet.send.title")}</h3>

      <label className="block text-sm">
        {t("wallet.send.address")}
        <Input
          className="mt-1 font-mono"
          value={address}
          onChange={(e) => {
            setAddress(e.target.value);
            setPreview(null);
            setPreviewError(null);
          }}
          autoComplete="off"
          spellCheck={false}
        />
      </label>

      <label className="block text-sm">
        {t("wallet.send.amount")}
        <Input
          className="mt-1"
          value={amountBtc}
          onChange={(e) => {
            setAmountBtc(e.target.value);
            setPreview(null);
            setPreviewError(null);
          }}
          placeholder="0.001"
          inputMode="decimal"
        />
      </label>

      <label className="block text-sm">
        {t("wallet.send.feeRate")}
        <Input
          className="mt-1"
          value={feeRate}
          onChange={(e) => {
            setFeeRate(e.target.value);
            setPreview(null);
            setPreviewError(null);
          }}
          placeholder={
            estimatedFeeRate !== null
              ? String(estimatedFeeRate)
              : chain === "mainnet"
                ? t("wallet.send.feeRateManualRequired")
                : String(REGTEST_FALLBACK_FEE_RATE)
          }
          inputMode="decimal"
        />
      </label>
      {estimatedFeeRate === null && (
        <p className="text-xs text-muted-foreground">
          {chain === "mainnet"
            ? t("wallet.send.noEstimateMainnet")
            : t("wallet.send.noEstimateRegtest", { rate: REGTEST_FALLBACK_FEE_RATE })}
        </p>
      )}

      {previewError &&
        (() => {
          const friendly = friendlyError(previewError);
          return (
            <ErrorPanel
              title={friendly.title}
              message={friendly.whatToDo ? `${friendly.message} ${friendly.whatToDo}` : friendly.message}
              technicalDetails={previewError.message}
            />
          );
        })()}

      {sendError &&
        (() => {
          const friendly = friendlyError(sendError);
          return (
            <ErrorPanel
              title={friendly.title}
              message={friendly.whatToDo ? `${friendly.message} ${friendly.whatToDo}` : friendly.message}
              technicalDetails={sendError.message}
            />
          );
        })()}

      {preview && (
        <div className="space-y-2 rounded-md border border-border bg-card p-3 text-sm">
          <p className="font-medium">{t("wallet.send.previewTitle")}</p>
          <dl className="grid grid-cols-2 gap-y-1">
            <dt className="text-muted-foreground">{t("wallet.send.previewFee")}</dt>
            <dd>{formatSats(preview.fee)}</dd>
          </dl>
          {feeWarnings.map((warning) => (
            <p key={warning} className="text-warning">
              {warning}
            </p>
          ))}
        </div>
      )}

      <div className="flex gap-2">
        <Button variant="outline" className="flex-1" onClick={onCancel} disabled={sending}>
          {t("wallet.cancel")}
        </Button>
        {preview ? (
          <Button className="flex-1" onClick={openConfirm} disabled={sending}>
            {t("wallet.send.submit")}
          </Button>
        ) : (
          <Button
            className="flex-1"
            onClick={runPreview}
            disabled={previewing || !address || !amountBtc || effectiveFeeRate === null}
          >
            {t("wallet.send.preview")}
          </Button>
        )}
      </div>

      <ConfirmDialog
        open={confirmOpen}
        onOpenChange={setConfirmOpen}
        environment={environment}
        title={t("wallet.send.confirmTitle")}
        description={t("wallet.send.confirmDescription", { address, amount: amountBtc })}
        onConfirm={() => attemptSend(needsPassphrase ? passphraseValue : null)}
        confirmLabel={t("wallet.send.submit")}
        passphrase={
          needsPassphrase
            ? {
                value: passphraseValue,
                onChange: setPassphraseValue,
                remember: rememberPassphrase,
                onRememberChange: setRememberPassphrase,
                error: passphraseError,
              }
            : undefined
        }
      />
    </div>
  );
}
