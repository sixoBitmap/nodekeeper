import { useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import type { Environment } from "@/bindings/Environment";
import type { ConsoleCommandPreview } from "@/bindings/ConsoleCommandPreview";
import type { TypedError } from "@/bindings/TypedError";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { ConfirmDialog } from "@/components/ConfirmDialog";

interface HistoryEntry {
  id: number;
  display: string;
  status: "success" | "error" | "blocked";
  output: string;
}

/**
 * docs/SPEC.md item 6: raw bitcoin-cli/ord commands, with a safety
 * layer in front. Backend does the actual classifying/blocking
 * (`console_classify`/`console_run` -- see DECISIONS.md "Phase 7 —
 * console execution wiring"); this screen just drives that: read-only
 * commands run immediately, state-changing ones go through the shared
 * `ConfirmDialog` ("Learn mode" shows the exact command), and a
 * `blocked_reason` is shown as a refusal with no run attempted at all.
 * One console per environment (matching every other screen in the
 * app) rather than the spec's independently-tabbed-per-environment
 * consoles -- multiple simultaneous tabs is a tracked follow-up, not
 * required for the console to be genuinely useful.
 */
export function ConsoleScreen({ environment }: { environment: Environment }) {
  const { t } = useTranslation();
  const [input, setInput] = useState("");
  const [history, setHistory] = useState<HistoryEntry[]>([]);
  const [busy, setBusy] = useState(false);
  const [pending, setPending] = useState<{
    commandLine: string;
    preview: ConsoleCommandPreview;
    dryRunOutput: string | null;
  } | null>(null);
  const [confirmOpen, setConfirmOpen] = useState(false);
  const nextId = useRef(0);

  const appendHistory = (entry: Omit<HistoryEntry, "id">) => {
    const id = nextId.current++;
    setHistory((h) => [...h, { ...entry, id }]);
  };

  const runNow = async (commandLine: string, display: string, dryRun: boolean) => {
    try {
      const result = await invoke("console_run", {
        chain: environment.chain,
        commandLine,
        dryRun,
      });
      appendHistory({ display, status: "success", output: JSON.stringify(result, null, 2) });
    } catch (e) {
      appendHistory({ display, status: "error", output: (e as TypedError).message });
    }
  };

  const submit = async () => {
    const commandLine = input.trim();
    if (!commandLine || busy) return;
    setInput("");
    setBusy(true);
    try {
      let preview: ConsoleCommandPreview;
      try {
        preview = await invoke<ConsoleCommandPreview>("console_classify", { commandLine });
      } catch (e) {
        appendHistory({ display: commandLine, status: "error", output: (e as TypedError).message });
        return;
      }

      if (preview.blocked_reason) {
        appendHistory({ display: preview.display, status: "blocked", output: preview.blocked_reason });
        return;
      }

      if (preview.read_only) {
        await runNow(commandLine, preview.display, false);
        return;
      }

      let dryRunOutput: string | null = null;
      if (preview.supports_dry_run) {
        try {
          const result = await invoke("console_run", {
            chain: environment.chain,
            commandLine,
            dryRun: true,
          });
          dryRunOutput = JSON.stringify(result, null, 2);
        } catch (e) {
          appendHistory({ display: preview.display, status: "error", output: (e as TypedError).message });
          return;
        }
      }

      setPending({ commandLine, preview, dryRunOutput });
      setConfirmOpen(true);
    } finally {
      setBusy(false);
    }
  };

  const confirmAndRun = async () => {
    if (!pending) return;
    setConfirmOpen(false);
    await runNow(pending.commandLine, pending.preview.display, false);
    setPending(null);
  };

  return (
    <div className="flex h-full flex-col gap-3 p-4">
      <h2 className="text-lg font-semibold">{t("console.title")}</h2>

      <div className="flex-1 space-y-3 overflow-y-auto rounded-md border border-border bg-muted/30 p-3 font-mono text-xs">
        {history.length === 0 && (
          <p className="font-sans text-sm text-muted-foreground">{t("console.empty")}</p>
        )}
        {history.map((entry) => (
          <div key={entry.id}>
            <p className="text-foreground">
              <span className="text-muted-foreground">[{environment.chain}] $ </span>
              {entry.display}
            </p>
            <pre
              className={`whitespace-pre-wrap ${entry.status === "success" ? "text-muted-foreground" : "text-danger"}`}
            >
              {entry.output}
            </pre>
          </div>
        ))}
      </div>

      <form
        onSubmit={(e) => {
          e.preventDefault();
          void submit();
        }}
        className="flex items-center gap-2"
      >
        <span className="font-mono text-sm text-muted-foreground">[{environment.chain}] $</span>
        <Input
          value={input}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={(e) => {
            // Belt-and-suspenders alongside the form's native submit-on-
            // Enter: explicit handling for a command-line-style input,
            // where Enter-to-run is the primary interaction.
            if (e.key === "Enter") {
              e.preventDefault();
              void submit();
            }
          }}
          placeholder={t("console.placeholder")}
          className="font-mono"
          disabled={busy}
        />
        <Button type="submit" disabled={busy || !input.trim()}>
          {t("console.run")}
        </Button>
      </form>

      {pending && (
        <ConfirmDialog
          open={confirmOpen}
          onOpenChange={(open) => {
            setConfirmOpen(open);
            if (!open) setPending(null);
          }}
          environment={environment}
          title={t("console.confirmTitle")}
          description={
            pending.dryRunOutput
              ? t("console.confirmDescriptionWithPreview", { preview: pending.dryRunOutput })
              : t("console.confirmDescription")
          }
          command={pending.preview.display}
          learnMode
          onConfirm={() => void confirmAndRun()}
          confirmLabel={t("console.run")}
        />
      )}
    </div>
  );
}
