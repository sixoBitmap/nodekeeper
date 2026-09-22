import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import type { SystemCheck } from "@/bindings/SystemCheck";
import { Button } from "@/components/ui/button";
import { formatBytes } from "@/lib/format";

/**
 * docs/SPEC.md item 1 (setup wizard): "Check OS, CPU, RAM, and disk
 * space." Calls the real `system_check` Tauri command (nk-core) — see
 * ARCHITECTURE.md "Typed IPC" for the u64-vs-bigint IPC subtlety this
 * command's type already accounts for.
 */
export function SystemCheckScreen({ onContinue }: { onContinue: () => void }) {
  const { t } = useTranslation();
  const [check, setCheck] = useState<SystemCheck | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    invoke<SystemCheck>("system_check", { dataDir: "data" })
      .then(setCheck)
      .catch((e: unknown) => setError(String(e)));
  }, []);

  return (
    <div className="flex h-full flex-col items-center justify-center gap-6 bg-background p-8 text-foreground">
      <div className="w-full max-w-md space-y-4">
        <h1 className="text-2xl font-semibold">{t("systemCheck.title")}</h1>

        {error && <p className="text-sm text-danger">{error}</p>}

        {check && (
          <dl className="grid grid-cols-2 gap-y-2 text-sm">
            <dt className="text-muted-foreground">{t("systemCheck.os")}</dt>
            <dd>
              {check.os} ({check.arch})
            </dd>

            <dt className="text-muted-foreground">{t("systemCheck.cpuCores")}</dt>
            <dd>{check.cpu_cores}</dd>

            <dt className="text-muted-foreground">{t("systemCheck.totalMemory")}</dt>
            <dd>{formatBytes(check.total_memory_bytes)}</dd>

            <dt className="text-muted-foreground">{t("systemCheck.availableMemory")}</dt>
            <dd>{formatBytes(check.available_memory_bytes)}</dd>

            <dt className="text-muted-foreground">{t("systemCheck.diskFree")}</dt>
            <dd>
              {check.disk_free_bytes !== null
                ? formatBytes(check.disk_free_bytes)
                : t("systemCheck.diskUnknown")}
            </dd>
          </dl>
        )}

        <Button className="w-full" disabled={!check} onClick={onContinue}>
          {t("systemCheck.continue")}
        </Button>
      </div>
    </div>
  );
}
