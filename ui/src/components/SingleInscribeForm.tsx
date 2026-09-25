import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import type { Chain } from "@/bindings/Chain";
import type { Environment } from "@/bindings/Environment";
import type { FilePreview } from "@/bindings/FilePreview";
import type { InscribeResult } from "@/bindings/InscribeResult";
import type { TypedError } from "@/bindings/TypedError";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { ConfirmDialog } from "@/components/ConfirmDialog";
import { ErrorPanel } from "@/components/ErrorPanel";
import { RegtestMineOffer } from "@/components/RegtestMineOffer";
import { isRealTauriRuntime, useDragDropFiles } from "@/hooks/useDragDropFiles";
import { friendlyError } from "@/lib/error-messages";
import { formatBytes, formatSats } from "@/lib/format";

/** Regtest has no real fee market (same reasoning/precedent as
 * `WalletSendForm`, docs/SPEC.md item 3/4, DECISIONS.md Phase 5 VERIFY). */
const REGTEST_FALLBACK_FEE_RATE = 1;
const HIGH_FEE_RATE_SAT_VB = 200;
const HIGH_ABSOLUTE_FEE_SATS = 50_000;
/** docs/SPEC.md item 4: "size warning." No spec-given number -- a
 * reasonable, clearly-labeled default, same precedent as the fee guard
 * thresholds above (not yet user-configurable). */
const LARGE_FILE_WARNING_BYTES = 400_000;

/**
 * docs/SPEC.md item 4's Inscribe studio, single-file mode. Structurally
 * mirrors `WalletSendForm`: fee-rate picker with the same fee guard,
 * dry-run preview, and the shared `ConfirmDialog` for the mainnet step
 * and passphrase unlock -- no screen implements its own confirmation
 * flow.
 */
export function SingleInscribeForm({ environment }: { environment: Environment }) {
  const { t } = useTranslation();
  const chain: Chain = environment.chain;

  const [filePath, setFilePath] = useState<string | null>(null);
  const [filePreview, setFilePreview] = useState<FilePreview | null>(null);
  const [fileError, setFileError] = useState<TypedError | null>(null);

  const [estimatedFeeRate, setEstimatedFeeRate] = useState<number | null>(null);
  const [feeRate, setFeeRate] = useState("");
  const [showAdvanced, setShowAdvanced] = useState(false);
  const [postage, setPostage] = useState("");
  const [parent, setParent] = useState("");

  const [preview, setPreview] = useState<InscribeResult | null>(null);
  const [previewError, setPreviewError] = useState<TypedError | null>(null);
  const [previewing, setPreviewing] = useState(false);

  const [confirmOpen, setConfirmOpen] = useState(false);
  const [needsPassphrase, setNeedsPassphrase] = useState(false);
  const [passphraseValue, setPassphraseValue] = useState("");
  const [rememberPassphrase, setRememberPassphrase] = useState(false);
  const [passphraseError, setPassphraseError] = useState<string | undefined>(undefined);
  const [inscribeError, setInscribeError] = useState<TypedError | null>(null);
  const [inscribing, setInscribing] = useState(false);
  const [inscribed, setInscribed] = useState<InscribeResult | null>(null);

  const loadFile = (path: string) => {
    setFileError(null);
    setFilePreview(null);
    setInscribed(null);
    setPreview(null);
    setPreviewError(null);
    setFilePath(path);
    invoke<FilePreview>("inscribe_file_preview", { path })
      .then(setFilePreview)
      .catch((e: TypedError) => setFileError(e));
  };

  const { dragOver } = useDragDropFiles((paths) => loadFile(paths[0]));

  useEffect(() => {
    invoke<number | null>("wallet_fee_estimate", { chain, confTarget: 6 }).then(
      setEstimatedFeeRate,
      () => setEstimatedFeeRate(null),
    );
  }, [chain]);

  const effectiveFeeRate =
    feeRate !== "" ? Number(feeRate) : (estimatedFeeRate ?? (chain === "mainnet" ? null : REGTEST_FALLBACK_FEE_RATE));
  const postageSats = postage !== "" ? Math.round(Number(postage)) : undefined;
  const parentId = parent.trim() !== "" ? parent.trim() : undefined;

  const runPreview = () => {
    if (effectiveFeeRate === null || !filePath) return;
    setPreviewing(true);
    setPreviewError(null);
    setPreview(null);
    invoke<InscribeResult>("wallet_inscribe_dry_run", {
      chain,
      filePath,
      feeRate: effectiveFeeRate,
      postage: postageSats,
      parent: parentId,
      reinscribeSatpoint: null,
    })
      .then(setPreview)
      .catch((e: TypedError) => setPreviewError(e))
      .finally(() => setPreviewing(false));
  };

  const openConfirm = () => {
    setNeedsPassphrase(false);
    setPassphraseValue("");
    setPassphraseError(undefined);
    setInscribeError(null);
    setConfirmOpen(true);
  };

  const attemptInscribe = (passphraseToUse: string | null) => {
    if (effectiveFeeRate === null || !filePath) return;
    setInscribing(true);
    setPassphraseError(undefined);
    setInscribeError(null);
    invoke<InscribeResult>("wallet_inscribe", {
      chain,
      filePath,
      feeRate: effectiveFeeRate,
      postage: postageSats,
      parent: parentId,
      reinscribeSatpoint: null,
      passphrase: passphraseToUse,
      remember: rememberPassphrase,
    })
      .then((result) => {
        setConfirmOpen(false);
        setInscribed(result);
      })
      .catch((e: TypedError) => {
        // Same reasoning as WalletSendForm: only re-prompt for a
        // passphrase on the locked-wallet case, not any other failure.
        if (needsPassphrase || e.code === "WALLET_LOCKED") {
          setNeedsPassphrase(true);
          setPassphraseError(e.message);
        } else {
          setConfirmOpen(false);
          setInscribeError(e);
        }
      })
      .finally(() => setInscribing(false));
  };

  const reset = () => {
    setFilePath(null);
    setFilePreview(null);
    setFileError(null);
    setFeeRate("");
    setPostage("");
    setParent("");
    setPreview(null);
    setPreviewError(null);
    setInscribed(null);
    setInscribeError(null);
  };

  if (inscribed) {
    return (
      <div className="space-y-4">
        <div className="space-y-2 rounded-md border border-border bg-card p-4 text-sm">
          <p className="font-medium text-success">{t("inscribe.done")}</p>
          <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1">
            <dt className="text-muted-foreground">{t("inscribe.doneId")}</dt>
            <dd className="break-all font-mono text-xs">{inscribed.id}</dd>
            <dt className="text-muted-foreground">{t("inscribe.previewFee")}</dt>
            <dd>{formatSats(inscribed.fee)}</dd>
          </dl>
        </div>
        <RegtestMineOffer chain={chain} />
        <Button onClick={reset}>{t("inscribe.inscribeAnother")}</Button>
      </div>
    );
  }

  const fee = preview?.fee ?? 0;
  const feeWarnings = preview
    ? [
        effectiveFeeRate !== null &&
          effectiveFeeRate > HIGH_FEE_RATE_SAT_VB &&
          t("inscribe.feeWarningRate", { rate: effectiveFeeRate }),
        fee > HIGH_ABSOLUTE_FEE_SATS && t("inscribe.feeWarningAbsolute", { fee: formatSats(fee) }),
      ].filter((w): w is string => Boolean(w))
    : [];

  return (
    <div className="space-y-4">
      {!filePath ? (
        <div
          className={`flex flex-col items-center justify-center gap-2 rounded-md border-2 border-dashed p-10 text-center text-sm text-muted-foreground ${
            dragOver ? "border-primary bg-primary/5" : "border-border"
          }`}
          // Real drag-and-drop only works in the actual Tauri webview
          // (no equivalent event source in the browser dev preview) --
          // outside it, clicking the zone loads a placeholder path so
          // the rest of this screen can still be exercised there,
          // mirroring how every other screen's dev-mock IPC stands in
          // for the real backend.
          onClick={() => {
            if (!isRealTauriRuntime()) loadFile("/dev-preview/example.png");
          }}
        >
          <p>{t("inscribe.dropZone")}</p>
          {!isRealTauriRuntime() && <p className="text-xs italic">{t("inscribe.dropZoneHint")}</p>}
        </div>
      ) : (
        <div className="space-y-3 rounded-md border border-border p-4">
          {fileError &&
            (() => {
              const friendly = friendlyError(fileError);
              return (
                <ErrorPanel
                  title={friendly.title}
                  message={friendly.whatToDo ? `${friendly.message} ${friendly.whatToDo}` : friendly.message}
                  technicalDetails={fileError.message}
                />
              );
            })()}

          {filePreview && (
            <>
              <div className="overflow-hidden rounded-md border border-border bg-background">
                <iframe
                  src={filePreview.data_url ?? undefined}
                  sandbox="allow-scripts"
                  referrerPolicy="no-referrer"
                  title={filePath}
                  className="h-48 w-full border-0 bg-background"
                />
              </div>
              <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1 text-sm">
                <dt className="text-muted-foreground">{t("inscribe.chosenFile")}</dt>
                <dd className="truncate font-mono text-xs">{filePath}</dd>
                <dt className="text-muted-foreground">{t("inscribe.fileSize")}</dt>
                <dd>{formatBytes(filePreview.size_bytes)}</dd>
                <dt className="text-muted-foreground">{t("inscribe.fileType")}</dt>
                <dd>{filePreview.content_type}</dd>
              </dl>
              {filePreview.size_bytes > LARGE_FILE_WARNING_BYTES && (
                <p className="text-warning text-xs">
                  {formatBytes(filePreview.size_bytes)} is large -- inscribing it will cost more.
                </p>
              )}
            </>
          )}

          <Button variant="outline" size="sm" onClick={reset}>
            {t("inscribe.removeFile")}
          </Button>
        </div>
      )}

      {filePath && (
        <>
          <label className="block text-sm">
            {t("inscribe.feeRate")}
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
                    ? t("inscribe.feeRateManualRequired")
                    : String(REGTEST_FALLBACK_FEE_RATE)
              }
              inputMode="decimal"
            />
          </label>
          {estimatedFeeRate === null && (
            <p className="text-xs text-muted-foreground">
              {chain === "mainnet"
                ? t("inscribe.noEstimateMainnet")
                : t("inscribe.noEstimateRegtest", { rate: REGTEST_FALLBACK_FEE_RATE })}
            </p>
          )}

          <div>
            <button
              type="button"
              className="text-xs text-muted-foreground underline"
              onClick={() => setShowAdvanced((v) => !v)}
            >
              {t("inscribe.advancedOptions")}
            </button>
            {showAdvanced && (
              <div className="mt-2 space-y-3">
                <label className="block text-sm">
                  {t("inscribe.postage")}
                  <Input
                    className="mt-1"
                    value={postage}
                    onChange={(e) => {
                      setPostage(e.target.value);
                      setPreview(null);
                      setPreviewError(null);
                    }}
                    placeholder={t("inscribe.postagePlaceholder")}
                    inputMode="numeric"
                  />
                </label>
                <label className="block text-sm">
                  {t("inscribe.parent")}
                  <Input
                    className="mt-1 font-mono"
                    value={parent}
                    onChange={(e) => {
                      setParent(e.target.value);
                      setPreview(null);
                      setPreviewError(null);
                    }}
                    placeholder={t("inscribe.parentPlaceholder")}
                    spellCheck={false}
                  />
                </label>
              </div>
            )}
          </div>

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

          {inscribeError &&
            (() => {
              const friendly = friendlyError(inscribeError);
              return (
                <ErrorPanel
                  title={friendly.title}
                  message={friendly.whatToDo ? `${friendly.message} ${friendly.whatToDo}` : friendly.message}
                  technicalDetails={inscribeError.message}
                />
              );
            })()}

          {preview && (
            <div className="space-y-2 rounded-md border border-border bg-card p-3 text-sm">
              <p className="font-medium">{t("inscribe.previewTitle")}</p>
              <dl className="grid grid-cols-2 gap-y-1">
                <dt className="text-muted-foreground">{t("inscribe.previewFee")}</dt>
                <dd>{formatSats(preview.fee)}</dd>
                <dt className="text-muted-foreground">{t("inscribe.previewLocation")}</dt>
                <dd className="truncate font-mono text-xs">{preview.location}</dd>
              </dl>
              {feeWarnings.map((warning) => (
                <p key={warning} className="text-warning">
                  {warning}
                </p>
              ))}
            </div>
          )}

          <div className="flex gap-2">
            {preview ? (
              <Button className="flex-1" onClick={openConfirm} disabled={inscribing}>
                {t("inscribe.submit")}
              </Button>
            ) : (
              <Button
                className="flex-1"
                onClick={runPreview}
                disabled={previewing || effectiveFeeRate === null}
              >
                {t("inscribe.preview")}
              </Button>
            )}
          </div>

          <ConfirmDialog
            open={confirmOpen}
            onOpenChange={setConfirmOpen}
            environment={environment}
            title={t("inscribe.confirmTitle")}
            description={t("inscribe.confirmDescription", { fee: formatSats(fee) })}
            onConfirm={() => attemptInscribe(needsPassphrase ? passphraseValue : null)}
            confirmLabel={t("inscribe.submit")}
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
        </>
      )}
    </div>
  );
}
