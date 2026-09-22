import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";

/**
 * The shared error display: a friendly message plus an optional "what to
 * do" action, with raw/technical detail behind a toggle (docs/SPEC.md
 * item 8: "Plain-language errors... Technical details are available
 * behind a toggle"). The actual failure-code -> friendly-message mapping
 * lives with the features that produce those errors (later phases) — this
 * is just the shared presentation shell every one of them renders into.
 */
export function ErrorPanel({
  title,
  message,
  whatToDo,
  technicalDetails,
}: {
  title: string;
  message: string;
  whatToDo?: { label: string; onClick: () => void };
  technicalDetails?: string;
}) {
  const { t } = useTranslation();
  const [showDetails, setShowDetails] = useState(false);

  return (
    <div role="alert" className="rounded-lg border border-danger/30 bg-danger/5 p-4">
      <h3 className="font-semibold text-danger">{title}</h3>
      <p className="mt-1 text-sm text-foreground">{message}</p>
      <div className="mt-3 flex items-center gap-3">
        {whatToDo && (
          <Button size="sm" onClick={whatToDo.onClick}>
            {whatToDo.label}
          </Button>
        )}
        {technicalDetails && (
          <button
            type="button"
            className="text-xs text-muted-foreground underline"
            onClick={() => setShowDetails((v) => !v)}
          >
            {showDetails ? t("errorPanel.hideTechnicalDetails") : t("errorPanel.showTechnicalDetails")}
          </button>
        )}
      </div>
      {showDetails && technicalDetails && (
        <pre className="mt-2 overflow-x-auto rounded bg-muted p-2 text-xs text-muted-foreground">
          {technicalDetails}
        </pre>
      )}
    </div>
  );
}
