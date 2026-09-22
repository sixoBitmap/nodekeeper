import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";

/**
 * docs/SPEC.md item 0: shown once, before setup, in plain language. The
 * caller decides "shown once" via the `disclaimer_acknowledged` setting
 * (see App.tsx) — this component only renders the content and reports
 * the click.
 */
export function FirstRunDisclaimer({ onAcknowledge }: { onAcknowledge: () => void }) {
  const { t } = useTranslation();

  return (
    <div className="flex h-full flex-col items-center justify-center gap-6 bg-background p-8 text-foreground">
      <div className="w-full max-w-lg space-y-4">
        <h1 className="text-2xl font-semibold">{t("disclaimer.title")}</h1>
        <p className="text-sm text-muted-foreground">{t("disclaimer.intro")}</p>
        <ul className="list-disc space-y-2 pl-5 text-sm">
          <li>{t("disclaimer.selfCustody")}</li>
          <li>{t("disclaimer.noRecovery")}</li>
          <li>{t("disclaimer.practiceFirst")}</li>
          <li className="font-medium text-env-mainnet">{t("disclaimer.mainnetIsReal")}</li>
        </ul>
        <Button className="w-full" onClick={onAcknowledge}>
          {t("disclaimer.acknowledge")}
        </Button>
      </div>
    </div>
  );
}
