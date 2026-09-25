import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import type { Environment } from "@/bindings/Environment";
import type { ScriptInfo } from "@/bindings/ScriptInfo";
import type { InterpreterAvailability } from "@/bindings/InterpreterAvailability";
import type { TypedError } from "@/bindings/TypedError";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";

interface RunState {
  status: "success" | "error";
  output: string;
}

/**
 * docs/SPEC.md item 6: the script runner. Scripts are "trusted code
 * with full node control" -- a different trust model from the console
 * (no per-command safety classification, just environment-scoping and
 * a mandatory, always-visible warning). Live output isn't rendered
 * here: it's already streaming to the Live Command Monitor for free
 * (`run_script` goes through the same executor every other command
 * does, tagged `CommandSource::Script`), so this screen only needs to
 * show the final result once the run finishes.
 */
export function ScriptsScreen({ environment }: { environment: Environment }) {
  const { t } = useTranslation();
  const [scripts, setScripts] = useState<ScriptInfo[] | null>(null);
  const [interpreters, setInterpreters] = useState<InterpreterAvailability[] | null>(null);
  const [args, setArgs] = useState<Record<string, string>>({});
  const [running, setRunning] = useState<string | null>(null);
  const [results, setResults] = useState<Record<string, RunState>>({});

  useEffect(() => {
    void invoke<ScriptInfo[]>("list_scripts").then(setScripts);
    void invoke<InterpreterAvailability[]>("list_available_interpreters").then(setInterpreters);
  }, []);

  const availabilityFor = (language: string) =>
    interpreters?.find((i) => i.language === language)?.available ?? null;

  const run = async (script: ScriptInfo) => {
    setRunning(script.id);
    setResults((r) => ({ ...r, [script.id]: undefined as unknown as RunState }));
    const argv = (args[script.id] ?? "").trim();
    try {
      const output = await invoke<string>("run_script", {
        chain: environment.chain,
        scriptId: script.id,
        args: argv.length > 0 ? argv.split(/\s+/) : [],
      });
      setResults((r) => ({ ...r, [script.id]: { status: "success", output } }));
    } catch (e) {
      setResults((r) => ({ ...r, [script.id]: { status: "error", output: (e as TypedError).message } }));
    } finally {
      setRunning(null);
    }
  };

  if (!scripts || !interpreters) {
    return <div className="h-full bg-background" />;
  }

  return (
    <div className="space-y-4 p-4">
      <div>
        <h2 className="text-lg font-semibold">{t("scripts.title")}</h2>
        <div className="mt-2 rounded-md border border-env-mainnet/40 bg-env-mainnet/10 p-3 text-sm">
          {t("scripts.warning")}
        </div>
      </div>

      <div className="space-y-3">
        {scripts.map((script) => {
          const available = availabilityFor(script.language);
          const blockedByRegtest = script.regtest_only && environment.chain !== "regtest";
          const result = results[script.id];
          return (
            <div key={script.id} className="space-y-2 rounded-md border border-border p-3">
              <div className="flex items-center justify-between">
                <h3 className="text-sm font-medium">{script.name}</h3>
                {script.regtest_only && (
                  <span className="rounded-full bg-secondary px-2 py-0.5 text-xs text-secondary-foreground">
                    {t("scripts.regtestOnly")}
                  </span>
                )}
              </div>
              <p className="text-xs text-muted-foreground">{script.description}</p>

              {available === false && (
                <p className="text-xs text-danger">
                  {t("scripts.interpreterUnavailable", { language: script.language })}
                </p>
              )}
              {blockedByRegtest && (
                <p className="text-xs text-danger">{t("scripts.blockedByRegtest")}</p>
              )}

              <div className="flex items-center gap-2">
                <Input
                  value={args[script.id] ?? ""}
                  onChange={(e) => setArgs((a) => ({ ...a, [script.id]: e.target.value }))}
                  placeholder={t("scripts.argsPlaceholder")}
                  className="font-mono"
                  disabled={running === script.id}
                />
                <Button
                  size="sm"
                  disabled={running !== null || available !== true || blockedByRegtest}
                  onClick={() => void run(script)}
                >
                  {running === script.id ? t("scripts.running") : t("scripts.run")}
                </Button>
              </div>

              {result && (
                <pre
                  className={`overflow-x-auto rounded bg-muted p-2 text-xs ${result.status === "error" ? "text-danger" : "text-muted-foreground"}`}
                >
                  {result.output || t("scripts.noOutput")}
                </pre>
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}
