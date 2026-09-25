import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import type { Chain } from "@/bindings/Chain";
import type { Environment } from "@/bindings/Environment";
import type { FilePreview } from "@/bindings/FilePreview";
import type { InscribeResult } from "@/bindings/InscribeResult";
import type { InscriptionDetail } from "@/bindings/InscriptionDetail";
import type { TypedError } from "@/bindings/TypedError";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Input } from "@/components/ui/input";
import { ConfirmDialog } from "@/components/ConfirmDialog";
import { ErrorPanel } from "@/components/ErrorPanel";
import { InscriptionPreviewTile } from "@/components/InscriptionPreviewTile";
import { isRealTauriRuntime, useDragDropFiles } from "@/hooks/useDragDropFiles";
import { useWalletInscriptions } from "@/hooks/useWalletInscriptions";
import { friendlyError } from "@/lib/error-messages";
import { formatBytes, formatSats } from "@/lib/format";

const REGTEST_FALLBACK_FEE_RATE = 1;
const HIGH_FEE_RATE_SAT_VB = 200;
const HIGH_ABSOLUTE_FEE_SATS = 50_000;
const LARGE_FILE_WARNING_BYTES = 400_000;

type Step = "pick" | "compose" | "review";

/**
 * docs/SPEC.md item 4's REINSCRIBE MODE. A 3-step flow: pick an owned
 * inscription (fills in its satpoint) -> compose the new content +
 * fee, with the sat's existing inscription history shown for context
 * (or the Foundation F explanation if `--index-sats` is off) -> a
 * dedicated review screen with a mandatory "I understand this sat
 * already has inscriptions" checkbox before the shared `ConfirmDialog`
 * does the actual mainnet-ack/passphrase/broadcast step. The review
 * screen is deliberately *not* part of `ConfirmDialog` itself: the
 * spec's checkbox is a reinscribe-specific review gate, not the
 * general "confirm this action" step every fund-moving flow already
 * goes through, so it's a screen of its own that leads *into*
 * `ConfirmDialog` rather than replacing it (per "no screen implements
 * its own confirmation flow" -- the final broadcast confirmation still
 * always goes through the one shared component).
 *
 * The permanence/visibility explainer (spec: "before the first
 * reinscription, explain...") is shown every time in the compose step,
 * not tracked as a one-time "seen it" flag -- simpler, and safer for an
 * irreversible action than trying to guess whether "first" means "ever
 * in this app" or something narrower.
 */
export function ReinscribeForm({ environment }: { environment: Environment }) {
  const { t } = useTranslation();
  const chain: Chain = environment.chain;

  const [step, setStep] = useState<Step>("pick");
  const { inscriptions: owned, error: ownedError, loading: ownedLoading } = useWalletInscriptions(chain);

  const [detail, setDetail] = useState<InscriptionDetail | null>(null);
  const [detailError, setDetailError] = useState<TypedError | null>(null);

  const [satHistory, setSatHistory] = useState<InscriptionDetail[] | null>(null);
  const [satHistoryError, setSatHistoryError] = useState<TypedError | null>(null);

  const [filePath, setFilePath] = useState<string | null>(null);
  const [filePreview, setFilePreview] = useState<FilePreview | null>(null);
  const [fileError, setFileError] = useState<TypedError | null>(null);

  const [estimatedFeeRate, setEstimatedFeeRate] = useState<number | null>(null);
  const [feeRate, setFeeRate] = useState("");

  const [preview, setPreview] = useState<InscribeResult | null>(null);
  const [previewError, setPreviewError] = useState<TypedError | null>(null);
  const [previewing, setPreviewing] = useState(false);

  const [understandsReinscribe, setUnderstandsReinscribe] = useState(false);

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
    setPreview(null);
    setPreviewError(null);
    setFilePath(path);
    invoke<FilePreview>("inscribe_file_preview", { path })
      .then(setFilePreview)
      .catch((e: TypedError) => setFileError(e));
  };

  const { dragOver } = useDragDropFiles((paths) => loadFile(paths[0]));

  const selectInscription = (id: string) => {
    setDetail(null);
    setDetailError(null);
    setSatHistory(null);
    setSatHistoryError(null);
    invoke<InscriptionDetail>("inscription_detail", { chain, id })
      .then(setDetail)
      .catch((e: TypedError) => setDetailError(e));
    setStep("compose");
  };

  useEffect(() => {
    if (!detail || detail.sat === null) return;
    invoke<string[]>("sat_inscriptions", { chain, sat: detail.sat })
      .then((ids) =>
        Promise.all(ids.map((id) => invoke<InscriptionDetail>("inscription_detail", { chain, id }))),
      )
      .then(setSatHistory)
      .catch((e: TypedError) => setSatHistoryError(e));
  }, [chain, detail]);

  useEffect(() => {
    invoke<number | null>("wallet_fee_estimate", { chain, confTarget: 6 }).then(
      setEstimatedFeeRate,
      () => setEstimatedFeeRate(null),
    );
  }, [chain]);

  const effectiveFeeRate =
    feeRate !== "" ? Number(feeRate) : (estimatedFeeRate ?? (chain === "mainnet" ? null : REGTEST_FALLBACK_FEE_RATE));

  const runPreview = () => {
    if (effectiveFeeRate === null || !filePath || !detail) return;
    setPreviewing(true);
    setPreviewError(null);
    setPreview(null);
    invoke<InscribeResult>("wallet_inscribe_dry_run", {
      chain,
      filePath,
      feeRate: effectiveFeeRate,
      postage: undefined,
      parent: undefined,
      reinscribeSatpoint: detail.satpoint,
    })
      .then((result) => {
        setPreview(result);
        setStep("review");
      })
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
    if (effectiveFeeRate === null || !filePath || !detail) return;
    setInscribing(true);
    setPassphraseError(undefined);
    setInscribeError(null);
    invoke<InscribeResult>("wallet_inscribe", {
      chain,
      filePath,
      feeRate: effectiveFeeRate,
      postage: undefined,
      parent: undefined,
      reinscribeSatpoint: detail.satpoint,
      passphrase: passphraseToUse,
      remember: rememberPassphrase,
    })
      .then((result) => {
        setConfirmOpen(false);
        setInscribed(result);
      })
      .catch((e: TypedError) => {
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
    setStep("pick");
    setDetail(null);
    setDetailError(null);
    setSatHistory(null);
    setSatHistoryError(null);
    setFilePath(null);
    setFilePreview(null);
    setFileError(null);
    setFeeRate("");
    setPreview(null);
    setPreviewError(null);
    setUnderstandsReinscribe(false);
    setInscribeError(null);
    setInscribed(null);
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
        <Button onClick={reset}>{t("inscribe.inscribeAnother")}</Button>
      </div>
    );
  }

  if (step === "pick") {
    return (
      <div className="space-y-2">
        <p className="text-sm text-muted-foreground">{t("inscribe.reinscribe.pickInstructions")}</p>
        {ownedError &&
          (() => {
            const friendly = friendlyError(ownedError);
            return (
              <ErrorPanel
                title={friendly.title}
                message={friendly.whatToDo ? `${friendly.message} ${friendly.whatToDo}` : friendly.message}
                technicalDetails={ownedError.message}
              />
            );
          })()}
        {ownedLoading && <p className="text-sm text-muted-foreground">{t("wallet.gallery.loading")}</p>}
        {!ownedLoading && !ownedError && owned && owned.length === 0 && (
          <p className="text-sm text-muted-foreground">{t("wallet.gallery.none")}</p>
        )}
        {!ownedLoading && !ownedError && owned && owned.length > 0 && (
          <div className="grid grid-cols-2 gap-3 sm:grid-cols-3 md:grid-cols-4">
            {owned.map((inscription) => (
              <button
                key={inscription.id}
                type="button"
                className="overflow-hidden rounded-md border border-border bg-card text-left hover:border-primary"
                onClick={() => selectInscription(inscription.id)}
              >
                <InscriptionPreviewTile
                  environment={environment}
                  id={inscription.id}
                  label={t("wallet.gallery.itemLabel", { id: inscription.id })}
                />
                <code className="block truncate px-2 py-1 text-xs text-muted-foreground">
                  {inscription.id}
                </code>
              </button>
            ))}
          </div>
        )}
      </div>
    );
  }

  if (step === "review" && preview && detail) {
    const resultingCount = (satHistory?.length ?? 0) + 1;
    return (
      <div className="space-y-4">
        <div className="space-y-3 rounded-md border border-border bg-card p-4 text-sm">
          <p className="font-medium">{t("inscribe.reinscribe.reviewTitle")}</p>
          <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1">
            <dt className="text-muted-foreground">{t("inscribe.reinscribe.targetSat")}</dt>
            <dd className="font-mono text-xs">{detail.sat}</dd>
            <dt className="text-muted-foreground">{t("inscribe.reinscribe.existingCount")}</dt>
            <dd>{satHistory?.length ?? 0}</dd>
            <dt className="text-muted-foreground">{t("inscribe.reinscribe.resultingCount")}</dt>
            <dd>{resultingCount}</dd>
            <dt className="text-muted-foreground">{t("inscribe.previewFee")}</dt>
            <dd>{formatSats(preview.fee)}</dd>
          </dl>

          {satHistory && satHistory.length > 0 && (
            <div className="grid grid-cols-3 gap-2 sm:grid-cols-4">
              {satHistory.map((entry) => (
                <div key={entry.id} className="overflow-hidden rounded-md border border-border">
                  <InscriptionPreviewTile
                    environment={environment}
                    id={entry.id}
                    label={t("inscribe.reinscribe.numberLabel", { number: entry.number })}
                    className="h-20 w-full border-0 bg-background"
                  />
                  <p className="truncate px-1 py-0.5 text-center text-xs text-muted-foreground">
                    {t("inscribe.reinscribe.numberLabel", { number: entry.number })}
                  </p>
                </div>
              ))}
            </div>
          )}

          {filePreview && (
            <div className="overflow-hidden rounded-md border border-border bg-background">
              <iframe
                src={filePreview.data_url ?? undefined}
                sandbox="allow-scripts"
                referrerPolicy="no-referrer"
                title={t("inscribe.reinscribe.newContentLabel")}
                className="h-32 w-full border-0 bg-background"
              />
            </div>
          )}

          {[
            effectiveFeeRate !== null &&
              effectiveFeeRate > HIGH_FEE_RATE_SAT_VB &&
              t("inscribe.feeWarningRate", { rate: effectiveFeeRate }),
            preview.fee > HIGH_ABSOLUTE_FEE_SATS && t("inscribe.feeWarningAbsolute", { fee: formatSats(preview.fee) }),
          ]
            .filter((w): w is string => Boolean(w))
            .map((warning) => (
              <p key={warning} className="text-warning">
                {warning}
              </p>
            ))}
        </div>

        <label className="flex items-start gap-2 text-sm">
          <Checkbox
            checked={understandsReinscribe}
            onCheckedChange={(c) => setUnderstandsReinscribe(c === true)}
          />
          {t("inscribe.reinscribe.mandatoryCheckbox")}
        </label>

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

        <div className="flex gap-2">
          <Button variant="outline" className="flex-1" onClick={() => setStep("compose")}>
            {t("inscribe.cancel")}
          </Button>
          <Button className="flex-1" onClick={openConfirm} disabled={!understandsReinscribe || inscribing}>
            {t("inscribe.reinscribe.submit")}
          </Button>
        </div>

        <ConfirmDialog
          open={confirmOpen}
          onOpenChange={setConfirmOpen}
          environment={environment}
          title={t("inscribe.reinscribe.confirmTitle")}
          description={t("inscribe.reinscribe.confirmDescription", {
            number: resultingCount,
            sat: detail.sat,
            fee: formatSats(preview.fee),
          })}
          onConfirm={() => attemptInscribe(needsPassphrase ? passphraseValue : null)}
          confirmLabel={t("inscribe.reinscribe.submit")}
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

  // step === "compose"
  return (
    <div className="space-y-4">
      <Button variant="outline" size="sm" onClick={reset}>
        {t("inscribe.reinscribe.pickDifferent")}
      </Button>

      {detailError &&
        (() => {
          const friendly = friendlyError(detailError);
          return (
            <ErrorPanel
              title={friendly.title}
              message={friendly.whatToDo ? `${friendly.message} ${friendly.whatToDo}` : friendly.message}
              technicalDetails={detailError.message}
            />
          );
        })()}

      {detail && (
        <div className="space-y-2 rounded-md border border-border bg-card p-3 text-sm">
          <p className="text-warning font-medium">{t("inscribe.reinscribe.permanenceExplainer")}</p>

          {detail.sat === null ? (
            <p className="text-xs text-muted-foreground">{t("inscribe.reinscribe.needsSatIndex")}</p>
          ) : satHistoryError ? (
            (() => {
              const friendly = friendlyError(satHistoryError);
              return (
                <ErrorPanel
                  title={friendly.title}
                  message={friendly.whatToDo ? `${friendly.message} ${friendly.whatToDo}` : friendly.message}
                  technicalDetails={satHistoryError.message}
                />
              );
            })()
          ) : satHistory === null ? (
            <p className="text-xs text-muted-foreground">{t("wallet.gallery.loading")}</p>
          ) : (
            <div className="space-y-1">
              <p className="text-xs text-muted-foreground">
                {t("inscribe.reinscribe.existingOnSat", { count: satHistory.length, sat: detail.sat })}
              </p>
              <div className="grid grid-cols-3 gap-2 sm:grid-cols-4">
                {satHistory.map((entry) => (
                  <div key={entry.id} className="overflow-hidden rounded-md border border-border">
                    <InscriptionPreviewTile
                      environment={environment}
                      id={entry.id}
                      label={t("inscribe.reinscribe.numberLabel", { number: entry.number })}
                      className="h-20 w-full border-0 bg-background"
                    />
                    <p className="truncate px-1 py-0.5 text-center text-xs text-muted-foreground">
                      {t("inscribe.reinscribe.numberLabel", { number: entry.number })}
                    </p>
                  </div>
                ))}
              </div>
            </div>
          )}
        </div>
      )}

      {!filePath ? (
        <div
          className={`flex flex-col items-center justify-center gap-2 rounded-md border-2 border-dashed p-10 text-center text-sm text-muted-foreground ${
            dragOver ? "border-primary bg-primary/5" : "border-border"
          }`}
          onClick={() => {
            if (!isRealTauriRuntime()) loadFile("/dev-preview/reinscribe.png");
          }}
        >
          <p>{t("inscribe.reinscribe.newContentDropZone")}</p>
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
          <Button
            variant="outline"
            size="sm"
            onClick={() => {
              setFilePath(null);
              setFilePreview(null);
              setFileError(null);
            }}
          >
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

          <Button
            className="w-full"
            onClick={runPreview}
            disabled={previewing || effectiveFeeRate === null || !detail}
          >
            {t("inscribe.preview")}
          </Button>
        </>
      )}
    </div>
  );
}
