import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import type { SystemCheck } from "@/bindings/SystemCheck";
import type { TypedError } from "@/bindings/TypedError";
import { Button } from "@/components/ui/button";
import { ErrorPanel } from "@/components/ErrorPanel";
import { friendlyError } from "@/lib/error-messages";
import { formatBytes } from "@/lib/format";

/**
 * docs/SPEC.md item 1 (setup wizard): "Check OS, CPU, RAM, and disk
 * space" plus "Let the user choose the data directory (including
 * external drives)." Calls the real `system_check` Tauri command
 * (nk-core) — see ARCHITECTURE.md "Typed IPC" for the u64-vs-bigint IPC
 * subtlety this command's type already accounts for. The disk-space
 * check runs against whatever the data directory *actually* is right
 * now (`get_environment_data_root`), not a hardcoded guess -- otherwise
 * changing the directory to an external drive would keep showing free
 * space on the wrong one, defeating the point of the picker.
 */
export function SystemCheckScreen({ onContinue }: { onContinue: () => void }) {
  const { t } = useTranslation();
  const [check, setCheck] = useState<SystemCheck | null>(null);
  const [error, setError] = useState<string | null>(null);

  const [dataDir, setDataDir] = useState<string | null>(null);
  const [changingDir, setChangingDir] = useState(false);
  const [changeDirError, setChangeDirError] = useState<TypedError | null>(null);

  const runCheck = (dir: string) => {
    invoke<SystemCheck>("system_check", { dataDir: dir })
      .then(setCheck)
      .catch((e: unknown) => setError(String(e)));
  };

  useEffect(() => {
    invoke<string>("get_environment_data_root").then((dir) => {
      setDataDir(dir);
      runCheck(dir);
    });
  }, []);

  const changeDataDir = async () => {
    const selected = await open({ directory: true, multiple: false, title: t("systemCheck.chooseDataDir") });
    if (typeof selected !== "string") return; // cancelled
    setChangingDir(true);
    setChangeDirError(null);
    try {
      await invoke("set_environment_data_root", { path: selected });
      setDataDir(selected);
      runCheck(selected);
    } catch (e) {
      setChangeDirError(e as TypedError);
    } finally {
      setChangingDir(false);
    }
  };

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

            {check.disk_filesystem !== null && (
              <>
                <dt className="text-muted-foreground">{t("systemCheck.filesystem")}</dt>
                <dd>{check.disk_filesystem}</dd>
              </>
            )}
          </dl>
        )}

        {/* docs/SPEC.md item 12: "warn if exFAT (corruption risk on
            unplug, no permission bits, macOS writes ._ metadata
            files); recommend NTFS if the user only uses Windows and
            Linux." `disk_filesystem_is_risky` is computed backend-side
            (nk-core's `is_risky_portable_filesystem`) so this screen
            doesn't carry its own copy of what counts as "risky". */}
        {check?.disk_filesystem_is_risky && (
          <p className="text-xs text-warning">{t("systemCheck.exfatWarning")}</p>
        )}

        <div className="space-y-2 rounded-md border border-border p-3">
          <p className="text-xs text-muted-foreground">{t("systemCheck.dataDirLabel")}</p>
          <code className="block break-all text-xs">{dataDir ?? "..."}</code>
          {changeDirError &&
            (() => {
              const friendly = friendlyError(changeDirError);
              return (
                <ErrorPanel
                  title={friendly.title}
                  message={friendly.whatToDo ? `${friendly.message} ${friendly.whatToDo}` : friendly.message}
                  technicalDetails={changeDirError.message}
                />
              );
            })()}
          <Button variant="outline" size="sm" disabled={changingDir} onClick={() => void changeDataDir()}>
            {t("systemCheck.changeDataDir")}
          </Button>
        </div>

        <Button className="w-full" disabled={!check} onClick={onContinue}>
          {t("systemCheck.continue")}
        </Button>
      </div>
    </div>
  );
}
