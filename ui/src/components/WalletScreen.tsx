import { useState } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import type { Chain } from "@/bindings/Chain";
import type { CreateWalletResult } from "@/bindings/CreateWalletResult";
import type { TypedError } from "@/bindings/TypedError";
import { Button } from "@/components/ui/button";
import { ErrorPanel } from "@/components/ErrorPanel";
import { RestoreWalletForm } from "@/components/RestoreWalletForm";
import { SensitiveSeedView } from "@/components/SensitiveSeedView";
import { WalletBalanceSection } from "@/components/WalletBalanceSection";
import { useWalletExists } from "@/hooks/useWalletExists";
import { friendlyError } from "@/lib/error-messages";

/**
 * The visual ord wallet (docs/SPEC.md item 3), scoped to the current
 * environment. This increment covers create/restore and balance/
 * receive; the inscriptions gallery, send flow, and transaction
 * history are separate, later tasks (PROGRESS.md).
 */
export function WalletScreen({ chain }: { chain: Chain }) {
  const { t } = useTranslation();
  const { exists, error: existsError, refresh } = useWalletExists(chain);

  // Set only right after a successful create -- while non-null, the
  // full-screen SensitiveSeedView takes over instead of anything else
  // in this component. Cleared as soon as the user confirms it (and
  // the caller, i.e. this component, is exactly the "owner" the
  // SensitiveSeedView doc comment says must clear its own copy).
  const [mnemonicToConfirm, setMnemonicToConfirm] = useState<string | null>(null);
  const [restoring, setRestoring] = useState(false);
  const [busy, setBusy] = useState(false);
  const [createError, setCreateError] = useState<TypedError | null>(null);

  const handleCreate = () => {
    setBusy(true);
    setCreateError(null);
    invoke<CreateWalletResult>("create_wallet", { chain })
      .then((result) => setMnemonicToConfirm(result.mnemonic))
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
        <WalletBalanceSection chain={chain} />
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
          ) : (
            <div className="flex gap-2">
              <Button disabled={busy} onClick={handleCreate}>
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
