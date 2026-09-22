import type { Environment } from "@/bindings/Environment";
import { chainBgClass } from "@/lib/environment-colors";

/**
 * The permanent environment banner every screen shows (docs/SPEC.md item
 * 10: "a permanent banner on every screen"). Color is never the only
 * signal — the name is always rendered as text too (item 8,
 * accessibility: colorblind users).
 */
export function EnvBanner({ environment }: { environment: Environment }) {
  return (
    <div
      role="status"
      className={`flex items-center gap-2 px-4 py-1.5 text-sm font-semibold tracking-wide text-white ${chainBgClass(
        environment.chain,
      )}`}
    >
      <span className="h-2 w-2 shrink-0 rounded-full bg-white/90" aria-hidden="true" />
      <span>{environment.name.toUpperCase()}</span>
    </div>
  );
}
