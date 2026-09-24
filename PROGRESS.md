# Progress

Checklist of every phase, its tasks, and its acceptance criteria. Update
after every completed task.

## Phase 0 — Feasibility spike (no app code)

Tasks:
- [x] Save spec verbatim as docs/SPEC.md
- [x] Create CLAUDE.md, ARCHITECTURE.md (initial draft), DECISIONS.md,
      PROGRESS.md
- [x] In spikes/, download current Bitcoin Core and ord, run on regtest
- [x] VERIFY: encrypted Core wallet + ord wallet commands, unlock method
      (confirmed, no conflict)
- [x] VERIFY: ord graceful shutdown (SIGINT / CTRL_BREAK_EVENT) (Windows
      confirmed live; macOS/Linux SIGINT needs a CI job — no such machine
      available in this session)
- [x] VERIFY: which ord commands accept secrets via stdin (mnemonic/
      descriptor: yes; BIP39 passphrase: argv-only — STOP AND ASK A)
- [x] VERIFY: ord server default bind address + flag to change it
      (defaults to 0.0.0.0, confirmed; `--address`/`--http` both required)
- [x] VERIFY: `ord wallet send --dry-run` / `inscribe --dry-run`
      existence/output (both exist and confirmed live)
- [x] VERIFY: reinscribe syntax, batch reinscribe support (both confirmed,
      full round trip tested live on regtest)
- [x] VERIFY: index-option -> feature mapping (confirmed empirically:
      sat views need index-sats, address lookups need index-addresses,
      rune listing needs index-runes but fails soft not hard)
- [x] VERIFY: per-chain data paths (Bitcoin Core, ord) (confirmed live)
- [x] VERIFY: testnet4 support; ord release checksums (testnet4 supported,
      should not be hidden; ord has no maintainer-signed checksums file)
- [x] VERIFY: built-in regtest command (e.g. `ord env`) (confirmed exists;
      Nodekeeper uses its own process manager instead)
- [x] Record all VERIFY results in DECISIONS.md with exact commands + output
- [x] Present summary, ask code-signing question, flag spec conflicts
      (see DECISIONS.md "STOP AND ASK")
- [x] [MANUAL] User has read DECISIONS.md, answered the two STOP AND ASK
      items (BIP39 passphrase: not supported; code signing: Windows yes,
      macOS no for now), approved

**Phase 0 complete (2026-09-22).** Next: begin Phase 1 task breakdown.

## Phase 1 — Foundation — complete (2026-09-22)

Every task and every acceptance criterion below is done, including the
real GitHub Actions run (green on all 3 OSes) — see the CI item and the
acceptance-criteria list further down. Repo:
https://github.com/sixoBitmap/nodekeeper (private).

Dev-environment setup (2026-09-22): this machine had no Rust toolchain,
no C++ linker, and an outdated Node.js at all. Installed Rust stable
(MSVC host) via rustup; Visual Studio Build Tools (C++ workload, MSVC +
Windows 11 SDK) for `link.exe` (blocked once on low disk space — user
freed space via Windows Update cleanup, retried with a leaner component
set, succeeded); Node.js upgraded 20.17.0 -> 24.21.0 LTS (current
frontend tooling needs Node >=20.19). All details and exact
versions/commands in DECISIONS.md.

Tasks (one at a time: implement -> test -> quality gate -> commit -> tick):

Environment / tooling
- [x] Confirm `cargo build` links successfully (MSVC toolchain smoke test)
- [x] Install `just` (cargo install just) for the quality-gate command

Cargo workspace
- [x] Root `Cargo.toml` workspace + `crates/nk-core`, `nk-exec`, `nk-proc`,
      `nk-verify`, `nk-rpc`, `nk-ord`, `nk-secrets`, `nk-store`,
      `nk-testkit` skeletons (lib.rs stub + Cargo.toml each, compiles)
- [x] `src-tauri/` thin Tauri 2 app skeleton (no business logic), wired to
      the workspace — scaffolded via `npm create tauri-app`, moved to the
      repo root per the spec's layout (see DECISIONS.md for the
      `beforeDevCommand`/`cwd` fix that required)
- [x] clippy `disallowed-methods` config banning
      `std::process::Command`/`tokio::process::Command` outside
      nk-exec/nk-proc; deliberately verified it fails the build once (a
      violation added to nk-core, confirmed, recorded in DECISIONS.md,
      then removed)
- [x] `Justfile` with `just check` (cargo fmt --check, cargo clippy -D
      warnings, cargo test, tsc, eslint, vitest)

CI
- [x] `.github/workflows/ci.yml`: matrix over windows-latest, macos-latest,
      ubuntu-latest running the quality gate. Pushed to
      https://github.com/sixoBitmap/nodekeeper (private) and **verified
      green on all 3 OSes** (run
      https://github.com/sixoBitmap/nodekeeper/actions/runs/35779301209).
      The first push failed on all 3 (real bug, not flakiness): CI pinned
      Node 20 while local dev had been upgraded to Node 24 earlier this
      session, and jsdom's fetch/Cache polyfill broke against Node 20's
      older internals (`webidl.util.markAsUncloneable is not a
      function`) — never seen locally since local testing only ran
      against Node 24. Also fixed the same `npm run tauri` working-
      directory issue in the "App builds" step that the Justfile's
      `build-app` recipe already had to work around. See DECISIONS.md.

Frontend shell
- [x] Vite + React + TypeScript + Tailwind (v4) scaffold in `ui/`, wired
      into the Tauri app; verified with a real `tauri build --debug
      --no-bundle` (Rust backend + frontend linked together, not just each
      half separately)
- [x] shadcn/ui: initialized (Vite template, Radix base, Nova preset).
      Its own tokens (`--background`, `--card`, `--primary`, etc.) were
      merged with the hand-written design tokens rather than left as two
      parallel systems — only the per-environment/status colors (which
      shadcn has no equivalent for) stayed as separate `--color-env-*`/
      `--color-success` etc. tokens. Added `button`, `dialog`, `checkbox`,
      `input`, `select` components as the shared components below needed
      them
- [x] Typed IPC scaffold: used **ts-rs**, not tauri-specta (its published
      docs for the current version looked inconsistent/stale when
      checked — not worth the risk; ts-rs is the spec's explicitly-allowed
      alternative). Real commands (`system_check`; `list_default_
      environments`; `get_setting`/`set_setting` backed by `nk-store`)
      proven end-to-end — Rust types -> generated `ui/src/bindings/*.ts`
      -> committed. Found and fixed a real u64-vs-bigint IPC mismatch
      along the way (see ARCHITECTURE.md "Typed IPC")
- [x] Locked-down Tauri capabilities file: reviewed, not just left as the
      scaffold default — `capabilities/default.json`'s `"windows":
      ["main"]` scoping is already sufficient for now (one window, no
      untrusted content source yet). Also hardened `security.csp` from
      `null` (no CSP at all) to a `default-src 'self'` baseline with no
      `frame-src` (so no iframes are possible yet — the correct
      fail-closed state until Phase 4's inscription rendering adds each
      environment's ord origin explicitly). Documented both, and what
      Phase 4 needs to add, in ARCHITECTURE.md "Webview security model"
- [x] Design tokens: per-environment colors (mainnet/regtest/signet/
      testnet4), dark-mode-first with a light override, in
      `ui/src/index.css`
- [x] Shared components: `EnvBanner`, `StatusBadge`, `ErrorPanel`,
      `ConfirmDialog` (mainnet extra step — a required checkbox that
      disables Confirm until checked — plus Learn mode's command display;
      tested), `SensitiveSeedView` (view -> confirm-3-random-words flow;
      tested) — all in `ui/src/components/`
- [x] i18n setup (react-i18next), English locale file, no hard-coded
      user-facing strings from this point on
- [x] Minimal environment switcher (top bar `Select`, backed by the real
      `list_default_environments` command; switching only updates a
      Zustand store, never starts/stops anything)
- [x] First-run disclaimer screen (self-custody warning, practice-on-
      regtest-first, mainnet-is-real; acknowledgement persisted via
      `nk-store`'s settings table through the new `get_setting`/
      `set_setting` commands, shown once)
- [x] System check **screen**: wired to the real `system_check` command,
      human-readable byte formatting

Verification beyond the automated suite: rendered the real app (Tauri
IPC mocked via `@tauri-apps/api/mocks`' `mockIPC` — `ui/src/lib/
dev-tauri-mock.ts`, active only in `npm run dev` outside a real Tauri
context, also useful for future UI iteration) in an actual browser and
confirmed visually: the disclaimer screen, and after acknowledging it,
the main shell with the orange MAINNET banner, environment switcher, and
real system-check data. Confirmed the theme toggle actually flips
`--background` et al. via computed styles (a screenshot-pipeline quirk in
the preview tool kept showing a stale frame after that specific
interaction — verified against `getComputedStyle`/`localStorage` instead
of trusting the screenshot). Did **not** visually verify the new CSP
against the real Tauri webview (only that `tauri build` accepts it) —
the dev-server browser check above doesn't exercise `tauri.conf.json`'s
CSP at all, since that only applies inside the actual Tauri webview.

Storage
- [x] `nk-store`: rusqlite (bundled) + rusqlite_migration, initial schema
      (`settings` table), migration runner with tests (round-trip,
      reopen-is-a-no-op, migration-list validity)

Environment model / path resolution (nk-core)
- [x] Environment struct (name, chain, ports, data_root) for mainnet/
      regtest/signet/testnet4 — `color` is intentionally *not* a Rust
      field: it's purely presentational (`ui/src/index.css`'s
      `--color-env-*` tokens, keyed by chain), so there's nothing to keep
      in sync by duplicating it backend-side
- [x] Per-chain path resolution module (cookie/wallet/wallets/index paths)
      with unit tests for every chain. Corrected a wrong assumption from
      Phase 0 along the way: ord nests its own chain subfolder under
      `--data-dir` even when given explicitly (see DECISIONS.md) — also
      verified bitcoind's signet/testnet4 subfolder names live, since
      Phase 0 had only actually checked regtest
- [x] Windows long-path support: `longPathAware` app manifest (correct
      2016-namespace element verified against Microsoft's own docs, not
      tauri-build's example, which uses the wrong 2005 one; embedded
      manifest confirmed with the Windows SDK's `mt.exe`, not just assumed
      from a successful build) + `nk_core::paths::to_verbatim`
      (`\\?\`-prefixing) for Nodekeeper's own file I/O, since the manifest
      alone needs a machine-wide registry value Nodekeeper can't set for
      the user. Tested with a real >260-character nested path under a
      tempdir: fails without `to_verbatim`, succeeds with it

Process / secrets
- [x] `nk-proc`: single-instance lock file (hostname/PID/timestamp),
      atomic `create_new` acquisition (no check-then-write race), stale-
      lock detection via `sysinfo` (same-host + PID no longer running).
      A lock from a *different* host is never treated as stale (can't
      probe a remote machine's processes) — matches the portable-drive-
      opened-from-two-machines case explicitly. Tests: fresh acquire +
      drop releases; second instance on the same folder refused; stale
      same-host lock cleaned up and re-acquired; different-host lock
      never touched
- [x] `nk-secrets`: OS keychain (`keyring` crate, default features
      already cover Windows/macOS/Linux Secret Service — checked its
      Cargo.toml rather than assuming a feature flag was needed) +
      encrypted secrets-file fallback (Argon2id, 64MiB/3-iter/4-parallel —
      well above OWASP's minimum since this runs once per unlock, not
      per-request; XChaCha20-Poly1305). Tests: round-trip with the
      correct password; wrong password fails; tampered ciphertext fails
      (not just wrong output); fresh salt+nonce every write. No automated
      keychain tests (OS keychain access isn't reliable in headless CI —
      documented in the module, covered by the [MANUAL] checklist
      instead). Used the actual crate source in the local cargo registry
      to get the current `aead`/`chacha20poly1305` `Generate`-trait API
      right (its docs.rs example didn't match this version) rather than
      guessing repeatedly

Acceptance criteria (from docs/SPEC.md Phase 1 "Done when"):
- [x] [CI] app builds and launches on all 3 OSes; the quality gate passes
      — verified both locally on Windows (`just check` + `tauri build
      --debug --no-bundle`) and via the real GitHub Actions run, green on
      windows-latest/macos-latest/ubuntu-latest (see the CI item above).
      "Launches" is CI-verified as "builds and starts" (no GUI on CI
      runners to click through); the full interactive golden path was
      manually verified on Windows only (see "Verification" note above)
- [x] [CI] a deliberate process spawn outside nk-exec/nk-proc fails the
      build (verified once, then removed) — done locally, see DECISIONS.md
- [x] [CI] path resolution tests pass for all chains, incl. a deeply
      nested Windows path — passes locally (11 nk-core tests)
- [x] [CI] a second app instance on the same data folder is refused —
      passes locally (nk-proc lock tests)
- [x] [CI] encrypted secrets file round-trips; wrong master password fails
      — passes locally (nk-secrets tests)
- [x] [MANUAL] banner shows the current environment; disclaimer appears on
      first run only — verified visually (see "Verification" note above)
      with mocked IPC; not yet verified against a real, persisted
      first-launch (that needs the actual Tauri app with its real SQLite
      file, not the browser-based mock)

## Phase 2 — Process manager, executor, installer

In progress. Tasks (one at a time: implement -> test -> quality gate ->
commit -> tick):

`nk-verify` (Bitcoin Core download + verification) — **done**
- [x] Fetch bitcoin-core/guix.sigs builder keys fresh (39 keys; not reused
      from Phase 0's spike copies), bundled into one embedded
      (`include_bytes!`) keyring file, source + fetch date recorded in
      DECISIONS.md
- [x] Download a Bitcoin Core release asset by URL, SHA-256 computed
      incrementally as it streams to disk
- [x] Verify SHA256SUMS contains the expected hash for the downloaded
      file
- [x] Verify SHA256SUMS.asc against the embedded builder keys, requiring
      >=3 valid signatures from *distinct* pinned keys. **STOP AND ASK**:
      sequoia-openpgp's pure-Rust backend (which the spec names
      explicitly) turned out to be flagged "not considered production
      ready" by its own maintainers and wouldn't even compile without an
      experimental opt-in — presented the trade-offs rather than picking
      unilaterally; switched to the `pgp` crate (rpgp) per the user's
      choice. Full reasoning in DECISIONS.md. Also discovered (and
      documented, and handled gracefully rather than crashing on) a real
      rpgp limitation: it can't parse one pinned key's `secp256k1` curve
- [x] Fail closed on every verification error (missing sig, <3 valid
      sigs, hash mismatch, corrupt download, unparseable pinned-key
      bundle) with a clear typed error
- [x] Tests: **a real download of the live Bitcoin Core 31.1 release
      verifies successfully end-to-end** (network test, not mocked) with
      >=3 valid signatures found; a tampered/corrupted file is rejected;
      too few valid signatures is rejected; a filename missing from
      SHA256SUMS is rejected. Also found and fixed a real bug in my own
      first implementation attempt: `pgp`'s `from_armor_many` silently
      parses only the *first* block of a multi-block concatenated file
      (1 of 38 keys, no error) — confirmed empirically with a throwaway
      test before trusting it, not from documentation, since Bitcoin
      Core's own `SHA256SUMS.asc` and this crate's pinned-key bundle are
      both exactly that shape (many single-item blocks concatenated)

`nk-core` (config generation) — **done**
- [x] Generate `bitcoin.conf` per environment: txindex=1, prune=0,
      server=1, RPC bound to 127.0.0.1 only (verified live against
      `bitcoind --help` 31.1 that `rpcbind` alone does nothing without
      `rpcallowip` — both are written), cookie auth (no explicit
      rpcuser/rpcpassword), dbcache sized to available RAM (a quarter of
      what's left after other environments' reservations, clamped to
      Core's accepted range and an 8 GiB ceiling)
- [x] Tests: generated conf has every required line and never writes
      rpcuser/rpcpassword; dbcache sizing scales with available memory
      within bounds and respects a caller-supplied "already reserved by
      other environments" amount

`nk-exec` (central command executor) — **done**
- [x] Argument-array command execution (never shell-interpolated
      strings) — the ONLY place besides nk-proc allowed to spawn
      processes (already enforced by the disallowed-methods check from
      Phase 1)
- [x] Tags every command with environment, source, triggering action
- [x] Redaction: every occurrence of every caller-declared secret string
      scrubbed from the command display and all output before it reaches
      the broadcast event stream
- [x] Sensitive-output channel: `Sensitivity::Sensitive` commands' real
      output goes only to `execute()`'s direct return value; the
      broadcast stream gets `[sensitive output hidden]` instead, never
      the real content even redacted (full wiring to a seed-view UI is
      Phase 5; this phase built the mechanism itself and proved it)
- [x] Zeroizes the stdin secret buffer immediately after writing it to
      the child
- [x] Streams structured `ExecEvent`s (Started/Output/Finished, TS-
      exported) via a `tokio::sync::broadcast` channel — the Live Command
      Monitor UI (Phase 3) will subscribe to this; Phase 2 only needed
      the producer side, proven with a direct subscriber in tests
- [x] Tests (12, all against a real spawned child process, not mocked):
      stdout/exit-code capture; redaction reaches the broadcast stream
      but not the direct return value; sensitive output never reaches
      the broadcast stream even as a placeholder-wrapped real value;
      stdin-passed secrets never appear in the command display; a
      command that never gets stdin input doesn't hang waiting for it

`nk-rpc` (Bitcoin Core JSON-RPC client) — **done**
- [x] Minimal JSON-RPC 1.0 client over HTTP with cookie auth (parses the
      real `__cookie__:<password>` file format), calls routed through
      `nk-exec`'s new `record()` (tagged as RPC, shown as the equivalent
      bitcoin-cli command per docs/SPEC.md item 7 — e.g. `bitcoin-cli
      -regtest generatetoaddress 1 <address>`, with the chain flag
      correctly omitted for mainnet)
- [x] Enough methods for Phase 2: `stop`, `generate_to_address`,
      `get_new_address`, `get_blockchain_info`, plus a raw `call()` escape
      hatch for anything not wrapped yet
- [x] Extended `nk-exec::Executor` with `record()`: the same tagged/
      redacted event-stream treatment as a spawned command, for
      operations (like an RPC call) that don't spawn a child process.
      Bundled its parameters into a `RecordSpec` struct after clippy
      flagged the first version for too many function arguments

`nk-proc` (process manager) — **done**
- [x] Spawns bitcoind as a tracked child process (long-lived daemon
      lifecycle is nk-proc's own direct responsibility, distinct from
      nk-exec's short-lived one-shot commands/RPC calls — see
      ARCHITECTURE.md); passes only `-datadir` and the chain flag, since
      everything else comes from the generated `bitcoin.conf`
- [x] Detects an already-running bitcoind on the same data directory via
      its own `bitcoind.pid` file (confirmed live: plain numeric PID)
      plus a liveness check, refusing to start a second one. **Narrower
      than the spec's full "offer to attach" flow**: that needs a UI
      decision (attach vs. refuse) that doesn't exist before Phase 3, and
      the spec's "cookie authenticates" check is naturally an RPC call
      (now available via nk-rpc) — Phase 2 built the detection primitive
      the later UI flow will call into, not the full attach decision
- [x] Graceful stop: RPC `stop` (via nk-rpc), then wait for the process
      to exit with a caller-supplied timeout; a force-kill path
      (`kill_sync`) exists separately for cleanup-only use (e.g. test
      teardown after a panic), never as the primary stop path
- [x] Tests: a pre-existing (live) bitcoind is detected; a dead process's
      leftover pid file is correctly *not* detected as running

`nk-testkit` — **done**
- [x] Starts real bitcoind in a temp dir on random free ports, can mine
      blocks (creating a wallet first if needed), always tears down —
      `Drop` force-kills if `stop()` was never called, proven with a test
      that checks the OS process list after an un-stopped fixture drops
- [x] Real regtest start + mine 101 blocks + stop cleanly integration
      test — the literal Phase 2 [CI] acceptance criterion, gated behind
      an `NK_TEST_BITCOIND` env var (path to a real bitcoind) so
      `cargo test` doesn't require a pre-staged binary on every dev
      machine; skips with a clear message when unset rather than failing

CI
- [x] `crates/nk-verify/examples/fetch_bitcoin_core.rs`: a CI helper that
      calls nk-verify's *real* download-and-verify path (not a bash
      reimplementation) for the current platform, extracts `bitcoind`,
      and sets `NK_TEST_BITCOIND` — wired into `ci.yml` before `cargo
      test`, with an `actions/cache` step (keyed by OS) so most runs skip
      the network transfer entirely (the helper checks for an already-
      extracted binary from a prior run and skips re-verifying if found)
- [x] Found and fixed a real bug via the first actual CI run (not
      inspection): the helper printed a path relative to the repo root
      (its own cwd when the CI step runs it), but `cargo test` runs each
      crate's test binary with cwd set to *that crate's own* manifest
      directory — so `nk-testkit`'s tests resolved the relative
      `NK_TEST_BITCOIND` path incorrectly and bitcoind failed to spawn
      (`NotFound`) on all 3 OSes. Fixed by canonicalizing to an absolute
      path before printing; used `dunce::canonicalize` rather than
      `std::fs::canonicalize` to avoid reintroducing the `\\?\`-verbatim-
      path problem already solved once in `nk-core::system_check`. See
      DECISIONS.md

Acceptance criteria (from docs/SPEC.md Phase 2 "Done when"):
- [x] [CI] real Bitcoin Core binaries download and verify with 3+ pinned
      signatures; a tampered file is rejected — passes locally against
      the live bitcoincore.org release (7-11 valid signatures found
      depending on run) **and confirmed by a real GitHub Actions run,
      green on all 3 OSes**: run 35795672861
- [x] [CI] regtest bitcoind starts, mines 101 blocks, and stops cleanly —
      passes locally with a real bitcoind **and confirmed in the same CI
      run**, with `cargo test` actually executing (not skipping)
      `nk-testkit`'s real regtest integration test on all 3 OSes
- [x] [CI] redaction unit tests pass; a pre-existing bitcoind is detected
      — both pass locally and in CI
- [ ] [MANUAL] mainnet bitcoind starts, connects to peers, and stops
      cleanly (no full sync required) — a helper now exists so this
      exercises Nodekeeper's real conf generation + process manager
      instead of ad hoc flags: `cargo run -p nk-testkit --example
      mainnet_smoke_test -- <path-to-verified-bitcoind>` (get a verified
      binary first via `cargo run -p nk-verify --example
      fetch_bitcoin_core`). Still needs you to actually run it and
      confirm it prints "Smoke test passed."
- [ ] [MANUAL] I have checked every pinned builder-key fingerprint
      against bitcoin-core/guix.sigs from a separate machine or browser
- [x] Security self-review completed (go through SECURITY RULES line by
      line, point to the code/test enforcing each, list any gaps) — see
      DECISIONS.md "Phase 2 — security self-review"

## Phase 3 — Dashboard and monitor

In progress. Tasks (one at a time: implement -> test -> quality gate ->
commit -> tick):

Backend — monitor history (docs/SPEC.md item 7)
- [x] `nk-store`: `command_history` migration + insert/query/export,
      capped per environment (spec: "e.g. last 5,000 entries"), tested
      (round-trip, cap/pruning, reopen keeps data)
- [x] Wire `nk-exec`'s `ExecEvent` broadcast into `nk-store`: a
      subscriber task that persists Started/Output/Finished rows —
      safe by construction since the broadcast stream is already
      redacted/placeholder'd before this layer ever sees it (Phase 2).
      `nk_store::persist_exec_events`, tested against a real spawned
      `Executor` (full lifecycle persisted; a `Sensitivity::Sensitive`
      command's real output never reaches history, only the
      placeholder; the task exits once the `Executor` is dropped)
- [x] Mark background-polling events so the UI can hide them by default
      (spec: "Background polling is hidden by default with a Show
      background polling toggle") — `background: bool` threaded through
      `CommandSpec`/`RecordSpec`/`ExecEvent::Started`/`command_history`;
      the caller decides per call, since the same RPC method (e.g.
      `getblockchaininfo`) is a meaningful one-off check in one context
      and polling noise in another. Tagged `true`: the Dashboard's
      3-second status poll (`NodeManager::status`) and
      `start_and_wait_ready`'s internal readiness-poll loop. Tested that
      the flag survives end to end (`nk-exec` -> `nk-store`)

Backend — plain-language errors (item 8, [CI] "a busy port produces the
friendly error")
- [x] Typed error-code enum (PORT_IN_USE, DISK_FULL, INDEX_BEHIND,
      INDEX_OPTION_DISABLED, WALLET_LOCKED, RPC_WARMING_UP,
      ORD_NOT_SYNCED, BINARY_NOT_VERIFIED), TS-exported —
      `nk_core::AppErrorCode`; a code carries no message text itself,
      the frontend owns code -> i18n mapping (next task). Serde's
      `SCREAMING_SNAKE_CASE` wire format pinned by a test against the
      spec's exact literal names rather than trusted blindly
- [x] `nk-proc::BitcoindProcess::start` distinguishes "port already in
      use" from other spawn failures and returns the typed error — a
      pre-flight bind check on the RPC/P2P ports *before* spawning
      (deterministic, not a race against bitcoind's own stderr);
      tested by binding the target port first with a nonexistent
      binary path and asserting `PortInUse` comes back before any
      spawn attempt
- [x] Frontend: i18n message + "What to do" action per error code,
      wired into the existing `ErrorPanel` (Phase 1) — `lib/error-
      messages.ts`, used by `DashboardScreen`

Backend — dashboard status
- [x] `nk-rpc`: peer count (`getnetworkinfo`) and mempool
      (`getmempoolinfo`) methods, alongside the existing
      `get_blockchain_info` — field names VERIFY'd live against a real
      regtest node (see DECISIONS.md), not assumed. Real-node coverage
      via a new `nk-testkit` test (`dashboard_rpc_methods_return_the_
      expected_fields`), run locally against the cached Bitcoin Core
      31.1 binary, not just compiled
- [x] `nk-core`: disk-usage helper (used-by-data-dir + free-on-volume),
      reusing `sysinfo` the same way `system_check` already does —
      `nk_core::disk::disk_usage_for`, tested (missing dir reports 0,
      not an error; recursive sum across nested dirs; real free-space
      lookup on the containing volume)
- [x] `nk-proc`: track each running process's start time for uptime —
      `BitcoindProcess::started_at`
- [x] A status aggregator (src-tauri command, composing the above) —
      integration-tested via nk-testkit against a real regtest node —
      `NodeManager::status()`, tested end-to-end
      (`starts_reports_status_and_stops_a_real_node`)

Backend — log viewer (item 2: "never load a whole file")
- [x] `nk-core` (or a small new module): tail-from-end file reader
      (seek, not full read), plus search/filter over the tailed window;
      tested against files larger than the tail window —
      `nk_core::log_tail` (`tail`, `page_before` for scrolling further
      back, `search`). Tested: small file returns everything; a large
      file returns only complete newest lines (no truncated line ever
      leaks through); repeated `page_before` calls from a `tail()`
      reconstruct the entire original file byte-for-byte with no gaps
      or overlap; search respects a match cap; a missing file errors
      rather than panicking

Backend — process manager wiring for the dashboard
- [x] src-tauri app state: running processes per environment + one
      shared `Executor` instance whose broadcast stream feeds both the
      history persistence above and the frontend event bridge below —
      `node_manager::NodeManager` (its own module, not `#[tauri::
      command]` bodies, so it's testable without the GUI per CLAUDE.md);
      the shared `Executor` and `Arc<Mutex<Store>>` are `.manage()`d
      once in `run()`. **Scoping note**: `start()` takes an
      already-known `binary_path` rather than locating/downloading one
      itself — there is no real "install Bitcoin Core to a persistent
      location" flow yet (only the CI/dev helper and test fixtures
      exercise download+verify+extract today), that's the setup
      wizard's job (item 1), not built as a UI flow yet. For now the
      path is read from the existing settings table (`bitcoind_path`);
      unset -> `AppErrorCode::BinaryNotVerified`. Set it manually via
      `set_setting` (e.g. to the path `fetch_bitcoin_core` prints) to
      exercise start/stop for real until the wizard exists
- [x] Tauri commands: start/stop/restart per environment, status query
      — `start_node`/`stop_node`/`restart_node`/`node_status`, plus
      `is_node_running` (cheap "is it running" check so the frontend
      doesn't have to treat "not started yet" as a `node_status` error).
      Real end-to-end test (`starts_reports_status_and_stops_a_real_
      node`) against a real regtest bitcoind: start refuses a second
      concurrent start for the same chain, status reports real
      blocks/peers, stop actually stops it, and status afterward
      correctly reports `NotRunning`, not stale data
- [x] Background task persisting `Executor` events into `command_history`
      — spawned once in `run()`'s `.setup()` hook via `nk_store::
      persist_exec_events`
- [x] Background task re-emitting `ExecEvent`s as Tauri events
      (`app_handle.emit("exec-event", event)`) for the (still to be
      built) Live Command Monitor UI to `listen()` for — a second task
      subscribed to the same shared `Executor`, spawned alongside the
      history-persistence one. Correctly distinguishes a lagged
      receiver (keep forwarding what arrives next) from a closed one
      (stop) rather than silently dying on the first missed burst of
      events, the same mistake the history bridge already avoided

Frontend — Live Command Monitor (item 7)
- [x] Resizable bottom drawer: show/hide (top-bar toggle + keyboard
      shortcut), remembered size/state, pulsing activity indicator when
      hidden and commands run — `LiveCommandMonitor`/`store/monitor.ts`.
      Show/hide state and height persisted via `get_setting`/
      `set_setting` (`monitor_visible`/`monitor_height_px`); shortcut is
      Ctrl/Cmd+\` (backtick); drag the top edge to resize
- [x] Entry list: timestamp, environment tag (text only — see note
      below), source, full command, status, duration, exit code,
      expandable live output — hydrated from `list_command_history` on
      open, then kept live via the `exec-event` Tauri event bridge
      (Started/Output/Finished merged by command id)
- [x] Filters: environment, source, status, text search, background
      polling — all real and working now. The Dashboard's 3-second
      status poll and `start_and_wait_ready`'s internal readiness loop
      are both tagged `background: true` end to end (backend task
      above) and hidden by default, matching the spec; the toggle
      reveals them. `filteredEntries` also excludes background entries
      from the "activity while hidden" pulse, for the same reason
      they're hidden from the list — a running node would otherwise
      make the indicator pulse almost constantly
- [x] Per-entry: copy command, copy output — both wired to the
      clipboard. **"Open in console" intentionally omitted**: it's
      meant to pre-fill a console tab, and there's no console to
      pre-fill until Phase 7 builds one; a disabled placeholder button
      felt more like a half-finished stub than an honest gap, so it's
      just not there yet
- [x] Panel controls: pause/resume auto-scroll, clear view, export to
      text file — export uses a client-side Blob download (no backend
      involved, nothing to redact beyond what's already redacted)
- [ ] Pop-out into its own window — **deferred, not attempted**: needs
      a routing split (a window-type flag so a popped-out window renders
      only the monitor, not the full app shell) and a Tauri capabilities
      change (window creation) that haven't been verified against Tauri
      2's actual multi-window API. Scoped out rather than rushed
- [ ] "Learn mode": `ConfirmDialog` already supports it (`learnMode`/
      `command` props, Phase 1) but nothing calls `ConfirmDialog` with a
      real command yet — no fund-moving action exists until Phase 5+.
      Nothing to wire this into yet, not a missed step

Environment tag color: shown as plain text, not color-coded per chain.
An entry's `environment` field is a free-text label (users can rename
an environment, e.g. "Dev Box" — Foundation A), not a `Chain` value, so
there's no reliable way to map it back to `chainBgClass` without
threading the actual `Chain` through the event/history schema, which
isn't there today. Showing a wrong color would be worse than showing
no color.

Verified in the browser (dev server, mocked IPC): opened the drawer via
the toggle, all filter controls render and respond, "No commands yet"
shows correctly (mocked history is empty), pause/resume auto-scroll
toggles its own label. The live `exec-event` subscription itself can't
be exercised in the browser preview (no real Tauri backend) — it fails
closed with a caught, logged warning (`dev-tauri-mock: no mock for IPC
command "plugin:event|listen"`) rather than crashing the panel; actual
live-event behavior needs a real Tauri app run to verify, not done yet.

Frontend — Dashboard (item 2)
- [x] Bitcoin Core panel: block height vs header height, verification
      progress %, peers, mempool, disk used, uptime —
      `DashboardScreen`/`useDashboardStatus`, polls `node_status` every
      3s. **ETA is not shown**: `NodeStatus` has no field for it and
      computing one needs a sync-rate history this phase doesn't track
      — a real gap, not an oversight, left for whenever that's built
      rather than faked with a made-up number. The spec's ord section
      (index height vs node height) is also absent: nothing backs it
      until Phase 4
- [x] Status badges with text labels (Syncing / Indexing / Ready) — only
      Stopped/Starting/Syncing/Ready exist for now (no "Indexing" state
      since ord isn't wired up)
- [x] Start / stop / restart controls wired to the process manager —
      plain buttons, not gated behind `ConfirmDialog`'s mainnet step
      (that step is scoped to fund-moving actions per the spec; start/
      stop isn't one)
- [x] Log viewer: tail + page, search/filter, never loads a whole file
      — `LogViewer`, backed by the real `tail_debug_log`/
      `page_debug_log_before`/`search_debug_log` commands
- [x] Disk monitor: projected usage including the (future) ord index,
      warns well before free space gets low — `DiskMonitor`, fixed 5
      GiB warning threshold (not yet configurable per environment)

Verified in the browser (dev server, mocked IPC — see `dev-tauri-mock.ts`):
disclaimer -> system check -> environment switch to Regtest ->
Dashboard; clicked Start, watched Syncing -> Ready with live-updating
block height/peers/mempool/uptime; dark theme toggle re-themes the
whole screen; log search narrows correctly to matching lines; no
console errors.

Acceptance criteria (from docs/SPEC.md Phase 3 "Done when"):
- [x] [CI] every command from Phase 2 is recorded by the monitor
      backend; secrets are redacted in records and exports — recording
      is CI-tested (`nk_store::history_bridge`'s tests prove a full
      command lifecycle is persisted, and that a `Sensitivity::
      Sensitive` command's real output never reaches `command_history`,
      only the placeholder). Exports (`LiveCommandMonitor`'s export
      button) have no separate redaction test of their own — none is
      needed, since the export path is a trivial formatter over
      `MonitorEntry` data that's already redacted by the time it
      reaches the frontend (nk-exec redacts before broadcasting; the
      history table only ever stores already-redacted content); there's
      no new redaction logic in the export step itself to verify
- [x] [CI] a busy port produces the friendly error —
      `nk-proc::bitcoind::tests::starting_with_a_busy_rpc_port_fails_
      with_the_friendly_error`, part of every `cargo test --workspace`
      run, confirmed on all 3 OSes
- [ ] [MANUAL] a large debug.log opens instantly; show/hide and pop-out
      work

## Phase 4 — ord integration

In progress. Tasks (one at a time: implement -> test -> quality gate ->
commit -> tick). VERIFY'd live before starting (DECISIONS.md): ord
0.29.0 is still current, its CLI surface (chain/index-option/path/
server flags), and its `/status` sync-API (needs `Accept: application/
json`, returns ord's own indexed height — "caught up" means comparing
that against `getblockchaininfo`'s `blocks` via nk-rpc, ord doesn't do
that comparison itself).

`nk-verify` (ord download + verification, pinned hashes) — **done**
- [x] Pin ord 0.29.0's per-platform SHA-256 hashes (from DECISIONS.md,
      independently computed, not just GitHub's reported digest);
      refuse any unpinned version rather than skipping verification
- [x] `download_and_verify_ord_asset`, mirroring `nk-verify::bitcoin_
      core`'s shape (no builder-key/PGP step — ord has no maintainer
      checksums file, so the pinned hash *is* the verification per
      docs/SPEC.md item 1). Checks the pin *before* downloading, so an
      unpinned version never wastes bandwidth
- [x] Tests: a real download of the live ord 0.29.0 release verifies
      successfully (network test, not mocked); a tampered/corrupted
      file is rejected; an unpinned version is refused, proven to
      happen *before* any network call (points at a nonexistent host —
      the test would fail with a network error instead of
      `UnpinnedVersion` if the check happened after downloading)

`nk-core` (ord paths, CLI argument generation, index options) — **done**
- [x] Per-environment index-option settings (index-sats, index-runes,
      index-addresses) — `Environment::index_options: IndexOptions`,
      defaulted per chain via `Chain::default_index_options()` (only
      Regtest defaults to all-on, per Foundation F). **Not yet actually
      persisted** — same pre-existing gap as every other `Environment`
      field (no environment-persistence system exists at all yet, not
      something new to this task); wired up now so the setup wizard has
      something real to write to once it exists
- [x] ord CLI argument-array generation: chain flag (`Chain::
      ord_cli_flag()`, double-dash, VERIFY'd distinct from bitcoind's
      single-dash convention and from ord's separate legacy `--testnet`
      flag), `--data-dir`, `--cookie-file`, `--bitcoin-data-dir`
      (pointing at Nodekeeper's own bitcoind, not ord's default
      `~/.bitcoin`), the enabled `--index-*` flags, `server --address
      127.0.0.1 --http --http-port <port>` —
      `nk_core::ord_conf::{ord_base_args, ord_server_args}`, tested that
      the RPC-bind-only-to-localhost rule from Foundation D applies here
      exactly like bitcoind's `rpcbind` (asserts the wildcard `0.0.0.0`
      never appears)

`nk-proc` (ord process manager)
- [ ] `OrdProcess::start()`: spawn, track pid file (mirroring
      `BitcoindProcess`), refuse a second instance on the same data dir
- [ ] Graceful stop: SIGINT on macOS/Linux, `CREATE_NEW_PROCESS_GROUP` +
      `CTRL_BREAK_EVENT` on Windows — **real CI test on all 3 OSes**,
      finally closing the gap Phase 0 could only verify on Windows
      ("needs a CI job — no such machine available in this session")
- [ ] Wait-for-sync: poll ord's `/status`, compare `height` against the
      node's `getblockchaininfo.blocks` via nk-rpc, until caught up (or
      a timeout) — real regtest integration test via nk-testkit

`nk-rpc` (or a small new module) — ord's HTTP JSON API
- [ ] A typed client for `GET /status` (`Accept: application/json`)

Frontend — Dashboard: ord section (item 2, previously omitted with a
note in Phase 3 since nothing backed it yet)
- [ ] Index height vs node height, indexing/caught-up status, which
      index options are enabled — extends `DashboardScreen`
- [ ] Start/stop wired to `OrdProcess` the same way bitcoind's controls
      already are

Acceptance criteria (from docs/SPEC.md Phase 4 "Done when"):
- [ ] [CI] ord verifies; an unpinned version is refused
- [ ] [CI] ord indexes regtest with all index options and stays caught up
- [ ] [CI] ord stops gracefully on Windows, macOS, and Linux and
      restarts without reindexing
- [ ] [MANUAL] ord server is not reachable from another machine on the LAN
- [ ] [MANUAL] I have checked every pinned ord SHA-256 hash against the
      official ord release from a separate machine or browser
- [ ] VERIFY results for Foundation F recorded in DECISIONS.md (mostly
      done in Phase 0's spike; revisit once index-option settings are
      actually wired to real feature gating, not just spike commands)

## Phase 5 — Wallet

Not started. See docs/SPEC.md Phase 5.

## Phase 6 — Inscribe studio

Not started. See docs/SPEC.md Phase 6.

## Phase 7 — Console, scripts, explorer (MVP complete)

Not started. See docs/SPEC.md Phase 7.

## Phase 8 — Multi-environment UI and Test Lab

Not started. See docs/SPEC.md Phase 8.

## Phase 9 — Portable mode

Not started. See docs/SPEC.md Phase 9.

## Phase 10 — Extras and release

Not started. See docs/SPEC.md Phase 10.
