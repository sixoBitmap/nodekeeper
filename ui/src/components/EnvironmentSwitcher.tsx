import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import type { Chain } from "@/bindings/Chain";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { chainBgClass } from "@/lib/environment-colors";
import { selectedEnvironment, useEnvironmentStore } from "@/store/environment";

const POLL_INTERVAL_MS = 3000;

/**
 * The top-bar environment switcher (docs/SPEC.md item 10): every
 * configured environment with a status badge. Switching only changes
 * which environment the UI shows — it never starts or stops anything.
 * Phase 1 has no persisted environment configuration yet, so this lists
 * `list_default_environments`'s one-per-chain defaults (see
 * src-tauri/src/lib.rs).
 */
export function EnvironmentSwitcher() {
  const { t } = useTranslation();
  const environments = useEnvironmentStore((s) => s.environments);
  const selected = useEnvironmentStore(selectedEnvironment);
  const select = useEnvironmentStore((s) => s.select);
  // Text label, never color alone (docs/SPEC.md item 8: colors always
  // paired with a text label) -- "any service running" per chain,
  // same signal `OverviewScreen`'s cards show in more detail.
  const [runningChains, setRunningChains] = useState<Set<Chain>>(new Set());

  useEffect(() => {
    let cancelled = false;
    const poll = () => {
      void Promise.all(
        environments.map((env) =>
          Promise.all([
            invoke<boolean>("is_node_running", { chain: env.chain }),
            invoke<boolean>("is_ord_running", { chain: env.chain }),
          ]).then(([nodeUp, ordUp]) => (nodeUp || ordUp ? env.chain : null)),
        ),
      ).then((results) => {
        if (!cancelled) {
          setRunningChains(new Set(results.filter((c): c is Chain => c !== null)));
        }
      });
    };
    poll();
    const id = window.setInterval(poll, POLL_INTERVAL_MS);
    return () => {
      cancelled = true;
      window.clearInterval(id);
    };
  }, [environments]);

  if (!selected) return null;

  return (
    <Select value={selected.chain} onValueChange={(value) => select(value as Chain)}>
      <SelectTrigger className="w-40" aria-label={t("environmentSwitcher.label")}>
        <SelectValue />
      </SelectTrigger>
      <SelectContent>
        {environments.map((env) => (
          <SelectItem key={env.chain} value={env.chain}>
            <span className="flex items-center gap-2">
              <span className={`h-2 w-2 rounded-full ${chainBgClass(env.chain)}`} aria-hidden="true" />
              {env.name}
              {runningChains.has(env.chain) && (
                <span className="text-xs text-muted-foreground">{t("environmentSwitcher.running")}</span>
              )}
            </span>
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  );
}
