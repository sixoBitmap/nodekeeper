import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import type { Chain } from "@/bindings/Chain";
import type { Environment } from "@/bindings/Environment";
import type { InscribeResult } from "@/bindings/InscribeResult";
import type { TypedError } from "@/bindings/TypedError";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { ConfirmDialog } from "@/components/ConfirmDialog";
import { ErrorPanel } from "@/components/ErrorPanel";
import { RegtestMineOffer } from "@/components/RegtestMineOffer";
import { isRealTauriRuntime, useDragDropFiles } from "@/hooks/useDragDropFiles";
import { friendlyError } from "@/lib/error-messages";
import { formatSats } from "@/lib/format";

const REGTEST_FALLBACK_FEE_RATE = 1;
const HIGH_FEE_RATE_SAT_VB = 200;
const HIGH_ABSOLUTE_FEE_SATS = 50_000;

/**
 * docs/SPEC.md item 4's "Visual batch-YAML builder with export and
 * edit": "visual" and "edit" are read as building the batch by adding/
 * removing files in this list, not by hand-editing raw YAML text that
 * then gets shelled out to ord -- the actual command is always built
 * server-side from this typed file list (`wallet_inscribe_batch`,
 * `nk_ord::wallet::batch_inscribe`), the same reasoning that ruled out
 * a free-text YAML box when that backend was built (DECISIONS.md Phase
 * 6: "avoids a path-injection-shaped surface"). The YAML shown/exported
 * here is a client-side mirror of that same data, for transparency
 * (same spirit as the Live Command Monitor showing the equivalent CLI
 * command) -- not itself fed to ord.
 */
function buildBatchYaml(filePaths: string[]): string {
  const lines = ["mode: separate-outputs", "inscriptions:"];
  for (const path of filePaths) {
    // Matches `nk_ord::wallet::batch_inscribe`'s own `serde_yaml`
    // output shape exactly (DECISIONS.md Phase 6 VERIFY) -- a plain,
    // unquoted `file:` entry per file.
    lines.push(`  - file: ${path}`);
  }
  return lines.join("\n") + "\n";
}

function exportYaml(yaml: string) {
  const blob = new Blob([yaml], { type: "application/x-yaml" });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = `nodekeeper-batch-${Date.now()}.yaml`;
  a.click();
  URL.revokeObjectURL(url);
}

/** docs/SPEC.md item 4's Inscribe studio, batch mode. Same fee-guard/
 * dry-run/`ConfirmDialog` shape as `SingleInscribeForm`, but for
 * several files inscribed together in one commit/reveal transaction
 * pair (DECISIONS.md Phase 6 VERIFY). No per-entry reinscribe, parent,
 * or postage -- ord 0.29.0's batch YAML schema doesn't support a
 * `reinscribe` field at all (VERIFIED live, hidden here rather than
 * offered and failing), and `nk_ord::wallet::BatchInscriptionEntry` is
 * deliberately scoped to just a file path for now. */
export function BatchInscribeForm({ environment }: { environment: Environment }) {
  const { t } = useTranslation();
  const chain: Chain = environment.chain;

  const [files, setFiles] = useState<string[]>([]);
  const [showYaml, setShowYaml] = useState(false);

  const [estimatedFeeRate, setEstimatedFeeRate] = useState<number | null>(null);
  const [feeRate, setFeeRate] = useState("");

  const [preview, setPreview] = useState<InscribeResult[] | null>(null);
  const [previewError, setPreviewError] = useState<TypedError | null>(null);
  const [previewing, setPreviewing] = useState(false);

  const [confirmOpen, setConfirmOpen] = useState(false);
  const [needsPassphrase, setNeedsPassphrase] = useState(false);
  const [passphraseValue, setPassphraseValue] = useState("");
  const [rememberPassphrase, setRememberPassphrase] = useState(false);
  const [passphraseError, setPassphraseError] = useState<string | undefined>(undefined);
  const [inscribeError, setInscribeError] = useState<TypedError | null>(null);
  const [inscribing, setInscribing] = useState(false);
  const [inscribed, setInscribed] = useState<InscribeResult[] | null>(null);

  const addFiles = (paths: string[]) => {
    setInscribed(null);
    setPreview(null);
    setPreviewError(null);
    // De-duplicated -- dropping the same file twice shouldn't silently
    // double-inscribe it.
    setFiles((current) => [...current, ...paths.filter((p) => !current.includes(p))]);
  };

  const removeFile = (index: number) => {
    setFiles((current) => current.filter((_, i) => i !== index));
    setPreview(null);
    setPreviewError(null);
  };

  const { dragOver } = useDragDropFiles(addFiles);

  useEffect(() => {
    invoke<number | null>("wallet_fee_estimate", { chain, confTarget: 6 }).then(
      setEstimatedFeeRate,
      () => setEstimatedFeeRate(null),
    );
  }, [chain]);

  const effectiveFeeRate =
    feeRate !== "" ? Number(feeRate) : (estimatedFeeRate ?? (chain === "mainnet" ? null : REGTEST_FALLBACK_FEE_RATE));

  const runPreview = () => {
    if (effectiveFeeRate === null || files.length === 0) return;
    setPreviewing(true);
    setPreviewError(null);
    setPreview(null);
    invoke<InscribeResult[]>("wallet_inscribe_batch_dry_run", {
      chain,
      filePaths: files,
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
    setInscribeError(null);
    setConfirmOpen(true);
  };

  const attemptInscribe = (passphraseToUse: string | null) => {
    if (effectiveFeeRate === null || files.length === 0) return;
    setInscribing(true);
    setPassphraseError(undefined);
    setInscribeError(null);
    invoke<InscribeResult[]>("wallet_inscribe_batch", {
      chain,
      filePaths: files,
      feeRate: effectiveFeeRate,
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
    setFiles([]);
    setFeeRate("");
    setPreview(null);
    setPreviewError(null);
    setInscribed(null);
    setInscribeError(null);
  };

  if (inscribed) {
    return (
      <div className="space-y-4">
        <div className="space-y-2 rounded-md border border-border bg-card p-4 text-sm">
          <p className="font-medium text-success">{t("inscribe.batchDone", { count: inscribed.length })}</p>
          <ul className="space-y-1">
            {inscribed.map((entry) => (
              <li key={entry.id} className="flex justify-between gap-4">
                <code className="truncate text-xs">{entry.id}</code>
                <span>{formatSats(entry.fee)}</span>
              </li>
            ))}
          </ul>
        </div>
        <RegtestMineOffer chain={chain} />
        <Button onClick={reset}>{t("inscribe.inscribeAnother")}</Button>
      </div>
    );
  }

  const totalFee = preview?.[0]?.fee ?? 0;
  const feeWarnings = preview
    ? [
        effectiveFeeRate !== null &&
          effectiveFeeRate > HIGH_FEE_RATE_SAT_VB &&
          t("inscribe.feeWarningRate", { rate: effectiveFeeRate }),
        totalFee > HIGH_ABSOLUTE_FEE_SATS && t("inscribe.feeWarningAbsolute", { fee: formatSats(totalFee) }),
      ].filter((w): w is string => Boolean(w))
    : [];
  const yaml = buildBatchYaml(files);

  return (
    <div className="space-y-4">
      <div
        className={`flex flex-col items-center justify-center gap-2 rounded-md border-2 border-dashed p-6 text-center text-sm text-muted-foreground ${
          dragOver ? "border-primary bg-primary/5" : "border-border"
        }`}
        onClick={() => {
          if (!isRealTauriRuntime()) addFiles([`/dev-preview/batch-${files.length + 1}.png`]);
        }}
      >
        <p>{t("inscribe.batchDropZone")}</p>
        {!isRealTauriRuntime() && <p className="text-xs italic">{t("inscribe.dropZoneHint")}</p>}
      </div>

      {files.length > 0 && (
        <ul className="space-y-1 rounded-md border border-border p-2 text-sm">
          {files.map((path, index) => (
            <li key={path} className="flex items-center justify-between gap-2">
              <code className="truncate text-xs">{path}</code>
              <Button variant="outline" size="xs" onClick={() => removeFile(index)}>
                {t("inscribe.removeFile")}
              </Button>
            </li>
          ))}
        </ul>
      )}

      {files.length > 0 && (
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
              onClick={() => setShowYaml((v) => !v)}
            >
              {t("inscribe.viewYaml")}
            </button>
            {showYaml && (
              <div className="mt-2 space-y-2">
                <pre className="overflow-x-auto rounded-md border border-border bg-card p-3 text-xs">{yaml}</pre>
                <Button variant="outline" size="sm" onClick={() => exportYaml(yaml)}>
                  {t("inscribe.exportYaml")}
                </Button>
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
                <dt className="text-muted-foreground">{t("inscribe.batchCount")}</dt>
                <dd>{preview.length}</dd>
                <dt className="text-muted-foreground">{t("inscribe.previewFee")}</dt>
                <dd>{formatSats(totalFee)}</dd>
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
            title={t("inscribe.batchConfirmTitle")}
            description={t("inscribe.batchConfirmDescription", { count: files.length, fee: formatSats(totalFee) })}
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
