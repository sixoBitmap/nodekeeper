import { useState } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import type { Chain } from "@/bindings/Chain";
import type { TypedError } from "@/bindings/TypedError";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Input } from "@/components/ui/input";
import { ErrorPanel } from "@/components/ErrorPanel";
import { friendlyError } from "@/lib/error-messages";

/**
 * Restore-from-mnemonic form (docs/SPEC.md item 3). Unlike
 * `SensitiveSeedView` (which *displays* a freshly generated mnemonic),
 * this is an *input* -- the user already has their own words, so there's
 * nothing to reveal or quiz them on, just a place to type it that never
 * logs or persists what's typed beyond this component's own state,
 * cleared the moment restore succeeds or the user cancels.
 */
export function RestoreWalletForm({
  chain,
  onRestored,
  onCancel,
}: {
  chain: Chain;
  onRestored: () => void;
  onCancel: () => void;
}) {
  const { t } = useTranslation();
  const [mnemonic, setMnemonic] = useState("");
  // Full rescan ("0") for a wallet with existing history; "now" skips
  // scanning entirely for a wallet with nothing to find yet -- see
  // nk_ord::wallet::restore_wallet's doc comment for why these are the
  // two timestamps that matter here.
  const [hasHistory, setHasHistory] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<TypedError | null>(null);

  // docs/SPEC.md SECURITY RULES: "All mainnet wallets are encrypted."
  // Restoring creates a fresh, initially unencrypted local Core wallet
  // just like create does, so it needs the same passphrase -- enforced
  // backend-side too (Phase 5 security self-review, DECISIONS.md), this
  // is the UI half.
  const isMainnet = chain === "mainnet";
  const [passphrase, setPassphrase] = useState("");
  const [passphraseConfirm, setPassphraseConfirm] = useState("");
  const passphraseMismatch =
    isMainnet && passphrase.length > 0 && passphrase !== passphraseConfirm;

  const submit = () => {
    setBusy(true);
    setError(null);
    invoke("restore_wallet", {
      chain,
      mnemonic: mnemonic.trim(),
      timestamp: hasHistory ? "0" : "now",
      passphrase: isMainnet ? passphrase : null,
    })
      .then(() => {
        setMnemonic("");
        setPassphrase("");
        setPassphraseConfirm("");
        onRestored();
      })
      .catch((e: TypedError) => setError(e))
      .finally(() => setBusy(false));
  };

  const wordCount = mnemonic.trim().split(/\s+/).filter(Boolean).length;
  const canSubmit =
    wordCount >= 12 && (!isMainnet || (passphrase.length > 0 && !passphraseMismatch));

  return (
    <div className="space-y-3 rounded-md border border-border p-4">
      <h3 className="text-sm font-semibold">{t("wallet.restoreTitle")}</h3>

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

      <textarea
        className="w-full rounded-md border border-input bg-transparent p-2 font-mono text-sm outline-none focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50"
        rows={3}
        placeholder={t("wallet.restorePlaceholder")}
        value={mnemonic}
        onChange={(e) => setMnemonic(e.target.value)}
        autoComplete="off"
        autoCorrect="off"
        spellCheck={false}
      />
      <p className="text-xs text-muted-foreground">
        {t("wallet.restoreWordCount", { count: wordCount })}
      </p>

      <label className="flex items-center gap-2 text-sm">
        <Checkbox checked={hasHistory} onCheckedChange={(c) => setHasHistory(c === true)} />
        {t("wallet.restoreHasHistory")}
      </label>

      {isMainnet && (
        <div className="space-y-2 rounded-md border border-border bg-muted/30 p-3">
          <p className="text-xs text-muted-foreground">{t("wallet.encryption.mainnetNotice")}</p>
          <label className="block space-y-1 text-sm">
            <span>{t("wallet.encryption.passphraseLabel")}</span>
            <Input
              type="password"
              value={passphrase}
              onChange={(e) => setPassphrase(e.target.value)}
              autoComplete="new-password"
            />
          </label>
          <label className="block space-y-1 text-sm">
            <span>{t("wallet.encryption.passphraseConfirmLabel")}</span>
            <Input
              type="password"
              value={passphraseConfirm}
              onChange={(e) => setPassphraseConfirm(e.target.value)}
              autoComplete="new-password"
            />
          </label>
          {passphraseMismatch && (
            <p className="text-xs text-destructive">{t("wallet.encryption.mismatch")}</p>
          )}
        </div>
      )}

      <div className="flex gap-2">
        <Button variant="outline" className="flex-1" onClick={onCancel} disabled={busy}>
          {t("wallet.cancel")}
        </Button>
        <Button className="flex-1" disabled={busy || !canSubmit} onClick={submit}>
          {t("wallet.restoreSubmit")}
        </Button>
      </div>
    </div>
  );
}
