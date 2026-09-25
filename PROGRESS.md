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
- [x] `OrdProcess::start()`: spawn, track pid file (mirroring
      `BitcoindProcess`, but ord writes none of its own — Nodekeeper
      writes and reads `Environment::ord_pid_path`), refuse a second
      instance on the same data dir. Plus `start_and_wait_ready()`
      (spawn + bounded wait for `/status` to respond, the ord
      equivalent of `BitcoindProcess`'s cookie/RPC-ready wait).
- [x] Graceful stop: `libc::kill(pid, SIGINT)` on macOS/Linux, a
      hand-rolled `GenerateConsoleCtrlEvent(CTRL_BREAK_EVENT, pid)` FFI
      binding against a `CREATE_NEW_PROCESS_GROUP` child on Windows.
      **Verified live on this Windows machine** via a real
      `nk-testkit` integration test (real bitcoind + real ord,
      graceful stop actually terminates the process, confirmed via
      `detect_running_ord`) — closing the gap Phase 0 could only note
      ("needs a CI job — no such machine available in this session").
      Linux/macOS coverage now runs via the same test in CI (added an
      ord fetch+cache CI step mirroring Bitcoin Core's); cross-platform
      confirmation lands once this commit's CI run completes.
- [x] Wait-for-sync: `wait_until_caught_up(ord, bitcoin_rpc, timeout)` —
      a free function, not an `OrdProcess` method (only needs the two
      already-running clients; the caller decides how long is
      acceptable, since a bounded regtest wait and a real mainnet
      catch-up are very different timescales). Polls both `/status` and
      `getblockchaininfo` until `height`/`blocks` match or the timeout
      elapses (`AppErrorCode::OrdNotSynced` on timeout — distinct from
      `start_and_wait_ready`'s `StartupTimeout`, which only means "HTTP
      server not answering yet"). Real regtest integration test in
      `nk-testkit` (extends `ord_starts_indexes_regtest_and_stops_
      gracefully`): mines 5 blocks, starts ord, waits for it to catch
      up, asserts `/status`'s `height` equals 5 — **ran live on this
      Windows machine, passed**.

`nk-ord` (new crate) — ord's HTTP JSON API — **done**
- [x] A client for `GET /status` (`Accept: application/json`) —
      `OrdClient::status()`, mirroring `nk-rpc::RpcClient::call()`'s
      shape exactly: routed through the central `Executor` (shown in the
      Live Command Monitor as the equivalent `curl` command), returns
      the raw `serde_json::Value` rather than a strongly-typed struct —
      same convention `nk-rpc`'s typed methods use for bitcoind's
      responses, so callers pick out only the fields they need (e.g.
      `nk-proc`'s future wait-for-sync loop reading just `height`).
      Added `CommandSource::OrdApi` (nk-exec) so this shows up as its
      own Live Command Monitor filter, distinct from actual `OrdCli`
      subprocess invocations — same reasoning as `Rpc` being separate
      from `BitcoinCli`. Unit-tested the pure URL-building logic only
      (no network) — same layering as `nk-rpc` (whose own tests are
      network-free; live-server verification lives in `nk-testkit`).
      The live integration test against a real running ord server now
      exists in `nk-testkit`, built alongside `nk-proc`'s `OrdProcess`
      below (needed it to actually stand a real ord server up).

Frontend — Dashboard: ord section (item 2, previously omitted with a
note in Phase 3 since nothing backed it yet) — **done**
- [x] Index height vs node height, indexing/caught-up status, which
      index options are enabled — `OrdSection` + `useOrdStatus`,
      extends `DashboardScreen`. Status badge uses item 2's exact
      wording ("Indexing", distinct from bitcoind's "Syncing").
      Index-options list reads ord's own `/status` report, not just
      what Nodekeeper configured. Verified live in the browser (dev
      preview, mocked IPC) for both an all-options-off and an
      all-options-on environment.
- [x] Start/stop/restart wired to `OrdProcess` via `NodeManager`
      (`start_ord`/`stop_ord`/`restart_ord`/`ord_status`/
      `is_ord_running`, mirroring bitcoind's own Tauri commands
      exactly) — real end-to-end test
      (`starts_ord_reports_status_and_stops_it`), ran live against a
      real bitcoind + ord on this Windows machine, passed.

Acceptance criteria (from docs/SPEC.md Phase 4 "Done when"):
- [x] [CI] ord verifies; an unpinned version is refused — `nk-verify`,
      confirmed in CI before this session's later GitHub Actions
      billing interruption (see below)
- [x] [CI] ord indexes regtest with all index options and stays caught
      up — `wait_until_caught_up` + `ord_status`, both with real
      integration tests, **run live on this Windows machine** (not
      yet re-confirmed by a fresh CI run — see note below)
- [x] [CI] ord stops gracefully on Windows, macOS, and Linux and
      restarts without reindexing — graceful stop confirmed live on
      Windows and (via CI, before the billing interruption) on
      Linux/macOS. "Restarts without reindexing" now has a real test,
      `ord_restarts_from_its_persisted_index_without_reindexing`
      (`nk-testkit`): stop, mine more blocks, restart against the same
      data dir, assert the *first* post-restart `/status` already shows
      the pre-stop height rather than 0 — **run live on this Windows
      machine, passed** (not yet re-confirmed by a fresh CI run on all
      3 OSes — see note below).
- [ ] [MANUAL] ord server is not reachable from another machine on the LAN
- [ ] [MANUAL] I have checked every pinned ord SHA-256 hash against the
      official ord release from a separate machine or browser
- [ ] VERIFY results for Foundation F recorded in DECISIONS.md (mostly
      done in Phase 0's spike; revisit once index-option settings are
      actually wired to real feature gating, not just spike commands)

**Note (2026-09-24):** GitHub Actions CI on this repo is currently
blocked — a run triggered mid-Phase-4 failed immediately on all 3 OSes
with "recent account payments have failed or your spending limit needs
to be increased," not a code issue. The last CI run that actually
completed (`nk-ord`'s `/status` client, and the `OrdProcess` commit's
Linux/macOS jobs) was green. Everything built afterward
(`wait_until_caught_up`, the `NodeManager`/Dashboard ord wiring) has
only been verified by running the real test suite locally on Windows
(`cargo test --workspace` with `NK_TEST_BITCOIND`/`NK_TEST_ORD` set to
real, freshly-verified binaries) — genuinely real verification, just
not yet cross-platform-confirmed by CI. Re-run CI once billing is
resolved to close out the remaining `[CI]` acceptance criteria above.

## Phase 5 — Wallet

In progress. Tasks (one at a time: implement -> test -> quality gate ->
commit -> tick). VERIFY'd live before starting (DECISIONS.md): ord's
`wallet` CLI surface (`--server-url`/`--name` are flags *on* `wallet`,
not top-level; `create`/`restore --from mnemonic` stdin syntax;
`send --dry-run` needs no unlock), and the encrypted-Core-wallet
interaction end to end against a real regtest wallet (works cleanly,
standard `walletpassphrase`/`walletlock` RPCs, no ord-specific
handling or workaround needed -- the spec's STOP-AND-ASK trigger for
this did not fire).

`nk-rpc` (wallet-unlock RPCs, with real redaction -- a gap found while
planning this phase) — **done**
- [x] Added a `redact: Vec<String>` parameter to `RpcClient::call()`
      (breaking change, every existing call site updated to pass
      `vec![]`) plus `wallet_passphrase`/`wallet_lock`/`encrypt_wallet`
      typed wrappers, each targeting `/wallet/<name>` (a new private
      `call_at`/`wallet_call` split keeps `call()` and the wallet
      wrappers from duplicating the record/broadcast plumbing) and
      passing the real passphrase through `redact` so it's scrubbed
      from `command_display` before broadcast.
- [x] Tests: two unit tests confirm a fake passphrase never appears in
      the broadcast `command_display` for `wallet_passphrase`/
      `encrypt_wallet` (network-free, same layering as `nk-rpc`'s other
      tests) -- the real, automatable slice of the Phase 5 [CI]
      "fake-passphrase search test" acceptance criterion (the
      fake-mnemonic half lives with the sensitive-channel work below).
      Plus a real `nk-testkit` integration test,
      `wallet_unlock_and_lock_gate_a_real_signing_rpc`: encrypts a real
      wallet, confirms a signing RPC fails cleanly while locked,
      `wallet_passphrase` unlocks it and the same RPC succeeds,
      `wallet_lock` re-locks it and the RPC fails again -- **ran live
      on this Windows machine, passed**.

`nk-ord` (wallet CLI wrapper) — **done**
- [x] `WalletTarget` (`environment`/`cookie_path`/`bitcoin_datadir`/
      `server_url`/`wallet_name`) + `base_args()`, mirroring
      `ord_conf`'s shape. `--server-url`/`--name` placement (on
      `wallet`, not top-level) pinned by a unit test after getting it
      wrong live while VERIFYing.
- [x] `create_wallet`/`restore_wallet` (stdin mnemonic,
      `Sensitivity::Sensitive` -- the mnemonic never reaches the
      broadcast/history path, only the direct `ExecOutcome` return
      value). `restore_wallet` takes `timestamp` as a caller-supplied
      parameter (`"now"`/unix-ts/`"0"`), not hardcoded.
- [x] `wallet_balance`/`wallet_receive`/`wallet_addresses`/
      `wallet_inscriptions`/`wallet_transactions`/`wallet_cardinals`
      (all `Sensitivity::Normal`).
- [x] `wallet_send` (dry-run and real, `Sensitivity::Normal`).
- [x] Real test, `wallet_cli_create_fund_send_and_restore`
      (`nk-testkit`): create -> mine -> wait for ord to catch up ->
      balance -> dry-run send -> encrypt -> locked real send fails ->
      unlock -> real send succeeds -> lock -> restore from the same
      mnemonic under a different name (full rescan) -> restored
      balance matches. **Ran live on this Windows machine, passed.**
      Along the way, found and documented (DECISIONS.md) a real ord
      behavior: every `ord wallet` subcommand refuses to run while
      ord's index is behind bitcoind, exactly matching docs/SPEC.md
      item 3's "Until ord is caught up, show... instead of errors"
      warning -- the real app must gate wallet actions on
      `wait_until_caught_up`/`ord_status().caught_up`, not just call
      through and handle the error.

Foundation D (webview security model, CSP)
- [x] `frame-src` CSP directive scoped to the 4 fixed default
      `http://127.0.0.1:<ord-port>` origins (8080/8081/8082/8083 --
      `Chain::default_ord_port`), no wildcard (`tauri.conf.json`).
      `cargo build -p nodekeeper` confirms it's valid config; real
      enforcement isn't testable until the iframe component below
      exists to load something into.
- [x] Sandboxed inscription-preview component: `InscriptionGallery`'s
      `<iframe sandbox="allow-scripts">` (no `allow-same-origin`, and
      also no `allow-forms`/`allow-popups`/top-navigation -- omitted
      entirely, not just left off by accident), `src` built from the
      environment's own `ord_port` pointed at `/preview/<id>` (not
      `/content/<id>` -- DECISIONS.md Phase 5 VERIFY explains why).
- [ ] [CI] acceptance criterion: a malicious test HTML/SVG inscription
      cannot call Tauri IPC or read app data. **Not yet verified against
      the real thing**: the sandbox attributes are in place and
      reasoned through in DECISIONS.md (opaque iframe origin, no Tauri
      IPC surface reachable from inside an iframe regardless, ord's own
      CORS/CSP headers), but that's code-level reasoning, not a live
      test -- doing so needs the real Tauri webview (the browser
      dev-preview has no real Tauri IPC to try calling at all, so it
      can't exercise this). Tracked as part of the Phase 5 security
      self-review below, not skipped.

`SensitiveSeedView` (frontend) + mnemonic display Tauri command
- [x] `create_wallet`/`restore_wallet` Tauri commands (`src-tauri/src/
      lib.rs`): a shared `wallet_context` helper checks both bitcoind
      and ord are running for the chain first (a clear error instead of
      a raw connection failure), then delegates to `nk_ord::wallet`.
      `CreateWalletResult { mnemonic }` is a dedicated TS-exported
      response type, returned directly in the IPC response and never
      threaded through anything else. Wallet name is hardcoded to
      `"ord"` for now (`DEFAULT_WALLET_NAME`) -- "Multiple named
      wallets" is a separate, later task. Added `WalletError::code()`
      (`nk-ord`) mapping the two real failure texts found live
      (locked-wallet, ord-behind-bitcoind) to `AppErrorCode::
      WalletLocked`/`OrdNotSynced`.
- [x] Full-screen view: already existed, built as Phase 1's skeleton
      (`SensitiveSeedView.tsx`, `feat: shared components...` commit) --
      screenshot warning, mnemonic shown once (numbered word grid),
      then a confirm step quizzing 3 random word positions, "Confirm"
      disabled until all 3 are typed correctly. Takes `words: string[]`
      + `onDone`; the future "Wallet screen" wiring passes
      `create_wallet`'s `mnemonic` split on whitespace. Re-verified
      this session (re-read the file properly after an initial mistake
      where I overwrote it without reading first, caught by noticing
      `git status` showed a *modified* file rather than a new one --
      reverted with `git checkout <commit> --`, restoring the original
      untouched) that it already fully satisfies this task; nothing
      left to build here.
- [x] Test: the fake-mnemonic half of the search-test acceptance
      criterion -- `create_and_restore_wallet_never_leak_the_mnemonic_
      to_the_broadcast_stream` (`nk-testkit`): neither a real
      ord-generated mnemonic nor a known fake one (a standard BIP39
      test vector) ever appears in any broadcast `ExecEvent`. **Ran
      live on this Windows machine, passed.**

Wallet screen (frontend + backend orchestration)
- [x] `wallet_exists(chain)` Tauri command + `nk_ord::wallet::
      wallet_exists`, so the screen knows whether to show create/
      restore or the wallet itself. VERIFY'd live first (DECISIONS.md):
      `ord wallet` commands transparently reload an existing on-disk
      wallet after a full bitcoind/ord restart with no explicit reload
      logic needed anywhere in Nodekeeper -- a nonexistent wallet fails
      with a distinguishable "Path does not exist" error, treated as
      `false` rather than propagated. Real test,
      `wallet_exists_reflects_whether_create_has_run` (`nk-testkit`):
      false before `create_wallet`, true immediately after. **Ran live
      on this Windows machine, passed.**
- [x] Backend half done: `WalletSession` (`src-tauri/src/
      wallet_session.rs`) -- in-memory-only per-chain remembered
      passphrase, 15-min idle timeout, never persisted, zeroized on
      expiry/forget (4 unit tests). `wallet_send_dry_run` (no unlock,
      confirmed live it needs none even against a locked wallet) and
      `wallet_send` (`WalletSendResult` TS-exported) Tauri commands:
      tries an explicit `passphrase` first, falls back to
      `WalletSession`, fails with `AppErrorCode::WalletLocked` before
      ever calling ord if neither exists; unlocks, optionally
      remembers, sends, always re-locks afterward regardless of the
      send's own outcome. The underlying unlock-send-lock cycle itself
      was already proven live by `wallet_cli_create_fund_send_and_
      restore` (`nk-testkit`) -- this just orchestrates the same
      already-tested pieces, consistent with every other thin Tauri
      command in this file having no command-layer test of its own.
      **Frontend half now built** -- `WalletSendForm`'s passphrase
      prompt, wrong-passphrase rejection, and "remember for this
      session" checkbox, wired through `ConfirmDialog`'s new optional
      `passphrase` prop (see Send bullet below for the full flow).
- [x] Balance (cardinal vs inscribed), receive (address + QR). New
      typed Tauri commands `wallet_balance`/`wallet_receive_address`
      (`WalletBalance` TS-exported), a `WalletScreen` composing
      create/restore (wired to the existing `SensitiveSeedView`/new
      `RestoreWalletForm`) with `WalletBalanceSection` once a wallet
      exists, backed by `useWalletExists`/`useWalletBalance`. Added
      `qrcode.react` (new frontend dependency, MIT, well-established)
      for the receive-address QR. Added a simple Dashboard/Wallet nav
      tab bar to `App.tsx` -- the app had no way to reach a second
      screen before this. Verified live in the browser (dev IPC mock
      extended with a fake per-chain wallet): create -> confirm words
      -> balance+QR renders; switching environments shows each one's
      own wallet state independently; restore -> balance+QR renders
      for the restored chain. "Multiple named wallets" not done yet --
      every wallet command hardcodes `DEFAULT_WALLET_NAME = "ord"`
      for now, tracked as a separate follow-up.
- [x] **Enforce mainnet wallet encryption.** Found by the Phase 5
      security self-review (item 8) and fixed following the user's
      decision (raised as a STOP-AND-ASK, per CLAUDE.md): encrypt
      during `create_wallet`/`restore_wallet` themselves, immediately
      after the underlying `ord wallet create`/`restore` call and
      before the mnemonic is shown or anything returns to the frontend.
      VERIFIED live first (DECISIONS.md) that this exact sequence is
      safe: encrypting a wallet right after `ord wallet create`
      doesn't invalidate the mnemonic ord already returned -- Core's
      `encryptwallet` response text ("a new HD seed was generated") is
      misleading boilerplate for descriptor wallets; a real round trip
      (create -> capture mnemonic + address -> encrypt -> fund -> full
      mnemonic restore into a separate wallet) recovered the exact same
      funds, and master-key fingerprints matched exactly before/after.
      New `require_encryption_passphrase_on_mainnet` helper enforces
      this backend-side (mainnet + no passphrase -> a clear typed
      error), not just via the frontend hiding/showing a field --
      `create_wallet`/`restore_wallet` both take an optional
      `passphrase`, wrapped in `Zeroizing` immediately per the
      self-review's own zeroization finding. Frontend: `WalletScreen`
      reveals a passphrase + confirm step before calling `create_wallet`
      only on mainnet (regtest/signet/testnet4 stay frictionless for
      testing, matching the spec's literal "every MAINNET wallet"
      scope); `RestoreWalletForm` gained the same fields, same gating.
      New real integration test,
      `encrypting_a_wallet_immediately_after_create_still_restores_
      correctly` (`nk-testkit`): the exact create -> encrypt (before
      any other use) -> fund -> restore -> compare-balances sequence,
      **ran live on this Windows machine, passed**. Verified live in
      the browser (dev IPC mock extended to mirror the mainnet-requires-
      passphrase rejection): mainnet Create reveals the passphrase step,
      mismatch is caught before submit, matching passphrases proceed to
      the seed view; regtest Create still skips straight to the seed
      view with no passphrase step at all.
- [x] Inscriptions gallery + rune balances. VERIFY'd live first
      (DECISIONS.md) against a real scratch regtest+ord with an actual
      inscribed HTML file: `ord wallet inscriptions`'s real field names
      (`inscription`/`location`/`postage`, not `id`), and that
      `/preview/<id>` (not `/content/<id>`) is ord's own
      cross-embedding-safe endpoint (tighter CSP, CORS-open, wraps
      every content type uniformly) -- confirming the Foundation D
      sandboxed-iframe component finally has a real target. New
      `wallet_inscriptions` Tauri command (`WalletInscriptionEntry {id,
      postage}`) backs `InscriptionGallery`: a grid of `<iframe
      sandbox="allow-scripts">` (no `allow-same-origin`, no
      `allow-forms`/`allow-popups`/top-navigation), `src` built from the
      environment's own `ord_port` -- already scoped by the existing
      CSP `frame-src` (Foundation D, done earlier this phase). Rune
      balances: extended `WalletBalance` with `runes: Option<Vec<{name,
      raw}>>` -- VERIFY'd live that ord's `wallet balance` JSON gains
      `"runes"`/`"runic"` keys only when the *running* server's
      index-runes is actually on (absent, not empty/zero, otherwise),
      so this checks the real response rather than trusting Nodekeeper's
      own `Environment.index_options` record (same "verify the real
      state, not just the config" reasoning as `OrdStatus`'s
      index-option booleans, Phase 4). Each rune's value is forwarded as
      opaque JSON text rather than a typed amount/symbol struct --
      etching a real rune on the scratch regtest to check the non-empty
      shape hit a 2-minute wall (a stuck `wallet batch` after
      broadcasting its commit tx) and wasn't worth further time for a
      rendering-precision detail; documented as an open gap in
      DECISIONS.md. Per docs/SPEC.md item 3, "Runes are VIEW-ONLY": the
      balance section always shows an explicit "not supported... yet"
      notice alongside any rune balance, and no send/mint/etch UI exists
      anywhere. Verified live in the browser (dev IPC mock extended with
      two fake inscription ids and a fake rune entry): the gallery grid
      renders with real ids (the iframes themselves can't load real
      content in the mocked browser preview -- no real ord server behind
      it there, same limitation as every other IPC-only mock); Regtest
      (index-runes on by default) shows the rune balance + view-only
      notice; Mainnet (index-runes off) shows no Runes section at all,
      not an empty one -- confirms the `null`-vs-`[]` distinction
      actually reaches the UI correctly.
- [x] Send: `WalletSendForm` (address, amount, fee rate) ->
      `wallet_send_dry_run` preview (fee shown, or the friendly
      wrong-network-address error with technical details expandable)
      -> `ConfirmDialog` (MAINNET extra ack checkbox, reused unchanged
      -- no screen implements its own confirmation flow) -> `wallet_
      send`. New `nk-rpc::estimate_smart_fee` +
      `wallet_fee_estimate` Tauri command (a new bitcoind-only
      `bitcoin_rpc_context` helper, factored out of `wallet_context`
      since fee estimation doesn't need ord running) pre-fill the fee
      field; VERIFY'd live (DECISIONS.md) that regtest has no fallback
      estimate (`estimatesmartfee` returns no `feerate` field), so the
      field is left blank with a placeholder rather than a fake
      number -- the user must enter one. Absurd-fee guard (>200 sat/vB,
      or >5% of the send amount, or >50,000 sats) shown as a
      non-blocking warning, not a hard block. `ConfirmDialog` extended
      with an optional `passphrase` prop (value/onChange/remember/
      error, Input `type="password"` + a "remember for this session"
      checkbox) -- `WalletSendForm` tries the send with no passphrase
      first, and only shows the field after catching a real
      `WALLET_LOCKED` rejection (matching how the backend already
      falls back to `WalletSession` before asking); a wrong password
      keeps the dialog open with the real RPC -14 error text inline.
      **Non-Taproot-inscription warning intentionally deferred**: it
      needs real inscription data to warn about, which doesn't exist
      until Phase 6's inscribe studio -- nothing to gate against yet,
      not a missed step. Verified live in the browser (dev IPC mock,
      `MOCK_WALLET_PASSPHRASE`): wrong-network address rejected with
      the exact live-VERIFIED ord error text; valid preview shows the
      correct fee with no false absurd-fee warning; Send opens
      `ConfirmDialog` with the mainnet warning; wrong passphrase
      rejected inline without closing the dialog; correct passphrase
      + "remember" succeeds and returns to the balance view; a second
      send immediately afterward succeeds with **no** passphrase
      re-prompt, confirming the remembered-passphrase path end to end.
- [x] Transaction history. VERIFY'd live first (DECISIONS.md): `ord
      wallet transactions` only returns `{transaction, confirmations}`
      per entry -- too sparse on its own -- so the new
      `wallet_transaction_history` Tauri command joins each txid
      against bitcoind's own wallet-scoped `gettransaction` (its
      top-level `amount` is already netted across every output, no
      manual summing) for amount/time/confirmations/`generated`.
      `TransactionHistorySection` lists them newest-first as ord
      already orders them, green `+`-prefixed amounts for receives,
      plain `-`-prefixed for sends, a "Mined"/"Pending" note per entry.
      Capped at the 50 most recent (`TRANSACTION_HISTORY_LIMIT`, same
      "last N" cap philosophy as `command_history`'s per-environment
      cap) -- not paginated yet. Verified live in the browser (dev IPC
      mock, three fake entries: a mined coinbase, a confirmed send, a
      pending receive) -- all three render with the correct sign,
      color, and label.

Acceptance criteria (from docs/SPEC.md Phase 5 "Done when"):
- [x] [CI] the fake-mnemonic and fake-passphrase search test passes --
      `create_and_restore_wallet_never_leak_the_mnemonic_to_the_
      broadcast_stream` (`nk-testkit`, real+fake mnemonic) plus
      `nk-rpc`'s two `wallet_passphrase`/`encrypt_wallet` redaction
      unit tests (fake passphrase). All part of every `cargo test
      --workspace` run; confirmed passing locally this session
      (pending a fresh CI run once GitHub Actions billing is resolved
      -- see the Phase 4 CI note).
- [ ] [CI] a malicious test HTML/SVG inscription cannot call Tauri IPC
      or read app data
- [ ] [CI] wrong-network addresses are rejected; an inscription send
      completes on regtest -- wrong-network rejection is VERIFIED live
      against real ord (DECISIONS.md) and exercised in the browser
      (mocked IPC mirrors the exact real error text), but has no
      automated `cargo test` assertion of its own yet; "an inscription
      send" can't be built until Phase 6 produces real inscriptions to
      send. Left unchecked until both exist as real CI-run tests.
- [ ] [MANUAL] mainnet confirmation appears for a mainnet send (cancel
      it before broadcasting); encrypted wallet unlock/lock works; the
      session remember clears after the idle timeout
- [x] Encryption compatibility with ord VERIFIED (DECISIONS.md,
      2026-09-24) -- works cleanly, no workaround needed
- [x] Security self-review completed (DECISIONS.md, 2026-09-25) -- went
      through every SECURITY RULES line item; closed two Phase-2-
      deferred items with real evidence (sensitive-channel mnemonic
      flow, argv-vs-stdin/RPC secrets), fixed a real `WalletSession`
      zeroization gap found while reviewing, and surfaced one
      significant unresolved gap (mainnet wallets aren't actually
      encrypted yet -- see the new task above) rather than improvising
      a fix for a named STOP-AND-ASK topic.

## Phase 6 — Inscribe studio

In progress. Tasks (one at a time: implement -> test -> quality gate ->
commit -> tick). VERIFY'd live before starting (DECISIONS.md): single
inscribe's and batch inscribe's real JSON output shape; reinscribe's
exact syntax (`--satpoint`/`--reinscribe`) and the exact rejection text
without it; that ord 0.29.0's batch YAML schema does **not** support a
per-entry `reinscribe` field (hide it from the batch builder, per the
spec's own "VERIFY; hide otherwise"); and that `GET /sat/<n>` is the
direct source for "all inscriptions on this sat, in order" (needs
`--index-sats`, gated per Foundation F).

`nk-ord` (inscribe/batch/reinscribe CLI wrapper, sat lookup HTTP client) — **done**
- [x] `nk_ord::wallet::inscribe` (single inscribe *and* reinscribe --
      `reinscribe_satpoint: Some(sp)` adds `--satpoint`/`--reinscribe`
      together, since VERIFYing found no legitimate reason to ever send
      one without the other) and `batch_inscribe` (dry-run and real,
      `Sensitivity::Normal` -- no secret involved, same as
      `wallet_send`). Batch YAML is built from typed
      `BatchInscriptionEntry` data and written to a real tempfile via
      `serde_yaml`, never hand-assembled text -- avoids a
      path-injection-shaped surface. Deliberately no per-entry
      `reinscribe` field: ord 0.29.0 rejects one outright (VERIFIED
      live, DECISIONS.md), so batch reinscribe isn't offered at all.
- [x] `OrdClient::sat(sat_number)` (the `/sat/<n>` JSON lookup) and
      `OrdClient::inscription(id)` (the `/inscription/<id>` JSON lookup,
      needed for reinscribe mode's "existing inscriptions with
      previews"). Refactored `status()` onto the same shared
      `get_json` helper these two use, rather than duplicating the
      record/redact/broadcast wiring a third time.
- [x] Real test, `inscribe_batch_and_reinscribe_all_work_and_the_sat_
      shows_both_in_order` (`nk-testkit`) -- the literal Phase 6 [CI]
      acceptance criterion: single inscribe, batch inscribe (2 files,
      1 reveal tx), and reinscribe all run against a real regtest+ord,
      then `GET /sat/<n>` confirms the original and the reinscription
      both appear, oldest first. **Ran live on this Windows machine,
      passed.** Along the way, found and fixed a real gap in an
      earlier Phase 5 test (`encrypting_a_wallet_immediately_after_
      create_still_restores_correctly`): it was missing the
      `#[serial(real_bitcoind)]` attribute every other real-bitcoind
      test in this file carries (documented CI-stability reason --
      several concurrent real bitcoind processes starved
      windows-latest runners past their startup timeout).

Backend — Tauri commands — **single inscribe and batch done, reinscribe gating and the sat lookup command still open**
- [x] `wallet_inscribe_dry_run`/`wallet_inscribe` -- file path comes
      from the frontend (drag-and-drop or the dev-preview placeholder),
      `postage`/`parent` as optional advanced params, plus
      `reinscribe_satpoint` (unused by the frontend yet -- reinscribe
      mode UI is still a separate task below, but the same command
      already supports it since it's the same underlying `ord wallet
      inscribe` call either way)
- [x] `wallet_inscribe_batch_dry_run`/`wallet_inscribe_batch` -- built
      (mirrors the single-inscribe command's unlock/remember/relock
      shape), but nothing calls them yet -- the batch-YAML builder UI
      is still a separate task below
- [x] `inscribe_file_preview`: size + best-effort extension-based
      content-type + a capped base64 `data_url` for the sandboxed
      preview (`None` above 2 MiB -- no point embedding megabytes of
      base64 through IPC for a small preview iframe)
- [ ] A dedicated `sat_inscriptions(sat_number)` Tauri command for the
      reinscribe gallery (the `OrdClient::sat`/`inscription` HTTP calls
      it would wrap already exist) -- not built, reinscribe mode isn't
      built yet either
- [ ] Explicit "block the action if the ord index isn't fully synced"
      gating for reinscribe specifically (spec item 4) -- today only
      the general `wallet_context` bitcoind+ord-running check applies,
      not an ord-*caught-up* check; needed once reinscribe mode exists

Frontend — Inscribe studio — **single inscribe done; batch UI and reinscribe mode are separate, later tasks**
- [x] `InscribeStudioScreen`: drag-and-drop via Tauri's core
      `onDragDropEvent` webview API (no plugin needed), sandboxed
      preview (`<iframe sandbox="allow-scripts">`, same Foundation D
      pattern as the wallet gallery, `src` a `data:` URI since there's
      no ord-hosted URL before inscribing), content-type + size shown,
      a size warning past `LARGE_FILE_WARNING_BYTES`. **Real drag-and-
      drop can't be exercised in the browser dev preview** -- confirmed
      live that `@tauri-apps/api/webview` reads `window.__TAURI_
      INTERNALS__.metadata` at *import* time (not just when called),
      which `dev-tauri-mock.ts`'s `mockIPC` never sets, so even a
      dynamic import crashes there unless wrapped in try/catch (fixed;
      same shape as `store/monitor.ts`'s exec-event subscription). The
      dev preview instead gets a click-to-load-a-placeholder-path
      affordance, gated on a new `isRealTauriRuntime()` helper (checks
      `.metadata` specifically, not just `__TAURI_INTERNALS__`
      presence, since the mock defines the latter too) -- used to
      verify the rest of the screen live.
- [x] Fee-rate picker + fee guard (same thresholds/shape as
      `WalletSendForm`, not reimplemented from scratch), dry-run
      preview showing fee + target location.
- [x] Mainnet extra confirmation via the existing shared
      `ConfirmDialog` -- no bespoke flow, verified live on both mainnet
      (extra ack + passphrase step) and regtest (neither).
- [x] Advanced options (hidden by default): postage, parent inscription
      ID.
- [x] Visual batch-YAML builder (`BatchInscribeForm`): drag-and-drop
      (or single-click in the dev preview) adds files, de-duplicated;
      each has its own Remove button. "Visual"/"edit" read as building
      the batch by adding/removing files, not by hand-editing raw YAML
      that then gets shelled out to ord -- the actual command is always
      built server-side from this same typed file list, matching why
      the backend never accepted free-text YAML in the first place
      (DECISIONS.md). A collapsible "View batch YAML" section shows a
      client-side mirror of that exact data (same `mode: separate-
      outputs` / `file:`-per-entry shape VERIFIED live against real
      ord) purely for transparency, with an "Export YAML" button (a
      client-side Blob download, same mechanism as the Live Command
      Monitor's own export). No reinscribe option per entry, no
      parent/postage -- ord 0.29.0's batch schema doesn't support the
      former at all (VERIFIED live) and `BatchInscriptionEntry` isn't
      wired for the latter yet. Shares `SingleInscribeForm`'s fee-guard
      thresholds and `ConfirmDialog` usage; both now share a new
      `useDragDropFiles` hook (extracted rather than duplicating the
      hard-won drag-drop-in-a-browser-preview handling, DECISIONS.md).
      Verified live in the browser: 3 placeholder files added, YAML
      preview matches exactly, dry-run shows the right count/fee,
      mainnet confirmation + passphrase flow, success screen lists all
      3 created inscriptions. Single mode re-verified working
      unchanged after being extracted into its own component.
- [ ] Reinscribe mode: pick an owned inscription from the gallery, show
      the sat's full inscription history in order (or the Foundation F
      explanation if `--index-sats` is off), permanence/visibility
      explainer before the first reinscription, dry-run + review screen
      with target sat/existing inscriptions/new content/fee/resulting
      count, mandatory "I understand this sat already has inscriptions"
      checkbox, "Reinscription #N on sat X" labeling.

**Observed, not yet addressed**: the passphrase-unlock flow (both
`wallet_send`, Phase 5, and the new `wallet_inscribe`) always attempts
`walletpassphrase` before a real signing action, on every chain -- this
assumes the wallet is encrypted. Mainnet now always is (Phase 5's fix),
but a regtest/signet/testnet4 wallet still isn't by default, so a real
(non-dry-run) send or inscribe against a never-encrypted test-chain
wallet would hit bitcoind's own "running with an unencrypted wallet"
RPC error rather than a clean UX. Not a new Phase 6 regression --
`wallet_send` already has the identical shape -- and every automated
test so far happens to encrypt the wallet first either way, so this
edge case has never actually been exercised end to end. Worth a look
whenever wallet flows are revisited next, not fixed here.

Acceptance criteria (from docs/SPEC.md Phase 6 "Done when"):
- [ ] [CI] inscribe and reinscribe both work on regtest; the sat shows
      both inscriptions in order
- [ ] [MANUAL] the reinscribe checkbox is enforced; gated features
      explain missing index options

## Phase 7 — Console, scripts, explorer (MVP complete)

Not started. See docs/SPEC.md Phase 7.

## Phase 8 — Multi-environment UI and Test Lab

Not started. See docs/SPEC.md Phase 8.

## Phase 9 — Portable mode

Not started. See docs/SPEC.md Phase 9.

## Phase 10 — Extras and release

Not started. See docs/SPEC.md Phase 10.
