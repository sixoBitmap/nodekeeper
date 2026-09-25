import type { Environment } from "@/bindings/Environment";

/**
 * A single sandboxed inscription preview (docs/SPEC.md item 3/4,
 * Foundation D) -- `sandbox="allow-scripts"` with no `allow-same-
 * origin`, so embedded content runs in an opaque origin: no access to
 * Nodekeeper's storage/cookies, no Tauri IPC, no top-level navigation,
 * no popups. `/preview/<id>` (not `/content/<id>`) is the embed target
 * -- DECISIONS.md Phase 5 VERIFY confirmed it's the one ord itself
 * designed for cross-embedding. Extracted from `InscriptionGallery` so
 * the reinscribe mode's inscription picker and sat-history display
 * (Phase 6) don't duplicate this markup a third time.
 */
export function InscriptionPreviewTile({
  environment,
  id,
  label,
  className = "h-32 w-full border-0 bg-background",
}: {
  environment: Environment;
  id: string;
  label: string;
  className?: string;
}) {
  return (
    <iframe
      src={`http://127.0.0.1:${environment.ord_port}/preview/${id}`}
      sandbox="allow-scripts"
      referrerPolicy="no-referrer"
      loading="lazy"
      title={label}
      className={className}
    />
  );
}
