import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";

export interface SensitiveSeedViewProps {
  /** The mnemonic words. Never logged, stored, or sent anywhere by this
   * component — it only ever renders what it's given and reports back
   * whether the user typed the right words at the confirm step. The
   * caller is responsible for zeroizing/discarding this once `onDone`
   * fires (docs/SPEC.md Foundation B: the sensitive-output channel). */
  words: string[];
  onDone: () => void;
}

/**
 * The one and only place a mnemonic is ever displayed (docs/SPEC.md item
 * 3): full-screen, a screenshot warning, then a check that the user
 * actually wrote it down before continuing. This is Phase 1's skeleton —
 * no wallet create/restore command calls into it yet (Phase 5); it exists
 * now so every later screen that needs to show a seed uses this one
 * component rather than rolling its own.
 */
export function SensitiveSeedView({ words, onDone }: SensitiveSeedViewProps) {
  const { t } = useTranslation();
  const [step, setStep] = useState<"view" | "confirm">("view");

  // Fixed per mount (not re-randomized on every render) so the indices
  // being asked about don't shift under the user while they're typing.
  const quizIndices = useMemo(() => pickQuizIndices(words.length), [words.length]);
  const [answers, setAnswers] = useState<Record<number, string>>({});

  const allCorrect = quizIndices.every(
    (i) => (answers[i] ?? "").trim().toLowerCase() === words[i].toLowerCase(),
  );

  return (
    <div className="fixed inset-0 z-50 flex flex-col items-center justify-center gap-6 overflow-y-auto bg-background p-8">
      <div className="w-full max-w-lg rounded-md border border-danger/40 bg-danger/5 p-3 text-center text-sm font-medium text-danger">
        {t("sensitiveSeed.screenshotWarning")}
      </div>

      {step === "view" ? (
        <>
          <h1 className="text-xl font-semibold">{t("sensitiveSeed.title")}</h1>
          <ol className="grid w-full max-w-lg grid-cols-3 gap-2">
            {words.map((word, i) => (
              <li
                key={i}
                className="flex items-center gap-2 rounded border border-border bg-card px-2 py-1.5 text-sm"
              >
                <span className="w-5 text-right text-muted-foreground">{i + 1}.</span>
                <span className="font-mono">{word}</span>
              </li>
            ))}
          </ol>
          <Button onClick={() => setStep("confirm")}>{t("sensitiveSeed.iWroteItDown")}</Button>
        </>
      ) : (
        <>
          <h1 className="text-xl font-semibold">{t("sensitiveSeed.confirmTitle")}</h1>
          <div className="flex w-full max-w-sm flex-col gap-3">
            {quizIndices.map((i) => (
              <label key={i} className="flex items-center gap-2 text-sm">
                <span className="w-16 shrink-0 text-muted-foreground">
                  {t("sensitiveSeed.word", { number: i + 1 })}
                </span>
                <Input
                  autoComplete="off"
                  autoCorrect="off"
                  spellCheck={false}
                  value={answers[i] ?? ""}
                  onChange={(e) => setAnswers((a) => ({ ...a, [i]: e.target.value }))}
                />
              </label>
            ))}
          </div>
          <Button disabled={!allCorrect} onClick={onDone}>
            {t("sensitiveSeed.confirm")}
          </Button>
        </>
      )}
    </div>
  );
}

function pickQuizIndices(wordCount: number, count = 3): number[] {
  const indices = Array.from({ length: wordCount }, (_, i) => i);
  for (let i = indices.length - 1; i > 0; i--) {
    const j = Math.floor(Math.random() * (i + 1));
    [indices[i], indices[j]] = [indices[j], indices[i]];
  }
  return indices.slice(0, Math.min(count, wordCount)).sort((a, b) => a - b);
}
