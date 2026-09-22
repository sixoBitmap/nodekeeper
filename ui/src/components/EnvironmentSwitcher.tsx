import { useTranslation } from "react-i18next";
import type { Chain } from "@/bindings/Chain";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { chainBgClass } from "@/lib/environment-colors";
import { selectedEnvironment, useEnvironmentStore } from "@/store/environment";

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
            </span>
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  );
}
