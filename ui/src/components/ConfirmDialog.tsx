import { useState } from "react";
import { useTranslation } from "react-i18next";
import type { Environment } from "@/bindings/Environment";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { chainTextClass } from "@/lib/environment-colors";

export interface ConfirmDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** Always shown, in large text, per docs/SPEC.md item 10. */
  environment: Environment;
  title: string;
  description: string;
  /**
   * The exact command this action will run. "Learn mode" (docs/SPEC.md
   * item 7: "confirmation dialogs also show the exact command that will
   * run") shows it inline when set.
   */
  command?: string;
  learnMode?: boolean;
  onConfirm: () => void;
  confirmLabel?: string;
}

/**
 * The single, shared confirmation dialog. Per docs/SPEC.md item 7: "No
 * screen may implement its own confirmation flow, so the mainnet step
 * cannot be skipped" — every fund-moving or state-changing action must go
 * through this component, not a bespoke one, specifically so the mainnet
 * extra step below can't be accidentally bypassed by a screen that forgot
 * to add it.
 */
export function ConfirmDialog({
  open,
  onOpenChange,
  environment,
  title,
  description,
  command,
  learnMode,
  onConfirm,
  confirmLabel,
}: ConfirmDialogProps) {
  const { t } = useTranslation();
  const isMainnet = environment.chain === "mainnet";
  // Mainnet extra step (docs/SPEC.md item 3/4/10): an explicit
  // acknowledgement the user must check before Confirm is even enabled.
  const [mainnetAck, setMainnetAck] = useState(false);

  const handleOpenChange = (next: boolean) => {
    if (!next) setMainnetAck(false);
    onOpenChange(next);
  };

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogContent>
        <DialogHeader>
          <p className={`text-lg font-bold tracking-wide ${chainTextClass(environment.chain)}`}>
            {environment.name.toUpperCase()}
          </p>
          <DialogTitle>{title}</DialogTitle>
          <DialogDescription>{description}</DialogDescription>
        </DialogHeader>

        {isMainnet && (
          <div className="rounded-md border border-env-mainnet/40 bg-env-mainnet/10 p-3 text-sm">
            <p className={`font-semibold ${chainTextClass(environment.chain)}`}>
              {t("confirmDialog.mainnetWarningTitle", { environment: environment.name })}
            </p>
            <label className="mt-2 flex items-start gap-2">
              <Checkbox
                checked={mainnetAck}
                onCheckedChange={(checked) => setMainnetAck(checked === true)}
              />
              <span>{t("confirmDialog.mainnetAcknowledge")}</span>
            </label>
          </div>
        )}

        {learnMode && command && (
          <div>
            <p className="text-xs font-medium text-muted-foreground">{t("confirmDialog.commandLabel")}</p>
            <pre className="mt-1 overflow-x-auto rounded bg-muted p-2 font-mono text-xs">{command}</pre>
          </div>
        )}

        <DialogFooter>
          <Button variant="outline" onClick={() => handleOpenChange(false)}>
            {t("confirmDialog.cancel")}
          </Button>
          <Button
            variant={isMainnet ? "destructive" : "default"}
            disabled={isMainnet && !mainnetAck}
            onClick={onConfirm}
          >
            {confirmLabel ?? t("confirmDialog.confirm")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
