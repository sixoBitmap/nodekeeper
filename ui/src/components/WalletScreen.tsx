import { useState } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import type { CreateWalletResult } from "@/bindings/CreateWalletResult";
import type { Environment } from "@/bindings/Environment";
import type { TypedError } from "@/bindings/TypedError";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { ErrorPanel } from "@/components/ErrorPanel";
import { InscriptionGallery } from "@/components/InscriptionGallery";
import { RestoreWalletForm } from "@/components/RestoreWalletForm";
import { TransactionHistorySection } from "@/components/TransactionHistorySection";
import { SensitiveSeedView } from "@/components/SensitiveSeedView";
import { WalletBalanceSection } from "@/components/WalletBalanceSection";
import { WalletSendForm } from "@/components/WalletSendForm";
import { useWalletExists } from "@/hooks/useWalletExists";
import { friendlyError } from "@/lib/error-messages";

/**
 * The visual ord wallet (docs/SPEC.md item 3), scoped to the current
 * environment: create/restore, balance/receive, send, the inscriptions
 * gallery, and transaction history.
 */
export function WalletScreen({ environment }: { environment: Environment }) {
  const { t } = useTranslation();
  const chain = environment.chain;
  const isMainnet = chain === "mainnet";
  const { exists, error: existsError, refresh } = useWalletExists(chain);
  const [sending, setSending] = useState(false);

  // Set only right after a successful create -- while non-null, the
  // full-screen SensitiveSeedView takes over instead of anything else
  // in this component. Cleared as soon as the user confirms it (and
  // the caller, i.e. this component, is exactly the "owner" the
  // SensitiveSeedView doc comment says must clear its own copy).
  const [mnemonicToConfirm, setMnemonicToConfirm] = useState<string | null>(null);
  const [restoring, setRestoring] = useState(false);
  // docs/SPEC.md SECURITY RULES: "All mainnet wallets are encrypted" --
  // on mainnet, clicking "Create wallet" reveals this passphrase step
  // instead of creating immediately; enforced backend-side too (Phase 5
  // security self-review, DECISIONS.md).
  const [settingPassphrase, setSettingPassphrase] = useState(false);
  const [passphrase, setPassphrase] = useState("");
  const [passphraseConfirm, setPassphraseConfirm] = useState("");
  const passphraseMismatch = passphrase.length > 0 && passphrase !== passphraseConfirm;
  const [busy, setBusy] = useState(false);
  const [createError, setCreateError] = useState<TypedError | null>(null);

  const handleCreate = (encryptionPassphrase: string | null) => {
    setBusy(true);
    setCreateError(null);
    invoke<CreateWalletResult>("create_wallet", { chain, passphrase: encryptionPassphrase })
      .then((result) => {
        setSettingPassphrase(false);
        setPassphrase("");
        setPassphraseConfirm("");
        setMnemonicToConfirm(result.mnemonic);
      })
      .catch((e: TypedError) => setCreateError(e))
      .finally(() => setBusy(false));
  };

  const handleSeedConfirmed = () => {
    setMnemonicToConfirm(null);
    void refresh();
  };

  const handleRestored = () => {
    setRestoring(false);
    void refresh();
  };

  if (mnemonicToConfirm) {
    return (
      <SensitiveSeedView
        words={mnemonicToConfirm.trim().split(/\s+/)}
        onDone={handleSeedConfirmed}
      />
    );
  }

  return (
    <div className="space-y-4 p-4">
      <h2 className="text-lg font-semibold">{t("wallet.title")}</h2>

      {existsError &&
        (() => {
          const friendly = friendlyError(existsError);
          return (
            <ErrorPanel
              title={friendly.title}
              message={friendly.whatToDo ? `${friendly.message} ${friendly.whatToDo}` : friendly.message}
              technicalDetails={existsError.message}
            />
          );
        })()}

      {exists === null ? (
        <p className="text-sm text-muted-foreground">{t("wallet.checking")}</p>
      ) : exists ? (
        <div className="space-y-4">
          <WalletBalanceSection chain={chain} />
          <InscriptionGallery environment={environment} />
          <TransactionHistorySection chain={chain} />
          {sending ? (
            <WalletSendForm
              environment={environment}
              onSent={() => setSending(false)}
              onCancel={() => setSending(false)}
            />
          ) : (
            <Button onClick={() => setSending(true)}>{t("wallet.send.title")}</Button>
          )}
        </div>
      ) : (
        <div className="space-y-4">
          <p className="text-sm text-muted-foreground">{t("wallet.noneYet")}</p>

          {createError &&
            (() => {
              const friendly = friendlyError(createError);
              return (
                <ErrorPanel
                  title={friendly.title}
                  message={
                    friendly.whatToDo ? `${friendly.message} ${friendly.whatToDo}` : friendly.message
                  }
                  technicalDetails={createError.message}
                />
              );
            })()}

          {restoring ? (
            <RestoreWalletForm
              chain={chain}
              onRestored={handleRestored}
              onCancel={() => setRestoring(false)}
            />
          ) : settingPassphrase ? (
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
              <div className="flex gap-2">
                <Button
                  variant="outline"
                  className="flex-1"
                  disabled={busy}
                  onClick={() => setSettingPassphrase(false)}
                >
                  {t("wallet.cancel")}
                </Button>
                <Button
                  className="flex-1"
                  disabled={busy || passphrase.length === 0 || passphraseMismatch}
                  onClick={() => handleCreate(passphrase)}
                >
                  {t("wallet.create")}
                </Button>
              </div>
            </div>
          ) : (
            <div className="flex gap-2">
              <Button
                disabled={busy}
                onClick={() => (isMainnet ? setSettingPassphrase(true) : handleCreate(null))}
              >
                {t("wallet.create")}
              </Button>
              <Button variant="outline" disabled={busy} onClick={() => setRestoring(true)}>
                {t("wallet.restore")}
              </Button>
            </div>
          )}
        </div>
      )}
    </div>
  );
}
