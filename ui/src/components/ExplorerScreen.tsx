import { useState } from "react";
import { useTranslation } from "react-i18next";
import type { Environment } from "@/bindings/Environment";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";

/**
 * docs/SPEC.md item 5: "Search inscriptions, sats, transactions,
 * addresses, blocks, and runes via the environment's local ord server
 * ... The embedded explorer follows Foundation D." Foundation D says
 * "Same rules for the embedded explorer" as inscription previews --
 * so this embeds ord's own explorer pages in a sandboxed iframe rather
 * than re-implementing search/result rendering: `/search/<query>` is
 * ord's real type-auto-detecting endpoint (redirects to /inscription,
 * /sat, /tx, /block, or /address -- VERIFIED live, DECISIONS.md
 * "Phase 7 — VERIFY: ord's explorer/search HTTP surface"). Same
 * `sandbox="allow-scripts"` / no `allow-same-origin` /
 * `referrerPolicy="no-referrer"` discipline as
 * `InscriptionPreviewTile`.
 */
export function ExplorerScreen({ environment }: { environment: Environment }) {
  const { t } = useTranslation();
  const [query, setQuery] = useState("");
  const [searchedQuery, setSearchedQuery] = useState<string | null>(null);

  const submit = (e: React.FormEvent) => {
    e.preventDefault();
    const trimmed = query.trim();
    if (!trimmed) return;
    setSearchedQuery(trimmed);
  };

  return (
    <div className="flex h-full flex-col gap-3 p-4">
      <div className="space-y-2">
        <h2 className="text-lg font-semibold">{t("explorer.title")}</h2>
        <p className="text-xs text-muted-foreground">
          {t("explorer.indexNote", { options: indexOptionsLabel(environment, t) })}
        </p>
        <form onSubmit={submit} className="flex gap-2">
          <Input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder={t("explorer.placeholder")}
          />
          <Button type="submit" disabled={!query.trim()}>
            {t("explorer.search")}
          </Button>
        </form>
      </div>

      <div className="min-h-0 flex-1 overflow-hidden rounded-md border border-border">
        {searchedQuery ? (
          <iframe
            src={`http://127.0.0.1:${environment.ord_port}/search/${encodeURIComponent(searchedQuery)}`}
            sandbox="allow-scripts"
            referrerPolicy="no-referrer"
            loading="lazy"
            title={t("explorer.title")}
            className="h-full w-full border-0 bg-background"
          />
        ) : (
          <div className="flex h-full items-center justify-center text-sm text-muted-foreground">
            {t("explorer.empty")}
          </div>
        )}
      </div>
    </div>
  );
}

function indexOptionsLabel(environment: Environment, t: (key: string) => string): string {
  const enabled = [
    environment.index_options.index_sats && t("ord.indexOption.sats"),
    environment.index_options.index_runes && t("ord.indexOption.runes"),
    environment.index_options.index_addresses && t("ord.indexOption.addresses"),
  ].filter((label): label is string => Boolean(label));
  return enabled.length > 0 ? enabled.join(", ") : t("ord.indexOption.none");
}
