import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import type { Chain } from "@/bindings/Chain";
import type { LogWindow } from "@/bindings/LogWindow";
import type { TypedError } from "@/bindings/TypedError";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { friendlyError } from "@/lib/error-messages";

/**
 * debug.log viewer (docs/SPEC.md item 2: "tail and page large files,
 * never load a whole file; search and filter"). Backed by
 * `tail_debug_log`/`page_debug_log_before`/`search_debug_log`, which
 * only ever read a bounded byte window (`nk_core::log_tail`) — this
 * component never asks for, or holds, the whole file at once either.
 */
export function LogViewer({ chain }: { chain: Chain }) {
  const { t } = useTranslation();
  const [lines, setLines] = useState<string[]>([]);
  const [earliestOffset, setEarliestOffset] = useState<number | null>(null);
  const [reachedStart, setReachedStart] = useState(false);
  // Starts true (rather than set synchronously in the mount effect
  // below) so the effect's only setState calls happen inside a `.then`
  // callback -- see that effect's comment.
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<TypedError | null>(null);

  const [query, setQuery] = useState("");
  const [searchResults, setSearchResults] = useState<string[] | null>(null);

  // Relies on the caller keying its element by `chain` (e.g. `key={chain}`)
  // so switching environments remounts this component with fresh initial
  // state, rather than resetting state synchronously in this effect
  // (which `react-hooks/set-state-in-effect` flags as a cascading-render
  // anti-pattern -- React's own recommended fix is a `key` change, not a
  // manual reset).
  useEffect(() => {
    invoke<LogWindow>("tail_debug_log", { chain })
      .then((window) => {
        setLines(window.lines);
        setEarliestOffset(window.start_offset);
        setReachedStart(window.reached_start_of_file);
      })
      .catch((e: TypedError) => setError(e))
      .finally(() => setLoading(false));
  }, [chain]);

  const loadOlder = () => {
    if (earliestOffset === null || reachedStart || loading) return;
    setLoading(true);
    invoke<LogWindow>("page_debug_log_before", { chain, endOffset: earliestOffset })
      .then((window) => {
        setLines((prev) => [...window.lines, ...prev]);
        setEarliestOffset(window.start_offset);
        setReachedStart(window.reached_start_of_file);
      })
      .catch((e: TypedError) => setError(e))
      .finally(() => setLoading(false));
  };

  const runSearch = (q: string) => {
    setQuery(q);
    if (!q) {
      setSearchResults(null);
      return;
    }
    invoke<string[]>("search_debug_log", { chain, query: q })
      .then(setSearchResults)
      .catch((e: TypedError) => setError(e));
  };

  const visibleLines = searchResults ?? lines;

  return (
    <div className="flex h-80 flex-col rounded-lg border border-border">
      <div className="flex items-center gap-2 border-b border-border p-2">
        <Input
          placeholder={t("logViewer.searchPlaceholder")}
          value={query}
          onChange={(e) => runSearch(e.target.value)}
          className="h-8 text-xs"
        />
        {!searchResults && !reachedStart && (
          <Button size="sm" variant="outline" disabled={loading} onClick={loadOlder}>
            {t("logViewer.loadOlder")}
          </Button>
        )}
      </div>

      {error && (
        <p className="p-2 text-xs text-danger">{friendlyError(error).message}</p>
      )}

      <div className="flex-1 overflow-y-auto bg-muted/30 p-2 font-mono text-xs">
        {visibleLines.length === 0 && !loading && (
          <p className="text-muted-foreground">
            {searchResults ? t("logViewer.noMatches") : t("logViewer.empty")}
          </p>
        )}
        {visibleLines.map((line, i) => (
          // Log lines have no stable identity of their own; index is
          // fine since this list is only ever replaced wholesale, never
          // reordered in place.
          <div key={i} className="whitespace-pre-wrap break-all">
            {line}
          </div>
        ))}
      </div>
    </div>
  );
}
