import { useEffect, useRef, useState } from "react";

/**
 * Whether this is a real Tauri webview, not the browser dev preview.
 * `window.__TAURI_INTERNALS__` alone isn't a reliable signal here --
 * `dev-tauri-mock.ts`'s `mockIPC` sets it too (just `.invoke` and a few
 * other members, confirmed by reading `@tauri-apps/api/mocks`' source),
 * so both contexts have it defined. `.metadata` specifically is only
 * ever populated by the real Tauri runtime (or by the mocks module's
 * separate `mockWindows()`, which this project doesn't call).
 */
export function isRealTauriRuntime(): boolean {
  return Boolean(
    (window as { __TAURI_INTERNALS__?: { metadata?: unknown } }).__TAURI_INTERNALS__?.metadata,
  );
}

/**
 * Subscribes to Tauri's core `onDragDropEvent` webview API (no plugin
 * needed) and calls `onDrop` with the dropped files' real filesystem
 * paths. Shared by every Inscribe studio mode that accepts files
 * (docs/SPEC.md item 4).
 *
 * There's no equivalent event source in the browser dev preview.
 * Loaded via a *dynamic* import, not a static one:
 * `@tauri-apps/api/webview` reads `window.__TAURI_INTERNALS__.metadata`
 * at module-*evaluation* time, not just when `getCurrentWebview()` is
 * called -- confirmed live (DECISIONS.md Phase 6), a static import
 * crashes the whole app in that preview ("Cannot read properties of
 * undefined (reading 'currentWindow')"), so the module must never even
 * be evaluated outside a real Tauri context. Same try/catch-and-warn
 * shape as `store/monitor.ts`'s exec-event subscription, for the same
 * underlying reason (Phase 3).
 */
export function useDragDropFiles(onDrop: (paths: string[]) => void) {
  const [dragOver, setDragOver] = useState(false);
  // Always calls the latest `onDrop` without re-subscribing on every
  // render -- the subscription itself only needs to happen once. Kept
  // current via its own effect (not a render-time assignment, which
  // this project's eslint config flags: "Cannot access refs during
  // render").
  const onDropRef = useRef(onDrop);
  useEffect(() => {
    onDropRef.current = onDrop;
  });

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    void (async () => {
      try {
        const { getCurrentWebview } = await import("@tauri-apps/api/webview");
        const fn = await getCurrentWebview().onDragDropEvent((event) => {
          setDragOver(event.payload.type === "over" || event.payload.type === "enter");
          if (event.payload.type === "drop" && event.payload.paths.length > 0) {
            onDropRef.current(event.payload.paths);
          }
        });
        if (cancelled) fn();
        else unlisten = fn;
      } catch (e) {
        console.warn("useDragDropFiles: failed to subscribe to drag-drop events", e);
      }
    })();
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  return { dragOver };
}
