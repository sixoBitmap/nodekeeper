import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import type { UnlistenFn } from "@tauri-apps/api/event";
import type { DownloadProgress } from "@/bindings/DownloadProgress";
import type { TypedError } from "@/bindings/TypedError";
import { Button } from "@/components/ui/button";
import { ErrorPanel } from "@/components/ErrorPanel";
import { friendlyError } from "@/lib/error-messages";
import { formatBytes } from "@/lib/format";

type BinaryKey = "bitcoin_core" | "ord";
type Status = "checking" | "not_installed" | "downloading" | "installed" | "error";

interface BinaryState {
  status: Status;
  path: string | null;
  progress: { downloaded: number; total: number | null } | null;
  error: TypedError | null;
}

const CHECKING: BinaryState = { status: "checking", path: null, progress: null, error: null };

/**
 * docs/SPEC.md item 1 (setup wizard), Phases 2 and 4: trigger
 * `nk_verify::bitcoin_core`/`nk_verify::ord`'s real download-and-verify
 * engine (SHA256SUMS + pinned-key signatures for Bitcoin Core, pinned
 * hashes for ord), show progress, and show the verification result --
 * fail closed, same as the backend already does. Shown between System
 * Check and the main app; skipped automatically once both binaries are
 * already configured (the settings keys `bitcoind_path`/`ord_path`
 * themselves ARE the "already done" marker -- no separate flag needed).
 */
export function BinarySetupScreen({ onContinue }: { onContinue: () => void }) {
  const { t } = useTranslation();
  const [bitcoinCore, setBitcoinCore] = useState<BinaryState>(CHECKING);
  const [ord, setOrd] = useState<BinaryState>(CHECKING);

  useEffect(() => {
    let cancelled = false;
    void Promise.all([
      invoke<string | null>("get_setting", { key: "bitcoind_path" }),
      invoke<string | null>("get_setting", { key: "ord_path" }),
    ]).then(([bitcoindPath, ordPath]) => {
      if (cancelled) return;
      if (bitcoindPath && ordPath) {
        onContinue();
        return;
      }
      setBitcoinCore({
        status: bitcoindPath ? "installed" : "not_installed",
        path: bitcoindPath,
        progress: null,
        error: null,
      });
      setOrd({
        status: ordPath ? "installed" : "not_installed",
        path: ordPath,
        progress: null,
        error: null,
      });
    });
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps -- onContinue is stable for the lifetime of this screen
  }, []);

  useEffect(() => {
    let unlisten: UnlistenFn | undefined;
    let cancelled = false;
    void (async () => {
      try {
        // Dynamic import, not a static one: matches the same caution as
        // `useDragDropFiles.ts`'s `@tauri-apps/api/webview` import --
        // this module isn't known to be equally safe to import eagerly
        // in the dev-browser preview, so don't risk it.
        const { listen } = await import("@tauri-apps/api/event");
        const fn = await listen<DownloadProgress>("setup-download-progress", (event) => {
          const { binary, downloaded_bytes, total_bytes } = event.payload;
          const setState = binary === "bitcoin_core" ? setBitcoinCore : setOrd;
          setState((s) => ({ ...s, progress: { downloaded: downloaded_bytes, total: total_bytes } }));
        });
        if (cancelled) {
          fn();
        } else {
          unlisten = fn;
        }
      } catch (e) {
        // The dev-browser preview doesn't mock Tauri's event system --
        // the download still completes via the invoke() promise below,
        // it just won't show live progress. Same degradation as
        // store/monitor.ts's exec-event subscription.
        console.warn("BinarySetupScreen: failed to subscribe to setup-download-progress", e);
      }
    })();
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  const download = async (binary: BinaryKey) => {
    const setState = binary === "bitcoin_core" ? setBitcoinCore : setOrd;
    setState({ status: "downloading", path: null, progress: null, error: null });
    try {
      const path = await invoke<string>(
        binary === "bitcoin_core" ? "download_and_verify_bitcoin_core" : "download_and_verify_ord",
      );
      setState({ status: "installed", path, progress: null, error: null });
    } catch (e) {
      setState({ status: "error", path: null, progress: null, error: e as TypedError });
    }
  };

  const bothInstalled = bitcoinCore.status === "installed" && ord.status === "installed";

  return (
    <div className="flex h-full flex-col items-center justify-center gap-6 bg-background p-8 text-foreground">
      <div className="w-full max-w-md space-y-4">
        <div>
          <h1 className="text-2xl font-semibold">{t("binarySetup.title")}</h1>
          <p className="mt-1 text-sm text-muted-foreground">{t("binarySetup.intro")}</p>
        </div>

        <BinaryCard
          title={t("binarySetup.bitcoinCore")}
          state={bitcoinCore}
          onDownload={() => void download("bitcoin_core")}
        />
        <BinaryCard title={t("binarySetup.ord")} state={ord} onDownload={() => void download("ord")} />

        <Button className="w-full" disabled={!bothInstalled} onClick={onContinue}>
          {t("systemCheck.continue")}
        </Button>
      </div>
    </div>
  );
}

function BinaryCard({
  title,
  state,
  onDownload,
}: {
  title: string;
  state: BinaryState;
  onDownload: () => void;
}) {
  const { t } = useTranslation();

  return (
    <div className="space-y-2 rounded-md border border-border p-3">
      <div className="flex items-center justify-between">
        <h2 className="text-sm font-medium">{title}</h2>
        <span
          className={
            state.status === "installed"
              ? "text-xs font-medium text-success"
              : state.status === "error"
                ? "text-xs font-medium text-danger"
                : "text-xs text-muted-foreground"
          }
        >
          {t(`binarySetup.status.${state.status}`)}
        </span>
      </div>

      {state.status === "installed" && state.path && (
        <code className="block break-all text-xs text-muted-foreground">{state.path}</code>
      )}

      {state.status === "downloading" && <DownloadProgressBar progress={state.progress} />}

      {state.status === "error" &&
        state.error &&
        (() => {
          const friendly = friendlyError(state.error);
          return (
            <ErrorPanel
              title={friendly.title}
              message={friendly.whatToDo ? `${friendly.message} ${friendly.whatToDo}` : friendly.message}
              technicalDetails={state.error.message}
            />
          );
        })()}

      {(state.status === "not_installed" || state.status === "error") && (
        <Button variant="outline" size="sm" onClick={onDownload}>
          {state.status === "error" ? t("binarySetup.retry") : t("binarySetup.download")}
        </Button>
      )}
    </div>
  );
}

function DownloadProgressBar({
  progress,
}: {
  progress: { downloaded: number; total: number | null } | null;
}) {
  const { t } = useTranslation();
  if (!progress) {
    return <p className="text-xs text-muted-foreground">{t("binarySetup.starting")}</p>;
  }
  const { downloaded, total } = progress;
  if (!total) {
    return (
      <p className="text-xs text-muted-foreground">
        {t("binarySetup.downloadedNoTotal", { size: formatBytes(downloaded) })}
      </p>
    );
  }
  const percent = Math.min(100, Math.round((downloaded / total) * 100));
  return (
    <div>
      <div className="h-2 w-full overflow-hidden rounded-full bg-muted">
        <div className="h-full bg-primary transition-[width]" style={{ width: `${percent}%` }} />
      </div>
      <p className="mt-1 text-xs text-muted-foreground">
        {t("binarySetup.downloadedOfTotal", { downloaded: formatBytes(downloaded), total: formatBytes(total) })} (
        {percent}%)
      </p>
    </div>
  );
}
