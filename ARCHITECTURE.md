# Architecture (initial draft — Phase 0)

This is a draft written before any code exists, based on docs/SPEC.md. It
will be revised as phases land and VERIFY results (DECISIONS.md) confirm or
change assumptions.

## Workspace layout

```
crates/nk-core     environment model, path resolution, config generation
crates/nk-exec     central command executor, redaction, sensitive channel
crates/nk-proc     process manager, PID/lock files, graceful stop
crates/nk-verify   downloads, SHA-256, PGP verification, pinned values
crates/nk-rpc      Bitcoin Core JSON-RPC client (calls go through nk-exec)
crates/nk-ord      ord CLI and ord server API wrappers
crates/nk-secrets  keychain, encrypted secrets file, zeroization
crates/nk-scripts  script runner: interpreter detection, env vars, execution
crates/nk-store    SQLite and migrations
crates/nk-testkit  regtest fixture for integration tests
src-tauri/         thin Tauri command layer only (no business logic)
ui/                React frontend
```

Business logic lives in crates so it's testable without the GUI. Tauri
command handlers stay thin wrappers.

## The environment model

Each network (mainnet, regtest, signet, testnet4) is an independent
"environment": its own bitcoind process, ord server process, data
directories, cookie file, RPC/P2P/ord-HTTP ports, index options, wallets,
history, logs, command history, console history, and templates. Multiple
environments run concurrently. No cross-environment command/script/wallet
action/template.

Default ports (RPC/P2P): mainnet 8332/8333, regtest 18443/18444, signet
38332/38333, testnet4 48332/48333. Each environment also gets a distinct ord
server port (8080, 8081, ...). Ports are checked free before start and are
user-configurable.

Data layout (installed and portable, always relative paths):
```
data/<environment>/bitcoin
data/<environment>/ord
```
Per-chain subfolders (e.g. regtest cookie under `.../bitcoin/regtest/`) are
resolved by one path-resolution module in `nk-core` — never hard-coded
elsewhere. Windows long-path support required (manifest `longPathAware` +
`\\?\`-prefixed paths as needed).

## The central command executor (`nk-exec`)

Single choke point for every ord CLI command, bitcoin-cli command, and RPC
call:
- argument arrays only, never shell-interpolated strings;
- tags each command with environment, source, and triggering app action;
- redacts secrets before logging/display/storage/export;
- secrets go via stdin or RPC params, never argv;
- SENSITIVE OUTPUT channel: mnemonic-bearing command output goes only to the
  full-screen seed view, never the monitor/logs/history/exports (monitor
  shows a placeholder instead);
- zeroizes secrets from memory as soon as unneeded;
- streams status/output to the Live Command Monitor.

Enforced mechanically: `std::process::Command`/`tokio::process::Command`
usable only inside `nk-exec`/`nk-proc` (clippy `disallowed-methods` in CI).

## The process manager (`nk-proc`)

Tracks every child process per environment via PID files; detects and offers
to attach to already-running instances (or refuses if the data dir doesn't
match); recovers from crashes (offer attach or safe shutdown); graceful stop
(`stop` RPC for bitcoind, SIGINT/CTRL_BREAK_EVENT for ord with a configurable
timeout, warns before a force-kill that could corrupt the ord index);
single-instance lock file per data folder (hostname/PID/timestamp) with safe
stale-lock detection.

## Webview security model

Inscriptions can contain arbitrary HTML/SVG/JS and must never reach app IPC
or the wallet: served from the environment's own ord server origin
(`http://127.0.0.1:<port>`), rendered only in sandboxed iframes (no
`allow-same-origin`; `allow-scripts` opt-in per inscription), strict app CSP
listing exact configured ord origins (no wildcards), and a locked-down Tauri
capabilities file (IPC only for the main window's own origin).

Phase 1 baseline (`src-tauri/tauri.conf.json`'s `security.csp`, set now
rather than left `null`, since a strict default doesn't need inscription
rendering to be worth having): `default-src 'self'` with `connect-src`/
`img-src` scoped to exactly what Tauri's own IPC and asset protocol need,
`style-src 'self' 'unsafe-inline'` (Radix/shadcn inject inline styles for
popover/dialog positioning). `capabilities/default.json` is already scoped
to `"windows": ["main"]` (the scaffold default) — with exactly one window
and no untrusted content source yet, this is sufficient for now; revisit
alongside the CSP once the ord-content webview/iframe exists.

Phase 5: `frame-src` now lists the 4 fixed default ord ports
(`http://127.0.0.1:8080` mainnet, `8081` regtest, `8082` signet, `8083`
testnet4 — `Chain::default_ord_port`) explicitly, no wildcard — this is
what makes iframes possible at all; the sandboxed
`<iframe sandbox="allow-scripts">` component itself (no
`allow-same-origin`) lands with the inscriptions gallery, once there's
real inscription content to render and the "malicious test HTML/SVG
inscription cannot call Tauri IPC or read app data" acceptance
criterion has something real to test against.

## Secrets storage (`nk-secrets`)

Installed mode: OS keychain (`keyring` crate). Linux without a Secret
Service provider: encrypted secrets file fallback. Portable mode (and the
Linux fallback): encrypted secrets file, Argon2id-derived key from a user
master password, XChaCha20-Poly1305, decrypted key kept in memory only.
Wallet encryption passphrases and mnemonics are **never** stored here or
anywhere persistent.

## Index options and feature gating

Each environment records ord index options (index-sats, index-runes,
index-addresses). Dependent features are hidden/explained rather than
erroring when the option is off. Exact feature->option mapping is a Phase 0
VERIFY item (see DECISIONS.md). Regtest enables all index options by default.

## Typed IPC

Rust types that cross the Tauri IPC boundary derive `ts_rs::TS`; a test in
`src-tauri/src/lib.rs` (`export_bindings`) regenerates
`ui/src/bindings/*.ts` from them on every `cargo test`/`just check` — the
frontend never hand-writes a duplicate type.

**u64/i64 gotcha**: ts-rs maps `u64`/`i64` to TS `bigint` by default
(technically lossless), but Tauri's IPC serializes command results as plain
JSON via serde_json, which the frontend receives as a JS `number`, not a
`bigint` — so the default mapping doesn't match what actually arrives at
runtime (discovered while building the `system_check` command's
`SystemCheck` type, Phase 1). Fix: annotate any `u64`/`i64` IPC field with
`#[ts(type = "number")]` (safe as long as the value can't realistically
exceed 2^53 — true for byte counts, satoshi amounts, block heights, etc.).
Apply this to every new u64/i64 field that crosses IPC, not just the ones
that happen to hit it first.

## Suggested libraries

Rust: tokio, serde, thiserror, tracing, reqwest (rustls), sha2,
sequoia-openpgp (pure-Rust backend), argon2, chacha20poly1305, zeroize,
secrecy, keyring, rusqlite (+ migration tool), tauri-plugin-updater.
Frontend: Vite, TanStack Query, Zustand, react-i18next, Vitest, Testing
Library. Exact versions to be confirmed and recorded in DECISIONS.md.

## Phase 0 spike results (summary)

Full detail and exact commands in DECISIONS.md. Key confirmations: ord
wallet commands work normally against an encrypted Core wallet via the
standard `walletpassphrase` RPC (no special handling needed); ord shuts down
cleanly on Windows via `CREATE_NEW_PROCESS_GROUP` + `CTRL_BREAK_EVENT`
(macOS/Linux SIGINT still needs a CI check); ord server binds `0.0.0.0` by
default so `nk-ord`'s server-launch config must always pass `--address
127.0.0.1 --http --http-port <port>` together; `--dry-run` exists on both
`wallet send` and `wallet inscribe`; reinscribe (single + batch) works as
documented; the index-option -> feature mapping in Foundation F is
confirmed exactly, except rune listing fails *soft* (empty, not an error)
without `index-runes`, so `nk-ord`/the UI must gate it proactively rather
than relying on ord's own error.

Two approved deviations from the spec came out of this (see DECISIONS.md
"Approved deviations"): **no BIP39-passphrase support** anywhere (ord only
accepts it via argv, which the security rules forbid for secrets), and
**Windows will be code-signed, macOS will not** (for now) — the macOS
first-launch flow must handle Gatekeeper + quarantine-attribute removal.

## Open questions

None blocking Phase 1. Future VERIFY items (e.g. macOS/Linux SIGINT
behavior) tracked in DECISIONS.md as they come up.
