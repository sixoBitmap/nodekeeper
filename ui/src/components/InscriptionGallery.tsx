import { useTranslation } from "react-i18next";
import type { Environment } from "@/bindings/Environment";
import { ErrorPanel } from "@/components/ErrorPanel";
import { InscriptionPreviewTile } from "@/components/InscriptionPreviewTile";
import { useWalletInscriptions } from "@/hooks/useWalletInscriptions";
import { friendlyError } from "@/lib/error-messages";

/**
 * docs/SPEC.md item 3's "Inscriptions gallery: static previews by
 * default, loaded from the ord server and rendered per Foundation D."
 * Each preview is a sandboxed iframe pointed at the environment's own
 * ord server -- `sandbox="allow-scripts"` with no `allow-same-origin`,
 * so embedded content (an HTML/SVG inscription's own script included)
 * runs in an opaque origin: no access to Nodekeeper's storage/cookies,
 * no Tauri IPC (unreachable from an iframe regardless), no top-level
 * navigation, no popups. `/preview/<id>` (not `/content/<id>`) is the
 * embed target -- DECISIONS.md Phase 5 VERIFY confirmed it's the one
 * ord itself designed for cross-embedding (tighter CSP, works for every
 * content type via ord's own wrapper), and that ord's `/content`
 * response's wildcard CORS header is what lets the sandboxed iframe's
 * opaque-origin fetch back to `/content` succeed.
 */
export function InscriptionGallery({ environment }: { environment: Environment }) {
  const { t } = useTranslation();
  const { inscriptions, error, loading } = useWalletInscriptions(environment.chain);

  return (
    <div className="space-y-2">
      <h3 className="text-sm font-medium">{t("wallet.gallery.title")}</h3>

      {error &&
        (() => {
          const friendly = friendlyError(error);
          return (
            <ErrorPanel
              title={friendly.title}
              message={friendly.whatToDo ? `${friendly.message} ${friendly.whatToDo}` : friendly.message}
              technicalDetails={error.message}
            />
          );
        })()}

      {loading && <p className="text-sm text-muted-foreground">{t("wallet.gallery.loading")}</p>}

      {!loading && !error && inscriptions && inscriptions.length === 0 && (
        <p className="text-sm text-muted-foreground">{t("wallet.gallery.none")}</p>
      )}

      {!loading && !error && inscriptions && inscriptions.length > 0 && (
        <div className="grid grid-cols-2 gap-3 sm:grid-cols-3 md:grid-cols-4">
          {inscriptions.map((inscription) => (
            <div
              key={inscription.id}
              className="overflow-hidden rounded-md border border-border bg-card"
            >
              <InscriptionPreviewTile
                environment={environment}
                id={inscription.id}
                label={t("wallet.gallery.itemLabel", { id: inscription.id })}
              />
              <code className="block truncate px-2 py-1 text-xs text-muted-foreground">
                {inscription.id}
              </code>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
