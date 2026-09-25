import { useState } from "react";
import { useTranslation } from "react-i18next";
import type { Environment } from "@/bindings/Environment";
import { Button } from "@/components/ui/button";
import { BatchInscribeForm } from "@/components/BatchInscribeForm";
import { ReinscribeForm } from "@/components/ReinscribeForm";
import { SingleInscribeForm } from "@/components/SingleInscribeForm";

type Mode = "single" | "batch" | "reinscribe";

/**
 * docs/SPEC.md item 4's Inscribe studio. A thin container: the mode
 * toggle plus whichever form is active -- `SingleInscribeForm`,
 * `BatchInscribeForm`, and `ReinscribeForm` each own all the actual
 * create/preview/confirm logic for their mode.
 */
export function InscribeStudioScreen({ environment }: { environment: Environment }) {
  const { t } = useTranslation();
  const [mode, setMode] = useState<Mode>("single");

  return (
    <div className="space-y-4 p-4">
      <div className="flex items-center justify-between">
        <h2 className="text-lg font-semibold">{t("inscribe.title")}</h2>
        <div className="flex gap-1">
          <Button
            variant={mode === "single" ? "secondary" : "ghost"}
            size="sm"
            onClick={() => setMode("single")}
          >
            {t("inscribe.modeSingle")}
          </Button>
          <Button variant={mode === "batch" ? "secondary" : "ghost"} size="sm" onClick={() => setMode("batch")}>
            {t("inscribe.modeBatch")}
          </Button>
          <Button
            variant={mode === "reinscribe" ? "secondary" : "ghost"}
            size="sm"
            onClick={() => setMode("reinscribe")}
          >
            {t("inscribe.modeReinscribe")}
          </Button>
        </div>
      </div>

      {mode === "single" ? (
        <SingleInscribeForm key={environment.chain} environment={environment} />
      ) : mode === "batch" ? (
        <BatchInscribeForm key={environment.chain} environment={environment} />
      ) : (
        <ReinscribeForm key={environment.chain} environment={environment} />
      )}
    </div>
  );
}
