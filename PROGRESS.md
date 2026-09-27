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
      mainnet_smoke_test -- <path-to-verified-bitcoind>` (since generalised and renamed
      `chain_smoke_test <chain> <bitcoind> [ord]`, Phase 10 step 2) (get a verified
      binary first via `cargo run -p nk-verify --example
      fetch_bitcoin_core`). Still needs you to actually run it and
      confirm it prints "Smoke test passed."
- [ ] [MANUAL] I have checked every pinned builder-key fingerprint
      against bitcoin-core/guix.sigs from a separate machine or browser
- [x] Security self-review completed (go through SECURITY RULES line by
      line, point to the code/test enforcing each, list any gaps) — see
      DECISIONS.md "Phase 2 — security self-review"

**Gap found and tracked (2026-09-25):** docs/SPEC.md's phase overview
states "the setup wizard grows phase by phase (system check and
disclaimer in Phase 1, Bitcoin Core in Phase 2...)" — that Bitcoin Core
step never got a real screen. Only the backend verification engine
(`nk-verify`, above) was built; the actual UI (download with progress,
choosing the data directory including external drives, showing the
verification result) does not exist. It slipped through because Phase
2's own "Done when" criteria are all backend-testable and didn't force
a screen into existence. Also still open from this same phase: the
"offer to attach" decision noted above (an already-running bitcoind
whose data directory matches and cookie authenticates) was deferred
"before Phase 3" and never picked back up in any later phase either.
Newly tracked here, not built yet:
- [x] Let the user pick the data directory (including external drives)
      before starting — data-directory picker on the System Check
      screen (`get_environment_data_root`/`set_environment_data_root`,
      native folder dialog via `@tauri-apps/plugin-dialog`, refuses
      while any environment is running, validates the folder is
      writable). See DECISIONS.md "Setup wizard UI: data-directory
      picker, scoped deliberately (2026-09-25)".
- [x] Setup wizard screen: trigger `nk_verify::bitcoin_core`'s download
      + verify, show progress and the verification result (fail closed,
      same as the backend already does) — `BinarySetupScreen`, real
      pure-Rust download/extract/install orchestration
      (`nk_verify::bitcoin_core::download_verify_and_install_bitcoin_core`),
      live-verified end to end. See DECISIONS.md "Setup wizard UI:
      binary download + verify screens (2026-09-25)".
- [ ] "Offer to attach" UI: when an already-running bitcoind is
      detected on the target data directory and its cookie
      authenticates, let the user attach instead of refusing outright
      (the detection primitive already exists in `nk-proc`, per above)
- [ ] Point at an already-installed Bitcoin Core/ord binary instead of
      downloading a fresh one. Deliberately not built alongside the
      download-and-verify screens (DECISIONS.md, "Setup wizard UI:
      binary download + verify screens") because doing so without
      verification would let a user configure an arbitrary,
      completely unverified executable -- a direct violation of
      CLAUDE.md's "binary verification fails closed" rule. Needs a new
      `nk-verify` entry point first: today it only exposes combined
      `download_and_verify_*`/`download_verify_and_install_*`
      functions, no standalone "verify this file that's already on
      disk" function (checksum + signature check against pinned
      values, same as the download path, just skipping the network
      fetch) -- design and build that, then the file picker becomes
      safe to add

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

**Gap found and tracked (2026-09-25):** same pattern as Phase 2's note
above — docs/SPEC.md's phase overview says the wizard should have grown
to include "ord and index options in Phase 4," and again only the
backend (`nk-verify`'s pinned-hash ord verification, `nk_core::
ord_conf`'s index-option plumbing) was built, no screen. Newly tracked
here, not built yet:
- [x] Setup wizard screen: trigger `nk_verify::ord`'s download + verify
      with progress/result — `BinarySetupScreen` (shared with Bitcoin
      Core, above), real pure-Rust download/extract/install
      orchestration (`nk_verify::ord::download_verify_and_install_ord`),
      live-verified end to end. See DECISIONS.md "Setup wizard UI:
      binary download + verify screens (2026-09-25)".
- [x] The index-options step -- explain disk/time cost per option, list
      which app features each unlocks (Foundation F), and state clearly
      the choice is effectively permanent (changing it later means a
      full reindex) — `IndexOptionsScreen`, one card per environment
      (all four chains), backed by a new per-chain
      `index_options_<chain>` setting and `set_index_options` command
      (refuses while that chain's ord is running). Live-verified:
      per-environment toggles are independent, Regtest defaults to
      everything on per Foundation F, choices persist and the
      environment store refreshes so the Dashboard reflects them
      immediately. See DECISIONS.md "Setup wizard UI: index-options
      step (2026-09-25)".
- [ ] Wizard default: per spec, start ord indexing automatically once
      Bitcoin Core finishes its initial sync, with a "start now anyway"
      override and warning -- no such sequencing exists yet; today
      bitcoind and ord are started independently by the user

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
- [x] `inscription_detail(chain, id)` and `sat_inscriptions(chain,
      sat)` -- wrap `OrdClient::inscription`/`sat` via a new `ord_client`
      helper (fresh client from public state, same "don't reuse
      `NodeManager`'s internal one" precedent as `bitcoin_rpc_context`'s
      `RpcClient`). `InscriptionDetail.sat: Option<u64>` is `None` when
      `--index-sats` is off, read from the real response each time, not
      a cached config value (DECISIONS.md).
- [x] "Block the action if the ord index isn't fully synced" (spec item
      4): decided *not* to add a separate proactive check -- ord's own
      wallet commands already refuse to run while behind with a clear
      "N blocks behind bitcoind" error, already mapped to
      `AppErrorCode::OrdNotSynced` and surfaced through the existing
      `ErrorPanel`/`friendlyError` path a reinscribe attempt would hit
      naturally. Adding a second, separate gate ahead of that would be
      duplicate logic for the same real signal.

Frontend — Inscribe studio — **done: single, batch, and reinscribe mode**
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
- [x] Reinscribe mode (`ReinscribeForm`), a 3-step flow: **pick** an
      owned inscription (reuses `useWalletInscriptions`, the same data
      `InscriptionGallery` uses) -> **compose** (permanence/visibility
      explainer shown every time, not a one-time flag, DECISIONS.md;
      the sat's full inscription history in order with previews +
      numbers, or the Foundation F explanation when `sat` comes back
      `None`; new content drop zone; fee rate; dry-run) -> **review**
      (target sat, existing/resulting inscription counts, fee, the
      existing history and new content previews again, the mandatory
      "I understand this sat already has inscriptions" checkbox gating
      a "Reinscribe" button) -> the shared `ConfirmDialog` (mainnet ack
      + passphrase, `title`/`description` reading "Create reinscription
      -- This will be reinscription #N on sat X, for a fee of Y",
      exactly the spec's labeling). The review screen is deliberately
      its own step, not folded into `ConfirmDialog` (DECISIONS.md: the
      checkbox is reinscribe-specific review content, not the general
      confirm-this-action gate every fund-moving flow already shares).
      New `InscriptionPreviewTile` extracted from `InscriptionGallery`
      so the picker grid and sat-history displays don't triplicate the
      sandboxed-iframe markup. Verified live in the browser (dev IPC
      mock: one entry with a real `sat`, one with `sat: null`): full
      happy path through to a real success screen showing "reinscribe
      #2 on sat X"; picking the `sat: null` entry shows the Foundation F
      explanation instead of a history list, exactly as designed.

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
- [x] [CI] inscribe and reinscribe both work on regtest; the sat shows
      both inscriptions in order --
      `inscribe_batch_and_reinscribe_all_work_and_the_sat_shows_both_
      in_order` (`nk-testkit`), the literal criterion as a real
      integration test against real bitcoind+ord. **Ran live on this
      Windows machine, passed** (pending a fresh CI run once GitHub
      Actions billing is resolved -- see the Phase 4 CI note).
- [ ] [MANUAL] the reinscribe checkbox is enforced; gated features
      explain missing index options -- UI is built and live-verified in
      the dev browser preview (mocked IPC: the mandatory checkbox
      genuinely disables "Reinscribe" until checked, and the Foundation
      F explanation shows correctly when a picked inscription's `sat`
      is `None`), but needs your own check against the real app.

## Phase 7 — Console, scripts, explorer (MVP complete)

In progress. Tasks (one at a time: implement -> test -> quality gate ->
commit -> tick):

Explorer (docs/SPEC.md item 5) — no new backend needed
- [x] VERIFY live against ord 0.29.0: `/search/<query>` is ord's own
      real type-auto-detecting endpoint (303 redirect to `/inscription`,
      `/sat`, `/tx`, `/block`, or `/address`), including a real
      block-hash-vs-txid disambiguation done server-side. See
      DECISIONS.md "Phase 7 — VERIFY: ord's explorer/search HTTP
      surface (2026-09-25)" for the full routing table and the bare-
      integer-is-always-an-inscription-number gotcha.
- [x] Design decision (same VERIFY entry): since Foundation D says "the
      embedded explorer follows [the same rules as inscription
      previews]," the Explorer embeds ord's own HTML pages in a
      sandboxed iframe rather than re-implementing search/result
      rendering -- no `OrdClient` additions or `explorer_search`
      command needed, `frame-src`'s existing per-origin CSP entries
      already cover every path under each environment's ord origin
- [x] `ExplorerScreen`: search box navigating a sandboxed
      `<iframe src=".../search/<query>">` (same `sandbox="allow-
      scripts"`, no `allow-same-origin`, `referrerPolicy="no-referrer"`
      discipline as `InscriptionPreviewTile`), plus a proactive summary
      of which index options this environment has enabled (Foundation
      F) shown above the search box instead of letting a disabled-
      index search hit ord's raw error page. Live-verified in the
      browser dev preview: the note reads "None enabled" for Mainnet's
      defaults and "Sats, Runes, Addresses" for Regtest's, updates
      immediately on environment switch, and the search box correctly
      transitions from the empty state to the (in dev preview, inert --
      no real ord server) iframe.
- [x] Add to the main nav alongside Dashboard/Wallet/Inscribe

Backend — Console command execution + safety layer (docs/SPEC.md item 6)
- [x] Safety classification: `nk_core::console_safety` --
      `classify_bitcoin_rpc`/`classify_ord_wallet_subcommand`, built
      from a real `bitcoin-cli help` + per-subcommand `ord wallet <cmd>
      --help` VERIFY (not guessed) recorded in DECISIONS.md "Phase 7 —
      VERIFY: the real RPC/CLI surface for the console safety layer".
      Read-only allowlist / fund-moving list / everything-else-defaults-
      to-needs-confirmation, so an unrecognized future RPC method fails
      closed (needs confirmation) rather than silently running
      instantly. 14 tests, including the exact fund-moving list, the
      `sendrawtransaction`-isn't-wallet-scoped distinction, and both
      ord's `mint`/`offer accept` no-dry-run gap.
- [x] Command-line parsing: `nk_core::console_parse` --
      `parse_command_line` (quote-aware tokenizing, so
      `sendtoaddress "bcrt1..." 0.5 "a comment with spaces"` splits
      correctly) and `coerce_json_args`, matching bitcoin-cli's own
      real argument convention (VERIFIED live: a bare `true` sends the
      JSON boolean, not the string -- DECISIONS.md). 12 tests.
- [x] `console_classify(command_line)` + `console_run(chain,
      command_line, dry_run)`: classify-then-run split (mirrors the
      dry-run/confirm pattern every other fund-moving screen already
      uses), `console_run` re-checks `blocked_reason` itself rather than
      trusting the frontend called classify first -- the backend is the
      real enforcement point. bitcoin-cli side calls
      `nk_rpc::RpcClient::call` directly (the same path
      `getblockchaininfo` etc. already use); ord side calls a new
      `nk_ord::wallet::run_console_subcommand`, the same
      `Executor::execute` machinery every other wallet command uses.
      `ord create`/`restore` are refused outright, not just confirmed
      (`OrdCommandClass::BlockedUseWalletScreen`) -- both can print a
      mnemonic to stdout, which running through the console's raw-
      output path would leak into the Live Command Monitor and
      `command_history`, violating the sensitive-output-channel rule.
      Also found and fixed: several bitcoin-cli RPCs
      (`walletpassphrase`, `encryptwallet`, `signmessagewithprivkey`,
      `signrawtransactionwithkey`, `importdescriptors`) take a
      passphrase/private key as a **plain positional argument** -- the
      one case where CLAUDE.md's "secrets via stdin/RPC params, never
      argv" can't be avoided, since a console has no other input
      channel. `console_secret_values`/`secret_bitcoin_rpc_arg_indices`
      redact those positions from the display and the
      `RpcClient::call` redact list unconditionally, regardless of
      read-only/state-changing classification (`signmessagewithprivkey`
      is technically read-only but still carries a private key).
- [ ] Mainnet fund-moving commands route through the same mainnet
      extra-confirmation `ConfirmDialog` every other fund-moving screen
      already uses -- no new confirmation flow (the backend already
      blocks fund-moving commands outright per the "Inscription
      protection" rule above; this item is about the *remaining*
      state-changing-but-not-blocked commands still needing the
      mainnet-specific extra warning on top of the ordinary confirm)
- [ ] PSBT preview flow for raw Core-wallet spend commands
      (`walletcreatefundedpsbt`/`testmempoolaccept`), matching how
      `wallet_send_dry_run` already previews ord-wallet sends

Frontend — Console UI
- [x] `ConsoleScreen`: prompt shows "[chain] $", scrollback-style
      history with pretty-printed JSON output. Drives the full safety
      flow: read-only runs instantly, state-changing goes through the
      shared `ConfirmDialog` (Learn Mode shows the exact command,
      mainnet gets its own extra step automatically since it's the
      same shared dialog every other fund-moving screen uses), a
      `blocked_reason` is shown as a refusal with nothing run, and a
      dry-run-capable ord command fetches and shows its preview inside
      the confirm dialog before the real run. Live-verified in the
      browser dev preview: all five paths (read-only, blocked
      bitcoin-cli fund-move, mainnet-confirmed state-changing, blocked
      `ord create`, dry-run-previewed `ord send`) exercised end to end.
      One console per environment (matching every other screen), not
      independently-tabbed multiple consoles per docs/SPEC.md's literal
      "each console tab" wording -- tracked below as a scope
      simplification, not equivalent functionality.
- [ ] Multiple simultaneous console tabs, each independently locked to
      its own environment (this pass built one console bound to the
      globally-selected environment, matching Dashboard/Wallet/
      Inscribe/Explorer's existing pattern)
- [ ] Autocomplete for bitcoin-cli/ord subcommands as the user types
- [ ] Saved command templates with fill-in fields
- [x] Add to the main nav

Backend — Script runner foundation
- [x] New `nk-scripts` crate: `detect_interpreters` (real probe --
      spawns `<name> --version` through the executor and checks
      `exit_code == 0`, not a PATH-presence check), `script_env_vars`
      (the four `NKP_*` variables), `run_script` (spawns through the
      executor with those env vars, `CommandSource::Script`). Found and
      handled live: on Windows, `python`/`python3` resolve on PATH but
      can be only the Microsoft Store app-execution-alias stub (prints
      "Python was not found... install from the Microsoft Store", exits
      49) -- a presence check alone would have been a false positive.
      Same shape as the already-known `bash` WSL-stub gotcha docs/
      SPEC.md calls out; both are handled for free by the same
      "actually run it and check the exit code" probe. Added `env_vars`
      to `nk_exec::CommandSpec` (threaded through to the real child
      process's environment) and a new `CommandSource::Script` so
      scripts show up distinctly in the Live Command Monitor. 6 tests
      total (2 pure, 2 real end-to-end -- including one that writes a
      real script and confirms an env var actually arrives in the
      child process, not just that the field was set). See
      DECISIONS.md "Phase 7 — script runner foundation" for the full
      write-up.
- [x] `run_script`/`list_scripts`/`list_available_interpreters` Tauri
      commands: `regtest_only` is a field on `ScriptInfo` Nodekeeper
      itself defines per script (never read from the script file's own
      content, which a user could edit to lie about it), re-checked by
      `run_script` itself against `chain` regardless of what the
      frontend already knew -- same "the backend is the enforcement
      point" shape as `console_run`. Built-in scripts are embedded at
      compile time (`include_str!`) and rewritten fresh to
      `data_root()/scripts/` on every run, so there's no risk of a
      stale on-disk copy surviving an app update.
- [x] 3 example scripts, all Python, all read-only (none need
      `regtest_only: true` -- nothing they do is unsafe on mainnet):
      export inscriptions to CSV (`/address/<addr>` +
      `/inscription/<id>` via ord's HTTP API), alert when the node
      falls behind (`getblockchaininfo` via a hand-rolled cookie-auth
      JSON-RPC call, since a script has no access to `nk-rpc`), daily
      disk-usage report (derives the data directory from
      `NKP_COOKIE_PATH`'s parent rather than needing a 5th env var).
      The RPC cookie-auth wire protocol (Basic Auth from the cookie
      file's `user:password`, JSON-RPC 1.0 POST body) was verified live
      against a real regtest bitcoind using a Node.js stand-in, since
      real Python isn't installed on this dev machine (see the
      script-runner-foundation DECISIONS.md entry) -- confirmed correct
      before trusting the equivalent Python `urllib`/`base64` code.
- [x] Script library UI: `ScriptsScreen` -- the mandatory "scripts are
      trusted code" warning shown persistently at the top (not a one-
      time dismissable gate), one card per script with its description,
      a `regtest_only` badge when applicable, an argument input, and a
      Run button disabled when the required interpreter isn't available
      or the environment doesn't satisfy `regtest_only`. No live-output
      UI of its own needed: `run_script` goes through the same executor
      every other command does, so its output already streams to the
      Live Command Monitor for free -- this screen only renders the
      final result once the run completes. Live-verified in the browser
      dev preview: typed an address into the CSV-export card's argument
      field, ran it, got the mocked result back inline.
- [ ] Import/create UI for the user's own scripts (today: only the 3
      built-ins are listed: `list_scripts` has no notion of a user-
      added script yet, and there's no file-picker/import flow or
      per-script `regtest_only` toggle for one)

- [x] Security self-review completed (go through SECURITY RULES line
      by line, point to the code/test enforcing each, list any gaps)
      -- see DECISIONS.md "Phase 7 — security self-review". No
      unresolved live gap in Phase 7's own new code; one forward-
      looking design question flagged for the not-yet-built script
      import flow (whether a fund-moving imported script needs its own
      confirmation dialog, since the runner currently doesn't have
      one -- deliberate for the 3 read-only built-ins, needs a real
      decision once import exists).

## Phase 8 — Multi-environment UI and Test Lab

**Done** (2026-09-25). docs/SPEC.md's "Build:" line for this phase
covers two of item 10/11's full scope plus three sub-parts of item 8
(notifications, tray, prevent-sleep) -- item 8's other sub-parts
(plain-language errors, diagnostics export, accessibility) already
shipped earlier or belong to a later phase per its own summary line.
Walkthrough (d) is the one named piece of item 11 intentionally not
built (needs multi-wallet support -- see DECISIONS.md and the Backlog
section below); everything else in this phase's own task list is
checked off.

Frontend — Multi-environment UI (docs/SPEC.md item 10) — **done**
- [x] `OverviewScreen`: one card per environment (reusing
      `useDashboardStatus`/`useOrdStatus` per chain -- no new backend
      needed, both hooks already poll+start/stop/restart), each
      showing bitcoind + ord status side by side; a "stop all" button
      that stops everything and silently ignores "wasn't running"
      errors, since it isn't the caller's job to know in advance what
      is or isn't up. Live-verified in the browser dev preview:
      starting Regtest's node updates its card (Syncing badge, disk
      usage) without touching the other three, and "Stop all" returns
      every card to Stopped.
- [x] Combined resource summary: aggregate a running-environment count
      (independent `is_node_running`/`is_ord_running` poll across all
      chains) + `system_check`'s available RAM; warns when 4+ services
      are running and available RAM is under 2 GB. Scoped down from the
      spec's literal "combined RAM... across running environments":
      precise per-process RAM attribution needs new backend
      instrumentation (tracking each spawned bitcoind/ord PID's actual
      memory via `sysinfo`) not built yet -- this pass uses system-wide
      available RAM plus a running-service count as the signal instead,
      tracked as a follow-up for the precise version.
- [x] Full switcher: `EnvironmentSwitcher` now polls
      `is_node_running`/`is_ord_running` per environment and shows a
      "(running)" text label next to any environment with something up
      -- text label, not color alone, per docs/SPEC.md item 8's
      accessibility rule. Live-verified the dropdown renders correctly
      with all 4 environments.
- [x] Add Overview to the main nav -- placed first, since it's the one
      screen not scoped to the currently-selected environment

Frontend + backend — Regtest Test Lab (docs/SPEC.md item 11)
- [x] VERIFY: does ord 0.29.0 have a built-in regtest environment
      command (e.g. `ord env`)? If so, decide whether to use it
      internally or keep the app's own controls only (spec: "the app's
      own controls must still work" either way). Found: `ord env`
      exists but starts its own bitcoind and takes no flag to point at
      an externally-managed one, so it would bypass nk-verify's pinned
      binary, NodeManager/nk-proc's process tracking, and the user's
      chosen data directory -- decided not to use it internally
      (DECISIONS.md).
- [x] One-click setup: start regtest bitcoind + ord, create a test
      wallet, mine 101 blocks to it (100-confirmation coinbase
      maturity) so its coins are spendable immediately. Implemented as
      `TestLabScreen`'s `oneClickSetup`, which starts both services
      then mines 101 blocks -- deliberately doesn't reimplement wallet
      creation inline (see below).
- [x] "Mine blocks" control (number field, default 1, `generatetoaddress`
      against the current wallet) usable standalone. Backend:
      `mine_blocks` Tauri command (regtest-only, refuses on any other
      chain), gets a receive address via `wallet_receive` then calls
      `generate_to_address`.
- [x] "Get test coins" button (mine blocks to the current wallet) --
      same `mine_blocks` command with count 1.
- [x] Post-action "Mine 1 block to confirm" offer after a send/inscribe
      on regtest, plus an optional auto-mine toggle. Built as one
      shared `RegtestMineOffer` component (renders nothing off Regtest,
      so every caller can mount it unconditionally) mounted in each of
      the 4 success views that end a fund-moving action:
      `WalletSendForm` (which previously had no persistent success view
      at all -- closed straight back to the balance screen on send;
      gave it one, matching the inscribe forms' existing pattern),
      `SingleInscribeForm`, `BatchInscribeForm`, `ReinscribeForm`. The
      auto-mine choice is one global setting (`regtest_auto_mine`, via
      `get_setting`/`set_setting` -- Regtest is the only chain that
      ever self-mines, so there's no reason to scope it per-chain);
      checking it mines immediately for the action just completed and
      makes every later action's offer auto-mine too, with no new
      backend command (reuses `mine_blocks` from above). Live-verified
      in the browser dev preview: sent BTC on Regtest, manually mined
      to confirm, then sent again and checked "Always mine
      automatically" (mined immediately), then did a fresh single-file
      inscribe and confirmed it auto-mined with zero clicks, the
      setting having persisted across the screen switch.
- [x] Guided walkthroughs with checkpoints, opening the Live Command
      Monitor automatically -- 4 of the 5 (a, b, c, e); (d) is
      intentionally not built (decided with the project owner
      2026-09-25): "send an inscription to a second test wallet" needs
      a second wallet in one environment, which every wallet-related
      Tauri command deliberately doesn't support (`DEFAULT_WALLET_NAME`
      in src-tauri/src/lib.rs) -- tracked in this file's Backlog
      section instead of worked around here.
      - (a) create wallet -> receive -> mine -> check balance
      - (b) inscribe -> mine -> see it in the gallery
      - (c) reinscribe -> mine -> see both on the sat
      - (e) run a console command and an example script
      Architecture: `useWalkthroughStore` (Zustand) tracks the active
      walkthrough/step/context so progress survives navigating away --
      `WalkthroughBanner` (mounted unconditionally in `App.tsx`, not
      inside `TestLabScreen`) would otherwise unmount every time a
      step sends the user to another screen. Each step's checkpoint is
      a real poll against Regtest state -- `wallet_exists`,
      `wallet_balance`, `node_status`'s block height,
      `wallet_inscriptions`'s count, `sat_inscriptions`, and
      `list_command_history` filtered by `triggering_action` (`"console"`
      vs. `"run script ..."`) -- no new backend commands needed, every
      checkpoint reuses existing IPC. A step with nothing machine-
      checkable (e.g. "look at your receive address") just enables
      Continue immediately; every step also has a "Skip" escape hatch.
      Live-verified all 4 walkthroughs end-to-end in the browser dev
      preview (mock enhanced to track a real growing inscriptions/sat
      list so the "inscriptionsIncreased"/"satHasTwoInscriptions"
      checkpoints could be genuinely exercised, not just skipped),
      catching and fixing two real bugs in the process: the checkpoint
      status text order (a "no checkpoint" step briefly showed "done"
      because `checkpointMet` defaults `true` for that kind), and
      "inscriptionsIncreased" comparing against a baseline taken when
      the *verification* step mounted instead of when the walkthrough
      started (the inscribe action happens on an earlier step, so the
      count had already risen by the time the wrong baseline was taken).
- [x] "Reset Test Lab": stop regtest services gracefully, delete only
      the regtest data directories after confirmation, start fresh --
      the docs/SPEC.md [CI] acceptance criterion ("Reset Test Lab
      deletes only regtest data") needs a real test proving other
      environments' directories are untouched. Backend: `reset_test_lab`
      Tauri command stops ord then bitcoind gracefully, then calls the
      extracted `delete_regtest_data_only(environment_data_root)`
      (zero-chain-parameter by construction, so it can never target the
      wrong chain), which removes only `Environment::new_default(Regtest,
      ..)`'s `data_root`. Two real tests (real tempdir, both regtest +
      mainnet subdirs with files present): confirms only regtest is
      deleted, and confirms it's a no-op when regtest has no data yet.
      Frontend: `TestLabScreen`'s "Reset" button goes through the shared
      `ConfirmDialog` before calling it.
- [x] Wire the setup wizard's "Try it safely" (currently doesn't exist
      as a real link) to open the full Test Lab. Added as a secondary
      button on `IndexOptionsScreen` (the wizard's last step, right
      before the mainnet-sync decision) -- saves the same index-option
      choices as the normal "Continue" path, then finishes the wizard
      into the Regtest environment's Test Lab screen instead of the
      default landing screen. Live-verified the full path in the
      browser dev preview: disclaimer -> system check -> binary
      setup -> index options -> "Try it safely first (Regtest)" lands
      directly on Test Lab with Regtest selected in both the env
      banner and switcher.

Frontend + backend — Notifications, tray, prevent-sleep (docs/SPEC.md
item 8's Phase-8 sub-parts)
- [x] Notifications: "node fully synced", "ord ready", disk-space
      warnings. VERIFY: the official plugin is `tauri-plugin-notification`
      (singular -- several unofficial `-notifications` plural forks
      exist and are not this), v2.4.0, confirmed compiling against this
      workspace's `tauri = "2"`. `NotificationWatcher` (mounted
      unconditionally in `App.tsx`) polls all 4 environments on its own
      15s interval, independent of whichever screen is open, and fires
      only on an observed transition this session (never just because
      an environment happens to already be synced/caught-up the first
      time it polls). Disk-space threshold reuses `DiskMonitor`'s
      existing 5 GiB constant rather than a second magic number.
- [x] Tray: minimize to tray while services run (installed mode). Cargo
      feature `tray-icon` added to the `tauri` dependency; VERIFY:
      current API is `TrayIconBuilder`/`Menu`/`MenuItem` in
      `tauri::tray`/`tauri::menu`, `Builder::on_window_event`'s closure
      takes `(&Window<R>, &WindowEvent)` (two args, not one) in the
      current release, `WindowEvent::CloseRequested { api, .. }` +
      `api.prevent_close()` to intercept the close button. Closing the
      window hides it instead of quitting only when
      `NodeManager::any_running()` is true (already existed, already
      tested -- checks every environment's bitcoind and ord at once);
      with nothing running, a normal close still quits, matching the
      spec's "while services run." A tray "Quit Nodekeeper" menu item
      stops every running environment gracefully (same timeouts
      `reset_test_lab` uses) before actually exiting, so quitting from
      the tray can't orphan a bitcoind/ord process. Portable-mode
      interaction not yet relevant -- portable mode itself doesn't
      exist until Phase 9.
      **Verification limits, noted honestly**: a tray icon lives
      outside any webview, so none of this session's browser-based
      tools can click it or see it render. Verified what's actually
      checkable: `cargo build`/`cargo check` succeed, `cargo test
      --workspace` passes (including `any_running`'s existing real-
      process tests), and a real build of the debug binary launches
      and stays running for several seconds with no panic/log output
      (proving `TrayIconBuilder::build(app)?` and the menu/window-event
      wiring don't error at startup) before being killed. The tray
      icon's actual appearance, click behavior, and the "hide on
      close, show on tray-click, quit stops everything" end-to-end
      flow still need a real manual check by the project owner
      (`just dev` or `just build-app`) the next time this runs outside
      this session.
- [x] Optional "prevent sleep during sync" setting. VERIFY: no official
      Tauri plugin exists for this (confirmed via search -- open
      feature request, tauri-apps/tauri#3697, still unresolved); used
      the `keepawake` crate (0.6.1, cross-platform RAII guard --
      Windows PowerRequest / macOS IOKit / Linux D-Bus under the hood,
      confirmed compiling with the guard held in a `Mutex` inside
      managed Tauri state). Backend: `set_prevent_sleep(enabled)`
      creates/drops a `keepawake::KeepAwake` guard (`sleep(true)`,
      display sleep left alone -- this is a background sync, not video
      playback). Frontend: a small `usePreventSleepStore` (Zustand,
      backed by `get_setting`/`set_setting`) holds the user's on/off
      preference; a toggle lives on the Overview screen (the closest
      thing to a cross-environment settings home today -- no dedicated
      Settings screen exists yet, that's docs/SPEC.md item 9, a later
      phase). `NotificationWatcher`'s existing per-environment poll
      (already fetching `node_status`/`ord_status` every 15s for the
      notification transitions above) also reports each environment's
      syncing/indexing state up to a parent aggregator, which calls
      `set_prevent_sleep(settingOn && anySyncing)` only when that
      combined value actually changes -- no second poll loop. Live-
      verified in the browser dev preview: the toggle persists across
      navigation, the command fires correctly on every relevant state
      change (confirmed via a temporary log statement, since the real
      OS-level sleep-inhibition effect itself isn't observable through
      any tool available this session), and `node_status`'s
      `initial_block_download` correctly reports `true` right after a
      node starts.

Security self-review at the end of this phase isn't explicitly
required by CLAUDE.md's "Phases 2, 5, and 7" list, but Test Lab's
"delete only regtest data" destructive-operation path deserves the
same VERIFY-before-trusting scrutiny as everything else touching
on-disk deletion in this project.

## Phase 9 — Portable mode

Paused (2026-09-26). Every item that could be built and verified on
this Windows machine is done; what's left is the packaging/build-
pipeline work below (portable layout, WebView2, native dialogs, drive-
prep wizard), deferred by the project owner, plus deferred/unverifiable
items each noted in place (macOS/Linux detection, USB speed, master-
password unlock). See docs/SPEC.md Phase 9 and item 12 for full scope.
**Cross-platform testing limit, stated up front**: this phase's own
[CI]/[MANUAL] criteria need real builds and manual runs on Windows,
macOS, and Linux -- this session runs on a single Windows machine, with
no macOS/Linux hosts, VMs, or cross-compilation toolchain available.
Windows-specific pieces here are built *and* verified for real; macOS-
and Linux-specific detection code is written from VERIFIED platform-API
research but cannot be executed or confirmed correct on those OSes in
this session -- flagged plainly per item, not silently claimed as
tested. Actually producing installers/bundles for all 3 OSes is a CI/
build-pipeline concern more than something built by hand here; tracked
separately, likely alongside Phase 10's own release/CI work.

Tasks (one at a time: implement -> test -> quality gate -> commit -> tick):
- [x] Portable-mode detection + relative data layout. A `config`
      directory next to the running executable is the portable-mode
      signal (the "prepare a new portable drive" wizard's job to create
      it once, up front) -- `is_portable_layout` extracted as a real
      tested pure function (3 tests: no config dir, a real config dir,
      a same-named file instead of a directory -- guards against a
      stray file flipping installed mode into portable by accident).
      `data_root()` (Nodekeeper's own settings DB/scripts/binary cache)
      and `default_environment_data_root()` (before the user picks
      their own via the existing data-directory picker) now branch:
      portable is `<exe_dir>/config` and `<exe_dir>/data` respectively,
      matching the spec's fixed relative layout; installed mode uses
      `dirs::data_dir()` (matches what Tauri's own `app_data_dir()`
      resolves to) joined with a friendly folder name. New
      `is_portable_mode` Tauri command exposes this to the frontend.
      **Real end-to-end integration check (2026-09-26), not just unit
      tests**: built the actual `nodekeeper.exe`, launched it from a
      fresh temp folder with a `config` subdirectory placed next to it,
      and confirmed the real running app wrote its settings database to
      `<that folder>/config/nodekeeper.sqlite3` -- not the OS app-data
      directory. Then the control case: launched the same binary from
      `target/debug/` (no `config` sibling) and confirmed it correctly
      fell back to the real `%APPDATA%/Nodekeeper/nodekeeper.sqlite3`
      instead, touching nothing at the binary's own location. Both
      real launches, both cleaned up afterward.
- [ ] Portable launchers and full folder layout (`/bin/<os>`,
      `/runtime/windows`, launcher naming) -- packaging/build-config
      work (`tauri.conf.json` bundle targets), not yet started.
      **Deferred by the project owner, 2026-09-26** ("leave it for
      now"), together with the WebView2, native-dialog, and drive-prep
      items below -- all four are one packaging effort. Context for
      whoever picks this up: Nodekeeper will ship as two distributions
      (a small installer, and a portable folder/zip for the drive), so
      they need separate build configs; `tauri.conf.json` today is one
      shared config with no portable-specific profile.
- [ ] Bundled WebView2 fixed-version runtime (Windows) -- packaging work.
      Checked against Tauri's docs: `bundle.windows.webviewInstallMode`
      `{ "type": "fixedRuntime", "path": "<extracted runtime dir>" }` is
      the only mode with no install step, but it needs Microsoft's
      WebView2 Fixed Version Runtime `.cab` (~180MB) downloaded from
      Microsoft and extracted locally before building. That download was
      not made (needs explicit permission), and setting it in the shared
      config would bloat the installed-mode build, which should keep the
      small bootstrapper -- so it belongs in a separate portable build
      profile, not yet created.
- [ ] Native prerequisite dialogs (a missing webview can't be reported
      via the webview itself) -- needs VERIFY of how Tauri surfaces a
      pre-webview-load failure natively per OS
- [ ] macOS App Translocation detection + guidance (quarantine
      attribute on the .app and on bitcoind/ord) -- code only,
      unverifiable without a real macOS host. Drafted and real-tested
      once (pure path-matching logic against the documented
      `/private/var/folders/.../AppTranslocation/...` shape, fully
      covered without needing a Mac), then deliberately not committed
      on the project owner's explicit call: this session can't compile-
      check the OS-gated half at all (a `#[cfg(target_os = "macos")]`
      function is skipped entirely by a Windows build, so even a
      syntax error would go unnoticed until a real macOS build), a
      meaningfully higher risk than this phase's other work. Revisit
      with real macOS access.
- [ ] Linux noexec-mount detection + guidance -- code only, unverifiable
      without a real Linux host. Same reasoning and same disposition as
      the macOS item above (drafted, real-tested for the pure `/proc/
      mounts`-parsing half, not committed) -- see that entry.
- [x] Filesystem checks: warn on exFAT, recommend NTFS. Backend:
      `SystemCheck` gained `disk_filesystem` (via `sysinfo::Disks`,
      reusing the same mount-point-prefix-match logic
      `disk_free_bytes` already used -- refactored into a shared
      `matched_disk_for_path` helper) and `disk_filesystem_is_risky`,
      computed once via `nk_core::system_check::
      is_risky_portable_filesystem` so the frontend never carries its
      own copy of what counts as "risky" (a first pass did exactly
      that -- a TS string comparison duplicating the Rust predicate --
      caught and fixed before committing). Real-machine VERIFY: this
      dev machine's own NTFS system drive reports its filesystem name
      as all-caps `"NTFS"`, confirmed via a temporary debug print
      against the real `sysinfo` call (removed after confirming), which
      is why the risk check is case-insensitive. Frontend: the System
      Check screen (the data-directory picker step) shows the detected
      filesystem and, when it's exFAT, the spec's exact warning text.
      Live-verified in the browser dev preview.
- [x] Windows long-path support -- already fully built and verified
      back in Phase 1 (see this file's Phase 1 section: `longPathAware`
      manifest + `nk_core::paths::to_verbatim`, a real >260-char nested
      tempdir test proving it). Wrongly re-listed here as a Phase 9 gap
      without first re-checking Phase 1's own entry; caught and fixed
      by re-reading rather than redoing the work. One real caveat found
      while checking: `to_verbatim` is only actually wired into one
      call site (`disk.rs`'s recursive directory-size scan) -- not
      audited across every other file I/O path (cookie reads, log
      tailing, the settings database, script files, binary extraction).
      Left as-is deliberately: those paths are a small, fixed number of
      segments under the environment data root, not the arbitrarily
      deep recursive trees `directory_size` walks, so the practical
      risk is low -- noted here rather than silently assumed fine.
- [ ] USB speed detection, free space (free space already exists via
      `system_check`; USB speed is new). Checked the real mechanism
      before starting: unlike the exFAT check (one `sysinfo` call),
      this needs a multi-hop WMI join on Windows (`Win32_DiskDrive`'s
      `PNPDeviceID` matched against a USB controller device to read
      `NegotiatedSpeed`) with no clean existing Rust crate for it, plus
      a new dependency, and it's Windows-only with no obvious
      equivalent researched yet for macOS/Linux. The spec itself
      softens this to "if possible." Deliberately deferred rather than
      sunk into right now, given its effort/value ratio is meaningfully
      worse than everything else built this phase -- revisit as its
      own focused task if it's wanted.
- [x] Unclean-shutdown recovery guidance. VERIFY, live (not assumed):
      whether a clean shutdown actually removes bitcoind's own
      `bitcoind.pid` file -- confirmed yes, via a real regtest bitcoind
      started and gracefully stopped
      (`bitcoind_removes_its_own_pid_file_on_a_clean_stop`, nk-proc).
      ord's own pid file needed no live VERIFY: it's Nodekeeper's own
      file (ord writes none itself), and `OrdProcess::stop` already
      removes it explicitly on a clean exit, right there in the same
      module. Added `bitcoind_had_unclean_shutdown`/
      `ord_had_unclean_shutdown` (nk-proc, real tests: no file / a live
      pid / a dead pid) alongside the existing `detect_running_*`
      functions, wired through a new `had_unclean_shutdown(chain)`
      command, checked once on the Dashboard screen's mount and shown
      as a plain-language banner only while the environment isn't
      running yet (the moment it's actually actionable). Live-verified
      in the browser dev preview.
      **Process note, not part of the feature itself**: building this
      feature's real-bitcoind test surfaced a live hang -- a first
      version of the test failed with `StartupTimeout` (missing
      `bitcoin.conf`, fixed by matching `NodeManager::start`'s real
      setup recipe exactly), and because `BitcoindProcess` has no
      `Drop`-based cleanup of its spawned child (a known, pre-existing
      gap -- nothing in this codebase kills child processes if the
      Rust process holding them exits abnormally), the panicking test
      left a real orphaned `bitcoind.exe` running, which appears to
      have stalled this session's own background-command completion
      signal until the orphan was killed by hand. Not a bug in the
      shipped feature -- flagged here because it's a real, reproducible
      gap worth remembering for any future test that can panic after
      spawning a real child process.
- [x] "Safely shut down and eject" button: stop every running
      environment (ord first, then bitcoind), wait for clean exits,
      then confirm it's safe to unplug. Backend: `safe_eject` command
      built on a new shared `stop_every_running_environment` helper,
      extracted from (and now also used by) the tray's "Quit" from
      Phase 8 -- Quit swallows a stop failure (best-effort, the app is
      exiting regardless) where `safe_eject` propagates one (telling
      the user it's safe to unplug when something didn't actually stop
      would risk real data corruption, so a failure has to surface).
      Frontend: a prominent section on the Overview screen, shown only
      when `is_portable_mode()` is true, with a live "Everything has
      stopped -- it's safe to unplug this drive now" confirmation.
      Live-verified in the browser dev preview: starting a node shows
      the button, clicking it stops the node and shows the confirmation
      immediately (not lagging a poll tick behind), and starting
      something again correctly hides the confirmation and brings the
      button back.
      **Follow-up done**: the window-close warning and its Tray
      interaction. VERIFY: `tauri-plugin-dialog`'s `MessageDialogBuilder
      ::show` (async, callback-based -- its `blocking_show` explicitly
      can't run on the main thread, so this is the one usable directly
      inside `on_window_event`'s closure) with `MessageDialogButtons::
      YesNo`. Portable mode's `CloseRequested` handler now branches:
      installed mode keeps Phase 8's hide-to-tray behavior unchanged;
      portable mode instead shows a native Yes/No warning ("closing now
      will stop them so it's safe to unplug") and, on confirmation,
      runs the same graceful stop-everything path as Safe Eject before
      exiting -- never silently hides to the tray and leaves bitcoind
      holding files open on a drive that could be unplugged. Compiled
      and passed the full workspace test suite; the dialog's actual
      on-screen appearance/click behavior falls under this phase's
      standing Windows-only verification limit (compiles + a real
      headless launch with no panic, same as Tray in Phase 8 -- not
      click-tested, since it's outside any webview).
- [ ] Master password unlock at launch, for the encrypted secrets file.
      Checked before starting this: `nk-secrets::encrypted_file`'s
      crypto (Argon2id + XChaCha20-Poly1305, real round-trip/tamper/
      wrong-password tests, Phase 5) is fully built and ready to wire
      up. Deliberately not building the launch-time UI flow yet,
      though: per that module's own doc comment, the secrets file
      "never stores wallet encryption passphrases or mnemonics... for
      settings-adjacent secrets only (e.g. a remote-mode SSH key, once
      that feature exists)" -- and remote mode doesn't exist yet
      (docs/SPEC.md item 9, unstarted). There is currently nothing real
      for a master password to protect, so a launch-time unlock prompt
      today would gate access to an empty store -- speculative UI for a
      need that doesn't exist yet, against CLAUDE.md's scope-discipline
      rule. Revisit once something actually writes to this store.
- [ ] "Prepare a new portable drive" wizard (formatting guidance, copy
      launchers/binaries/runtime, initialize the folder structure) --
      depends on the final folder layout existing first

Security self-review deserves particular attention at the end of this
phase (CLAUDE.md calls out Phases 2/5/7 explicitly, but portable mode's
master-password/secrets-file unlock is exactly the kind of seed/key-
adjacent surface the same discipline should apply to).

## Phase 10M — Mainnet readiness (owner goal: real use on mainnet)

Added 2026-09-26 at the owner's request ("start those steps to implement
what it needs to start using the app on mainnet, put it on plan"). This
phase **comes before the rest of Phase 10**: the installer, signing,
updater, remote mode, services and scheduling are how the app reaches
*other* people; none of them is needed for the owner to run a self-built
portable release exe on their own PC, and none of them makes real funds
safer.

Source: a read-only audit in six domains (wallet & funds, node
lifecycle, install & supply chain, security & privacy, ordinals on
mainnet, verification & rehearsal), one skeptic per domain re-checking
every "done" claim, and a completeness critic. The full ledger -- ~110
items with evidence, gaps, proposed tasks and owner questions -- is
**[docs/MAINNET_AUDIT_2026-09-26.md](docs/MAINNET_AUDIT_2026-09-26.md)**;
ids like `[wallet/encryption-state-checked-at-use]` below refer to it.
**The audit was read-only, so every finding is a lead until reproduced**
(CLAUDE.md VERIFY rule): each task begins by reproducing the finding
against the real binaries, and one that turns out false is recorded as
false in DECISIONS.md, not silently dropped. Several claims came from web
summaries of third-party source (ord's CSP on `/preview`, rune
locking, Core standardness) -- those in particular.

### Where things stand (2026-09-26)

- Only a **debug** build has ever run. No release exe, no installer, no
  signing, no updater. **Mainnet has never been started** through the
  app; signet/testnet4 were only started, given a peer, and stopped (no
  wallet was ever created or funded on either).
- What is solid: the executor/redaction/sensitive-channel machinery, the
  fail-closed verification of the two third-party binaries, the Windows
  child-process handling (proven with a probe and a debug exe), wallet
  create/encrypt/send/inscribe/restore *at library level on tiny regtest
  chains* against real bitcoind 31.1 and ord 0.29.0.
- What the audit says is not: several safety properties **exist only in
  React** (the mainnet confirmation, the preview, the fee guard); a
  failed `encryptwallet` after `ord wallet create` **loses the mnemonic
  and leaves an unencrypted mainnet wallet**; encryption is never checked
  at use; a startup timeout or crash **orphans a running bitcoind** that
  the app can neither stop nor re-attach to; the 60 s / 120 s / 30 s
  budgets were sized on empty chains; inscription previews run scripts
  with no per-inscription opt-in and the sandbox-vs-IPC property was never
  tested in the real webview; the index options the wizard saves may
  never reach `ord server`; rune-bearing outputs are treated as spendable
  when the rune index is off (the mainnet default); nothing is
  documented for the day the app itself is broken.
- **`just check` is not a real-binary gate.** The recipe exports no
  `NK_TEST_BITCOIND` / `NK_TEST_ORD`, and ~19 live tests return early when
  they are unset, so it can be green while the wallet, restore and
  inscribe tests did nothing. (Confirmed: the `Justfile` recipe.) GitHub CI
  has been billing-blocked since 2026-09-24.
- **This PC** (Windows 11, 5.9 GB RAM, one 237 GB volume) had 1.5-4 GB
  free on C: throughout this work and cannot host mainnet; builds fail
  with "no space" at that level. Attach/free a large NTFS drive first.

### Staged definition of "using mainnet"

"Mainnet use" is not one thing; the gates differ. Each stage opens only
when its gates are closed **with recorded evidence** (below), and stops at
any unexplained observation.

- **Stage A -- a mainnet node, no wallet.** bitcoind syncs, ord indexes
  read-only, the Explorer is browsable. Needs G0, G1, G2 and G4.
- **Stage B -- a wallet that only receives** (disposable wallet W-A, a
  tiny amount, seed written down). Needs Stage A + G3, G5.
- **Stage C -- spending**: a tiny self-send, the restore drill, a sweep
  to the long-term wallet W-B. Needs Stage B + G7.
- **Stage D -- inscriptions and runes**: a tiny inscription; then raise
  the cap in steps. Needs Stage C + G9.

Value caps, abort rules and who holds which seed are owner decisions
(D12). Claude never sees a mainnet seed or passphrase.

### Rules for this phase

- **A box is ticked only with evidence**: the commit hash it was done in,
  the test names, *executed vs skipped* counts for live tests, and -- for
  anything that runs in the app -- the hash of the exe it ran in. A
  checkbox is not proof (the audit found ticks that were wrong).
- The exe the owner runs must be the exe that was tested: build from a
  clean, tagged commit (`RC<n>`), record its SHA-256.
- ⛔ marks a STOP AND ASK task (touches seeds, wallet encryption,
  graceful ord shutdown, fund movement, or is beyond the spec): stop, put
  the options and trade-offs to the owner, wait.
- Claude's coding work is serial (one task -> gate -> commit); the real
  parallelism is Claude's work vs the owner's actions and wall-clock time
  (the sync). Start the sync as early as G1/G2 allow.
- Do not add a generic timeout/kill to signing or rescan commands: a kill
  between ord's commit and reveal, or during a restore rescan, is the worst
  state. Timeouts are per operation class (D7-related).

### Owner decisions and actions (batched -- re-ask per gate, not all at once)

Recommendations are Claude's; none is taken until the owner answers.

- **D1 -- Step 1(b).** *Already answered* ("do what you believe better"):
  skip the unlock for an **unencrypted wallet on a non-mainnet chain**,
  fail closed on mainnet. Plan detail from the audit, not a new question:
  the *rehearsal* wallets on signet/testnet4 are created **with a
  passphrase**, so the rehearsal exercises the real mainnet
  unlock -> sign -> relock path.
- **D2 -- Scripts in inscription previews.** Spec (Foundation D): off by
  default, per-inscription "Render interactive content". Today they run
  automatically. Recommend: implement the spec.
- **D3 -- Trust in the pinned binaries.** The builder-key bundle and the
  four ord hashes were fetched by Claude, once, over one channel. Owner
  action: verify the fingerprints (bitcoin-core/guix.sigs) and the ord
  SHA-256 values **from a second device**. Also: is hash-only trust in ord
  enough for real funds (ord has no maintainer signature), or does mainnet
  wait for a source review / build-from-source?
- **D4 -- Network exposure. Answered 2026-09-27: outbound-only.** Verified
  live that Core defaults to listening on all interfaces and mapping the
  port on the router (NAT-PMP); every transaction would otherwise be
  broadcast from the owner's IP. `listen=0`/`natpmp=0` are now in
  `generate_bitcoin_conf` for every chain (DECISIONS.md, "Node-lifecycle
  hardening for the public preview"), proven live. Tor (Phase 10 step 5) is
  unchanged -- still a later step, not required for this.
- **D5 -- ord index options on mainnet** (permanent per environment; a
  wrong choice is a multi-day reindex). Recommend `--index-runes` **on**
  (so rune-bearing outputs are protected), sats and addresses off unless
  needed. Decide **before ord's first mainnet start**.
- **D6 -- Where mainnet lives.** An external NTFS SSD in portable layout
  (a `config` folder next to the exe), size chosen after the G2 smoke
  run's measurements (the audit's estimate: about 1 TB or more with
  txindex and the ord index). Is there an existing synced Core datadir to
  reuse? Is another Bitcoin Core running on this PC (ports 8332/8333)?
- **D7 -- Stop policy.** ⛔ (graceful ord shutdown): ord stop timeout 120 s
  per spec (today 30 s), bitcoind stop budget from the G2 measurement,
  never a force-kill, and after a failed stop the app says so loudly
  instead of "Stopped".
- **D8 -- Create/restore semantics.** ⛔ (seeds, encryption): (a) if
  `encryptwallet` fails after create, show the mnemonic anyway with a hard
  "not encrypted, do not fund" state and offer retry; (b) restore
  ordering -- restore with `--timestamp now`, encrypt immediately, then
  rescan, instead of encrypting only after a multi-hour rescan; (c)
  minimum passphrase length (recommend 12) and "the passphrase is not
  stored" copy; (d) gate the receive address on a persisted
  "seed confirmed" marker.
- **D9 -- Spending guards.** Hard fee ceilings (sat/vB and absolute sats);
  whether a remembered passphrase is allowed on mainnet (recommend off or
  5 minutes); whether the mainnet console may build/broadcast a spend at
  all (recommend: not on the ord wallet) and whether `--no-sync` is
  refused on fund-moving commands (recommend: yes).
- **D10 -- Inscription transfers and stuck transactions.** Build the GUI
  "send this inscription" before Stage D, or hold inscriptions until it
  exists (recommend: hold). Stuck/low-fee transactions: documentation
  only, or an in-app "accelerate" (scope beyond the spec -- ask first).
- **D11 -- Release build.** A plain unsigned portable exe is enough for the
  owner's own PC. `panic = "abort"` kills the app with no stop and no log
  (bitcoind/ord keep running): recommend `unwind` + a panic hook + an
  application log file until crash recovery exists. GitHub Actions
  billing: fix it, or accept local-only gating explicitly.
- **D12 -- Caps and people.** The value cap per stage, the abort rule, who
  independently reviews the money path (Claude reviewing its own code is
  not independence), and a disposable seed (W-A) for the drills.
- **Owner actions (nobody else can do these):** attach/free a large NTFS
  SSD and free space on C: (G0); approve real P2P traffic on signet/
  testnet4 and then mainnet; Windows Defender exclusion for the data
  folder and power/Windows-Update settings for the days-long sync; hold the
  seed and passphrase; the independent pin check (D3); go/no-go at each
  stage.

### Gates

**G0 -- Baseline, hardware, decisions** (start now; the owner is the
bottleneck)
- [x] Disk: the owner attached a **1.86 TB NTFS disk (D:)** on 2026-09-26
      and told Claude to use it for builds and tests. Claude builds under
      `D:\_claude-dev\` only (`CARGO_TARGET_DIR`; removable) and does
      **not** install Core/ord there: **the owner runs the app themself to
      install Core and ord onto D:** -- the first real run of the wizard's
      download + verify path (record what they see). C: is still nearly
      full (about 5 GB free).
- [ ] D3, D4, D5 and D11 answered (they gate G1, G2); D6 is partly answered
      (mainnet lives on D:; portable layout or a data root on D:, and how big
      an index, still to say).
- [ ] Commit Step 1(a) (finished: refuse/scrub/erase; console secret
      arguments hidden by position and by known-method list) after the
      full gate **with the real binaries** (see next item), and tag `RC0`.
      `[security/commit-console-secret-fix]`
- [x] Make the real-binary tests mandatory (done 2026-09-26, DECISIONS.md
      "The quality gate runs the live tests"): `just check` sets
      `NK_TEST_BITCOIND` / `NK_TEST_ORD` from the verified cache and
      `NK_REQUIRE_LIVE=1`, so a missing binary fails instead of skipping.
      `just` itself is not installed here (not run through it).
      `[verification/ci-and-silent-skips]`
- [ ] Re-baseline: every audit claim citing a line number or compile state
      is re-checked on the committed tree before work starts on it.
- [ ] Defer Phase 10 steps 6-11 (see below); record in this file.

**G1 -- Node lifecycle hardening** (before any long sync; no wallet
dependency; regtest first, then the release exe)
- [ ] Startup: readiness wait aware of warm-up ("Loading block index...",
      -28) instead of a hard 60 s; a timeout must not orphan a running
      bitcoind. Evidence already in hand: the live tests in `nk-testkit`
      hit `Bitcoind(StartupTimeout)` intermittently when several start
      bitcoind at once (3 of 6 full workspace runs on 2026-09-26, a
      different test each time, each passes alone). **Each such failure
      also left a running `bitcoind` behind** (four were found after the
      gate, killed by hand): `RegtestFixture` must kill what it started
      when the readiness wait fails, or the orphans pile up and make the
      next run flakier -- the same bug the app itself has.
      `[node/startup-warmup-budget-and-orphaned-child]`
- [ ] Stale `.cookie` / `bitcoind.pid` after a crash or reboot: re-read
      the cookie after the new process writes it; verify a pid file names
      *our* process. Real test: kill -9, restart on the same datadir.
      `[node/restart-after-crash-stale-cookie-pid]`
- [ ] **Attach / adopt / "shut down safely"** for a running or orphaned
      bitcoind (spec Foundation C, still unbuilt) with a clear ownership
      rule: never stop a node the app did not start unless the owner says
      so. ⛔ the ord half (it cannot be signalled without its original
      console -- graceful shutdown rule). `[node/attach-adopt-orphan-processes]`
- [ ] Stop path: retry the stop RPC (warm-up -28, full RPC queue), "Stopped"
      only when the process is gone, loud failure text that does not say
      "Task Manager"; Quit/close paths that verify or say "not stopped".
      ⛔ D7. `[node/stop-path-at-mainnet-scale]`, skeptic items on
      portable close.
- [ ] Child-death detection; ord stdout/stderr captured to a log (with a
      viewer); corruption/reindex guidance (⛔ exposing `-reindex`).
      `[node/child-death-detection-and-ord-logs]`,
      `[node/corruption-recovery-reindex]`
- [ ] Status polling that cannot hurt an IBD: RPC timeout, in-flight guard,
      no recursive walk of a 500 GB tree every 3 s; history rows of
      background polls do not evict the real commands.
      `[node/polling-load-vs-ibd]`
- [ ] Disk gating: capacity verdict before a mainnet start, warning with
      real headroom, `DiskFull` produced; refuse a mainnet start on the OS
      drive. `[node/disk-space-gating]`, `[install/mainnet-storage]`,
      `[verification/disk-space-and-data-root]`
- [ ] Config that the owner can control: dbcache (concurrent-environment
      accounting), listen/natpmp per D4, ports; record the 31.1 defaults
      (VERIFY: `bitcoind -help-debug`). `[node/dbcache-ram-limits]`,
      `[node/firewall-defender-p2p-policy]`
- [ ] ord sequencing: not started during IBD / before txindex is caught up
      (spec default: auto-start when ready); ord launch config actually
      receives the saved index options, an explicit chain flag on mainnet,
      `ORD_*` environment scrubbed. **Blocker candidate:**
      `[ordinals/ord-launch-config-not-controlled]` (reproduce first),
      `[node/ord-start-after-ibd]`
- [ ] Release exe: build from the tagged tree; run it once through the
      whole node lifecycle (Start/Stop/Restart/Safe Eject, kill -9 and
      restart, the console-less child handling); D11 (`unwind`, panic hook,
      log file). `[install/release-build-unproven]`,
      `[verification/release-candidate-freeze]`
- [ ] Binary integrity at run time: re-hash `bitcoind`/`ord` before spawn
      (cache by size+mtime), refuse on mismatch; truncated extraction is
      not "verified". `[install/post-verify-integrity]`

**G2 -- Bounded mainnet smoke, then the owner's IBD** (Stage A)
- [ ] With the release exe, owner present, real P2P approved: start
      mainnet from the app's own config, watch headers/peers, graceful
      stop, kill -9 + restart, stop + start over the same datadir; record
      RAM/disk and what the stop/startup budgets really are (D7).
      `[node/mainnet-never-run-rehearsal]`, `[install/mainnet-first-start-smoke]`
- [ ] Live sockets: RPC and ord listen on loopback only (netstat), P2P
      exposure as decided in D4. `[node/rpc-ord-bind-localhost-cookie]`
- [ ] **[MANUAL]** Owner starts the full IBD on the SSD (days). Sleep and
      Windows Update off for the duration; Defender exclusion; do not run
      heavy live tests meanwhile unless G2 measured the headroom.
- [ ] ord on mainnet only after Core is out of IBD and txindex caught up;
      D5 applied; first ord start reproduces "ord has never indexed a real
      chain" findings (index size/time/RAM, stop time under load).
      `[verification/real-chain-ord-indexing]`

**G4 -- Hostile content and integrity** (before the Explorer/gallery is
first opened on mainnet -- anyone can send an inscription to any address)
- [ ] D2: scripts off by default, per-inscription opt-in; same for the
      Explorer iframe. `[ordinals/hostile-inscription-preview-and-ipc-isolation]`
- [ ] **Live test in the real webview** (release exe) with a hostile
      HTML+SVG inscription: parent/top access, `__TAURI_INTERNALS__.invoke`,
      fetch to 127.0.0.1 RPC/ord ports and to an external host, navigation.
      VERIFY ord's real `/preview` and `/content` response headers
      (`curl -I`) -- the recorded CSP rationale may be wrong.
      `[security/webview-boundary-real-build]`, `[verification/webview-sandbox-real-test]`
- [ ] `set_setting`: an allowlist of keys; binary and data-root paths only
      through validated commands (a compromised webview can otherwise point
      `bitcoind_path`/`ord_path` at any exe). `[install/settings-ipc-binary-hole]`
- [ ] IPC surface: no arbitrary-file / URL-open / process primitives
      reachable with one `invoke` (`inscribe_file_preview`, opener, script
      args). `[security/ipc-primitives-hardening]`
- [ ] Loopback RPC/ord clients ignore proxy environment variables
      (`no_proxy` / builder `.no_proxy()`), so a passphrase can never be
      sent to a proxy (VERIFY for the pinned reqwest).
- [ ] Dependency hygiene: `cargo audit` / `cargo deny` / `npm audit`,
      locked builds from a clean tag. `[security/dependency-and-build-hygiene]`
- [ ] **[MANUAL]** D3: owner's independent check of the builder keys and ord
      hashes; Claude adds negative tests proving verification failure
      refuses to run (wiring, not just helpers).
      `[install/core-keys-provenance]`, `[install/ord-hash-provenance]`

**G3 -- Custody: wallet creation and recovery** (Stage B; regtest, then a
public test chain with an **encrypted** wallet)
- [x] Step 1(b) (done 2026-09-26, Phase 10 step 1(b)): shared
      `with_wallet_unlocked`, encryption asked of Core, an unencrypted
      mainnet wallet refused on the guided screens **and the console's**
      `ord wallet` signing commands, `WALLET_NOT_ENCRYPTED`, skip-unlock only for
      an unencrypted non-mainnet wallet (D1).
- [ ] What 1(b) left for here: a **guided "encrypt this wallet" action** (the
      text points at the Console's `encryptwallet` today) and the wallet's
      encryption state shown on the Wallet screen, so a refusal appears
      before a form is filled in; surfacing a failed re-lock; the live
      tests that skip silently. `[wallet/encryption-state-checked-at-use]`,
      `[security/mainnet-encrypted-wallet-invariant]`
- [ ] ⛔ D8: atomic create/restore as one testable function; encryption
      failure never drops the mnemonic; restore ordering; minimum passphrase
      + immediate unlock/lock self-test; an "encryption pending" state that
      blocks every signing command; tests for the mainnet gate
      (`require_encryption_passphrase_on_mainnet` has none).
      `[wallet/mainnet-encrypt-create-restore]`,
      `[verification/mainnet-encryption-gate-and-atomicity]`
- [ ] ⛔ Seed view: "show my words again" from the confirm step; persisted
      non-secret `seed_confirmed:<chain>` marker; receive address gated on
      it (D8); content protection while the seed is on screen; redacting
      `Debug` for the mnemonic result. `[wallet/seed-display-and-backup-confirmation]`
- [ ] Prove recoverability: create -> encrypt -> receive/change addresses
      **after** encryption -> fund -> inscribe on the encrypted wallet ->
      restore into a fresh datadir; compare descriptors, balance,
      **inscriptions and the cardinal/ordinal split**; record commands in
      DECISIONS.md. `[wallet/encryption-vs-seed-recoverability-proof]`,
      `[verification/restore-from-seed-fresh-environment]`
- [ ] Restore form copy: no BIP39-passphrase support, ord's derivation, what
      "has history" does; a "rescan" action after a "now" restore; progress
      and cancel for a long rescan. `[wallet/seed-restore-drill-and-mainnet-scale]`
- [ ] Passphrase lifecycle: "Lock now", visible unlock state, surface
      `walletlock` failures, setting for the remember default (D9).
      `[wallet/passphrase-lifecycle]`
- [ ] Encrypted-wallet inscribe/batch/reinscribe run live (all existing
      live inscribe tests use an unencrypted wallet), and a Tauri-command
      level test of unlock -> run -> relock including failure paths.
- [ ] Public-test-chain rehearsal in the release exe through the GUI, with
      an **encrypted** wallet on signet or testnet4: create -> confirm ->
      faucet -> receive -> restore into a second environment. Needs the
      chain synced (start early; real P2P approval) and test coins.
      `[wallet/mainnet-rehearsal-and-release-exe-run]`
- [ ] Documentation: an emergency runbook rehearsed on regtest **with
      Nodekeeper closed** (wallet location, running bundled `bitcoin-cli`/
      `ord` by hand, restoring the seed with stock ord, stuck transaction,
      what never to delete) and the owner's operating notes (sleep, power,
      Defender, safe unplug). `[wallet/emergency-runbook-and-user-docs]`,
      `[install/end-user-docs]`
- [ ] Backups, first half of Phase 10 step 4: a **public-descriptor** backup
      with a no-private-material guard and a positive-control test (the
      encrypted private backup and restore stay in the deferred list).

**G5 -- Independent review and canary scan** (before Stage B)
- [ ] Secret canary harness (first half of Phase 10 step 3): fake
      mnemonic/passphrase/cookie/xprv scanned across the event stream, the
      raw SQLite file, Core `debug.log`, the ord data dir, the WebView2
      profile and `%TEMP%` after create/restore/unlock flows on a test
      chain, with a negative control. `[security/secret-leak-canary-scan]`
- [ ] Pre-mainnet security self-review over the **whole IPC surface** (a
      compromised-frontend threat model), Phases 8-10M included.
      `[security/security-self-review-pre-mainnet]`
- [ ] **[MANUAL]** D12: someone other than the author reviews the money
      path (wallet commands, confirm flow, fee guard) -- Claude reviewing
      Claude's code is not independence.

**Stage B (owner-run)** -- create disposable wallet W-A in the GUI, write
the seed down, receive one tiny amount, confirm on a block explorer. Claude
never sees the seed or passphrase. **[MANUAL]**

**G7 -- Spending safety** (before Stage C)
- [ ] Single-flight signing: a per-chain in-flight guard (typed
      `WALLET_BUSY`), `ConfirmDialog` busy state, exactly one broadcast on a
      double click; refresh balance/history after a send; "outcome unknown"
      guidance; quit/Safe Eject/stop blocked or warned while a signing
      command is running. `[wallet/sign-action-single-flight-and-stale-view]`
- [ ] `ConfirmDialog`: the mainnet acknowledgement resets on every open;
      fee, rate, total and any fee warning shown *inside* the dialog;
      environment name genuinely large. `[wallet/shared-confirm-dialog-mainnet-step]`
- [ ] Backend enforcement: `wallet_send`/inscribe/batch require a
      `preview_id` bound to (chain, address, asset, rate, dry-run fee) and,
      on mainnet, an acknowledgement token from the confirmed dialog; the
      dry-run is repeated just before signing and aborts if the fee moved.
      The preview shows what ord actually built (decoded PSBT: recipient,
      amount, change, fee), not only the fee.
      `[wallet/backend-enforcement-of-preview-and-mainnet-confirm]`,
      `[security/backend-enforced-fund-confirmation]`
- [ ] Fee guard and address guards: one module (TS + Rust mirror) with the
      unit tests the spec requires, D9 ceilings enforced in Rust, estimate-
      relative warnings, wrong-network tests in both directions, grouped
      address display on mainnet. `[wallet/fee-guard-and-address-guards]`,
      `[ordinals/fee-rate-source-and-guard-mainnet]`
- [ ] Console spend policy (D9): fund-affecting RPCs
      (`walletprocesspsbt`, `signrawtransactionwithwallet`,
      `sendrawtransaction`, `submitpackage`, `lockunspent`,
      `abandontransaction`) blocked or acknowledgement-gated on the ord
      wallet; `--no-sync` refused on fund-moving commands; real-bitcoind
      test that the console refuses `sendtoaddress`/`psbtbumpfee`.
      `[wallet/console-and-raw-command-fund-safety]`, `[security/console-spend-and-unlock-policy]`
- [ ] Stuck-transaction procedure (D10): VERIFY whether ord's transactions
      signal RBF and whether `bumpfee` is safe for plain sends but not
      commit/reveal; write the procedure; fee rate and age of pending
      transactions shown. `[wallet/stuck-transaction-bump-rbf]`
- [ ] Regtest delta tests: no fee estimates, a block after every action,
      all index options on, no relay-policy rejections, no rune outputs,
      no hostile content -- each difference from mainnet gets a test or a
      named rung. `[verification/mainnet-only-code-paths-unexercised]`

**Stage C (owner-run)** -- tiny self-send with the PSBT decoded and
checked with a second tool; restore W-A's seed into a second environment
(the drill on the real chain; a full rescan takes hours: observe progress and
timeouts); sweep to the long-term wallet W-B. **[MANUAL]**

**G9 -- Ordinals safety** (before Stage D)
- [ ] D5 applied and verified: rune-bearing outputs are protected (rune
      index on) or fund-moving actions are hard-blocked with a clear
      message; `wallet_balance` accounts for `runic`.
      `[ordinals/rune-utxo-protection-needs-rune-index]`
- [ ] **Inscription-safety drill on regtest:** inscribe, then send
      all-but-dust BTC and assert the inscription's location is unchanged;
      also with the reveal unmined and after a bitcoind restart; a plain
      BTC send never moves an inscribed sat. VERIFY ord 0.29.0's behaviour
      rather than assuming. `[verification/inscription-safety-drill]`,
      `[wallet/inscription-and-rune-utxo-protection]`
- [ ] D10: GUI "send this inscription" (with the non-Taproot / exchange
      warning) **or** an explicit "no inscription moves until it exists"
      rule. `[wallet/send-inscription-gui]`
- [ ] Inscribe: cost preview includes postage, size warning that fires,
      "commit sent but reveal failed" guidance, reinscribe correct with the
      sat index off. `[ordinals/inscribe-cost-breakdown-postage-size-warning]`,
      `[ordinals/inscribe-partial-failure-commit-reveal]`,
      `[ordinals/reinscribe-mainnet-default-sat-index-off]`

**Stage D (owner-run)** -- one tiny inscription, then raise the cap in
steps (D12). **[MANUAL]**

### Not on the mainnet critical path (deferred until after Stage C)

Phase 10 steps **6** (script scheduling), **7** (update checker), **8**
(MSI/installer -- the owner runs a portable release exe; keep the
"user docs skeleton" only), **9** (signed self-updates), **10** (remote
mode) and **11** (OS services); code signing; the encrypted *private*
backup; the rest of the diagnostics export; Tor (unless D4 makes it a
prerequisite); macOS/Linux verification. They are needed to hand the app
to other people, not to run it safely on one PC.

## Phase 10R — Public preview release (owner goal: "anyone can use it")

Added 2026-09-27. The owner asked to publish a release so that anyone can use the
app. Because Phase 10M still lists blockers that could lose someone's bitcoin,
the answer is a **public preview**, not the full release: an MSI installer and a
portable zip, built on GitHub, unsigned, **not for real money**, with mainnet
wallets blocked. Owner decisions: DECISIONS.md, "Public preview release: owner
decisions". Facts below come from a sourced scoping pass (official Tauri, GitHub,
WiX and legal sources; the raw findings are in this session's workflow output) --
they are **leads to reproduce**, and the first real release build and run happen
on GitHub, so several items can only be proven there.

### Choices taken from the scoping pass (reversible; tell the owner)

- **The MSI is per-machine** (Program Files, an admin prompt): Tauri's stock MSI
  cannot be per-user. The earlier "MSI, per-user" wording in this file is
  superseded; the release notes say so.
- **Numeric version, `preview` by other means.** The MSI build hard-fails on
  `0.1.0-preview.1`, so the app version stays `0.1.0`, `0.1.1`, ... and "preview"
  is the GitHub pre-release flag, the release title, the in-app banner and the
  build flavor. Each release bumps the first three fields (Windows Installer
  ignores the fourth). A test keeps `tauri.conf.json`, `Cargo.toml` and
  `ui/package.json` in step, and the workflow checks tag == version.
- **The preview flavor is a compile-time Cargo feature**, `mainnet-wallets`, **off
  by default**, so a plain `tauri build` *is* the preview and only a deliberate
  `--features mainnet-wallets` (the owner's own full build) unlocks mainnet
  wallets. Nothing the webview or a setting can change. The block is enforced in
  **Rust**; the UI panel only explains it.
- **The mainnet console in the preview is read-only commands only** (which also
  refuses `stop`/`addnode` there): a deny-list of wallet methods would miss
  `walletprocesspsbt`, `sendrawtransaction`, `submitpackage`, future RPCs.
- **The mainnet node and Explorer stay available** (owner decision, 2026-09-27,
  against the recommendation to gate them too); only mainnet *wallet*
  create/restore/sign/receive is blocked. Because of this, **Phase 10M's G1
  (node-lifecycle hardening: the readiness-wait/orphan/stale-cookie loop, and a
  mainnet Start confirmation with a real disk check) becomes a precondition for
  the release tag**, not a later step -- a stranger's first click on the Mainnet
  Dashboard is a Start button that hits exactly that loop today.
- **The VC++ runtime `ord.exe` needs is detected, not bundled**: a plain-language
  message and a link to Microsoft's own page if it's missing, no extra download
  shipped inside Nodekeeper. `bitcoind.exe` does not need it.
- **WebView2: Tauri's default `downloadBootstrapper`** (Windows 11 already has the
  runtime; the download from Microsoft only happens where it is missing) -- and
  the portable zip needs the runtime installed. Both go in the README's network
  section.
- **Data location:** installed mode moves from Roaming to **Local AppData** (a
  Roaming profile can sync; this is hundreds of GB) *before* the first release,
  because moving it later strands users.
- **The Tauri logo is replaced** (a trademark-policy problem, not only cosmetic)
  by an original placeholder mark; the owner can supply artwork later.
- **Third-party notices are generated in CI** (`cargo-about`/`cargo-deny`, the
  npm runtime closure) and shipped in the MSI and the zip, so nothing has to be
  installed on the owner's PC.

### Tasks, in order

- [ ] **R0 -- Mainnet node-lifecycle precondition** (moved up 2026-09-27: the
      mainnet node stays available in the preview, so this gates the tag).
      From Phase 10M, G1:
  - [x] The readiness wait no longer orphans a running `bitcoind`/`ord` on
        timeout (it kills the process and removes the pid file), and no longer
        hangs past its own deadline if a stuck RPC/HTTP connection never
        answers; the wait is now warm-up aware (`-28` extends the deadline, up
        to a cap). Done 2026-09-27 -- DECISIONS.md "Node-lifecycle hardening
        for the public preview". 8 new tests across `nk-proc` and
        `nk-testkit`, each with a negative control against a real or a
        scripted node.
  - [x] bitcoind's P2P `listen`/`natpmp` defaults VERIFIED live for 31.1 and
        recorded; owner chose outbound-only (`listen=0`, `natpmp=0`, now in
        `generate_bitcoin_conf`), proven live against a real node (RPC still
        works; the P2P port refuses a connection). Same DECISIONS.md section.
        **Still owed**: re-run the signet/testnet4 smoke example
        (`chain_smoke_test.rs`) against this change -- it waits for an
        *outbound* peer, which `listen=0` shouldn't affect, but that hasn't
        been re-checked live.
  - [ ] A stale `.cookie`/pid after a crash must not lock the user out with no
        remedy (a different bug from the orphan-on-timeout one just fixed: what
        happens on the *next* start after an *earlier* unclean exit).
  - [ ] Attach/adopt for a bitcoind or ord the app didn't start; the stop-path
        budget and swallowed stop-RPC failures.
  - [ ] Mainnet **Start** needs a confirmation (the 1 TB+ / multi-day figures)
        plus a real free-space check before it runs, since a stranger's first
        click on the Mainnet Dashboard is exactly that button.
- [ ] **R1 -- Preview safeguards** (the reason it is safe to publish). Feature
      flag and a read-only `get_build_info`; one Rust gate called first in
      `create_wallet`, `restore_wallet`, `wallet_send`, `wallet_inscribe`,
      `wallet_inscribe_batch`, `wallet_receive_address` and both branches of
      `console_run` (`console_classify` gains the chain, so the UI refuses up
      front, dry-run included); new error code `PREVIEW_MAINNET_BLOCKED`; a
      **manifest test that fails when a new command is added without being
      classified or without the gate**; defense in depth `disablewallet=1` for the
      preview's mainnet `bitcoind` (VERIFY live that `ord server` still indexes);
      persistent banner + a re-appearing acknowledgement (stored as the running
      version, written only by its own command); explanation panels so blocked is
      never a dead end; the preview lands on Regtest, not on a live Mainnet
      Dashboard; a confirmation and a free-space check before a mainnet **Start**
      (multi-day, 1 TB+). Also from the audit's G4, because the mainnet Explorer
      stays open to hostile inscription content: a **`get_setting`/`set_setting`
      allowlist** (today one call sets `bitcoind_path` to any exe), a
      `binaries_status` command instead of the UI reading the paths, `opener:default`
      removed, `inscribe_file_preview` limited to dialog-chosen paths. VC++
      runtime: detect at ord-start time and show a plain message linking to
      Microsoft's redistributable page (owner decision, 2026-09-27) --
      `bitcoind.exe` doesn't need it, `ord.exe` does.
- [ ] **R2 -- Crash handling and a log** (D11, taken): `panic = "unwind"`, a panic
      hook that shows a message box and writes a log/crash file, the startup
      `.expect(...)`s routed through it, `eprintln!`s that vanish in a GUI exe
      replaced by a small application log (no secrets in it). A release-only
      failure must not be silent.
- [ ] **R3 -- First-run and branding**: Local AppData default; a one-time notice
      when closing to the tray leaves `bitcoind`/`ord` running; copy fixes
      (`walletNotEncrypted` sends users to the Console, a mainnet dead end in
      the preview; `scripts.warning`; the "preview" collision in the inscribe
      hint); original icons, favicon, remove the template SVGs and the Vite
      README; the version shown in the UI.
- [ ] **R4 -- Legal and user docs**: `LICENSE` (MIT, "Copyright (c) 2026
      sixoBitmap"); `README.md` for users (what it is, PREVIEW, intended networks,
      install, where data lives, the unsigned-build warnings, the exact network
      destinations, how to verify a download); `SECURITY.md` (private
      vulnerability reporting); notices generation and the "not affiliated with
      Bitcoin Core / ord" line; release-notes template.
- [ ] **R5 -- Packaging config**: `bundle` block (MSI only, publisher, copyright,
      license file, pinned WiX `upgradeCode`, `allowDowngrades: false`,
      `createUpdaterArtifacts: false`); version-sync test; a guard that keeps the
      updater off; the portable-zip assembly (exe + an empty `config` folder that
      survives zipping, with a placeholder file) and its test.
- [ ] **R6 -- Release workflow** (`release.yml`): trigger on a `v*` tag plus a
      manual dry run; three jobs with least-privilege tokens (gate: read-only;
      build: contents read, attestations; publish: contents write only, `gh release
      create --draft --prerelease`); every third-party action pinned to a commit
      SHA (re-verified by API, annotated tags dereferenced); `--locked`/`npm ci`;
      no `--all-features` and a marker check that the built exe is the preview
      flavor; SHA256SUMS; build-provenance attestation; the portable zip is
      extracted and the exe started in CI as the **first ever release-exe run**;
      the quality gate runs first. `ci.yml` gets `NK_REQUIRE_LIVE`, read-only
      permissions and `--locked`. ⛔ the fetch examples' cache short-circuit
      (they return early without re-verifying) touches the verification path:
      ask before changing. The attestation step runs in its own job that only
      downloads the built artifacts and executes no repo code (the build job
      runs ~600 crates' worth of build scripts and should not also hold
      `attestations:write`). Add `cargo deny check advisories` (or cargo-audit)
      and `npm audit --omit=dev` to the gate, recorded in DECISIONS.md.
- [ ] **R7 -- What can be checked on this PC**: unit and live tests for everything
      above; the debug build in a real portable folder; the MSI's contents
      (`msiexec /a` extraction, no system change) if a debug MSI can be built
      (WiX is fetched by the bundler). A real install/uninstall needs the
      owner's OK; the release exe and the MSI on a clean machine are proven only
      by the GitHub run and the owner's test.

### Owner actions before and at publication

Only the owner can do these; the plan lists them so nothing is a surprise.

- [ ] **Decide what the public sees in the history.** Every commit (113) carries
      `gperan6@gmail.com`; the tree includes `docs/MAINNET_AUDIT_2026-09-26.md`
      (114 items, 43 "blocker") and the build prompt `nodekeeper-prompt.txt`.
      Options: keep everything (honest), turn on GitHub's "keep my email private"
      for future commits, or rewrite history (destructive; only with the owner's
      explicit go). A real secret scanner (gitleaks/trufflehog) should be run
      over the history first -- a regex scan found nothing, which is not the same.
- [ ] Make the repository **public**, then: confirm a trivial workflow runs
      (billing); enable **private vulnerability reporting**; set the default
      workflow token to read-only; add a `v*` tag ruleset; enable **release
      immutability before the first publish** (not retroactive).
- [ ] Run the release workflow as a manual dry run; download the artifacts; test
      the MSI and the zip on a clean Windows 11 profile; inspect the **draft**
      pre-release; publish it; verify the published download as a user would
      (checksum and `gh attestation verify`).
- [ ] Name/trademark search for "Nodekeeper", the icon artwork, and the export-
      control question -- counsel-type items; nothing here is legal advice.
- [ ] Scan the built MSI, zip and exe with Windows Defender (and ideally
      VirusTotal) before publishing -- an unsigned exe that downloads binaries
      and sends CTRL_BREAK to children is a plausible false-positive target --
      and have a Microsoft false-positive submission ready.
- [ ] Account hardening before going public: 2FA/passkeys on the GitHub account,
      a ruleset on `master` and on `v*` tags, and a release-notes line that says
      "download only from this repository's Releases page" (hashes and
      attestations prove integrity, not that the account wasn't compromised).
- [ ] Write a short yank/rollback runbook (no updater exists yet): a bad release
      can only be re-worded, marked more clearly pre-release, or deleted (which
      burns the tag once immutability is on) -- decide the procedure before it's
      needed, not during an incident.
- Later, not for the preview: a code-signing certificate (SmartScreen "unknown
  publisher" and Windows Smart App Control can still block an unsigned build).

## Phase 10 — Extras and release

> **Order changed 2026-09-26 -- see "Phase 10M -- Mainnet readiness"
> above.** The owner's goal is real use on mainnet on their own PC, so
> Phase 10M comes first. Of the steps below: Step 1(b) is task G3, step
> 3's canary harness is G5, step 4's public-descriptor backup is G3, and
> steps 6-11 are **deferred** until after Stage C. Steps 3, 4 and 5 keep
> their remaining parts here.

Scoped 2026-09-26 by a read-only analysis: one analyst per spec feature,
then a cross-cutting critic that re-checked claims against the repo and
ordered the work. The order below is the critic's. Phase 10 **cannot be
fully closed from this Windows dev machine** (systemd/launchd, a real
SSH host, a real Tor daemon, macOS/Linux installers, code-signing certs,
and GitHub Actions billing (blocked since 2026-09-24) are all outside
it), so the plan is the same as earlier phases: close what is verifiable
here, record the limits in DECISIONS.md, leave [MANUAL] boxes unticked,
and don't commit unverifiable cfg-gated macOS/Linux code (Phase 9
precedent). Several analyst claims about external tools (updater plugin,
russh, Tor PROTOCOLINFO, ord index schema, WebView2 env vars, MSI
pre-release limits) came from web summaries -- treat as leads, VERIFY
live before relying on them (CLAUDE.md).

Done when (docs/SPEC.md):
- [ ] [CI] installers build; the support bundle contains no secrets
- [ ] [CI] a signed test update installs and verifies; an unsigned one
      is rejected
- [ ] [CI] a default backup contains no private descriptors; an
      encrypted private backup restores only with the correct backup
      password
- [ ] [MANUAL] remote mode works through an SSH tunnel; Tor toggle works
      with a system Tor daemon

### Tasks, in order

- [x] **Step 0 -- Windows release-build process handling** (gates
      packaging, services, updater, scheduling, safe-eject). Found by
      the critic, not by any analyst: the release exe is a GUI-subsystem
      program, so `ord`'s CTRL_BREAK stop and every child spawn behave
      differently from every build tested so far. **Done 2026-09-26** --
      full write-up, commands, outputs, negative controls and the review
      in DECISIONS.md "Windows release builds: console-less process
      handling"; the one item still owed is listed last.
  - [x] VERIFIED live: on unmodified code a console-less parent **cannot
        stop ord** (`GenerateConsoleCtrlEvent` -> "The handle is
        invalid", ord orphaned) and **every child gets a visible console
        window**. bitcoind's RPC stop is unaffected.
  - [x] STOP AND ASK answered: owner approved option A (hidden consoles +
        attach-and-signal ord stop, with a runtime console/no-console
        split) and stop-everything continuing past failures.
  - [x] Implemented: `nk-exec/src/console.rs`, `nk-proc/src/console.rs`
        (+ ord/bitcoind spawn and stop changes, standard handles saved
        and restored around the attach, null stdin), `NodeManager::
        stop_everything` (keeps going, verifies pid files afterwards,
        single-flight, `any_running()` counts stops in flight), Safe
        Eject's specific failure message.
  - [x] Tests: probe + helper bins, driver, and the Windows regression
        test `nk-testkit/tests/console_less.rs` (respawn after the stop,
        NULL std handles, executor child without a console window,
        graceful exit 0, no visible window for bitcoind/ord), each fix
        shown to fail without it via negative controls (the exit-code-0
        assertion itself was not separately shown to fail); unit tests
        for the mode split, the already-exited ord, the hung stop RPC,
        and the whole stop-everything behavior (keep going, verify,
        single-flight, bounded); Vitest for Safe Eject.
  - [x] Adversarial 4-lens review + skeptic verification; every
        confirmed finding fixed or explicitly declined in DECISIONS.md.
  - [ ] **Still owed before any packaging work:** one run with a real
        Tauri *release* exe (disk was too tight to build one; the probe
        stands in for it), and macOS/Linux CI (only a cfg-flip
        simulation of the non-Windows build was possible here).
  - [ ] Follow-up noted, not scheduled: the portable-mode window-close
        dialog promises "safe to unplug" but the close path still exits
        even if a stop failed (pre-existing best-effort design).
- [ ] **Step 1 -- foundation fixes to shipped code** (small, separate
      commits; verifiable here against real regtest bitcoind/ord)
  - [x] (a) Console private-key output and secret arguments. Done
        2026-09-26 -- DECISIONS.md "Console private-key output and secret
        arguments". `ord wallet dump`, `listdescriptors true` and
        `gethdkeys` with `private` were classed read-only and printed real
        private descriptors into the monitor and `command_history`. Owner
        delegated the choice ("Do what you believe better"); the recommended
        option was taken: **refuse** them in the console (per call,
        strictly), enforce that in the backend, **scrub extended private keys
        (and descriptor WIF keys) from everything the executor emits**, and
        **erase old history rows** that revealed a secret at startup -- from
        the database *file* too (`secure_delete` + `VACUUM`). Also fixed:
        `ord wallet --no-sync create` (a leading option) was not blocked;
        `createwallet`/`migratewallet` passphrases were shown and stored;
        secret arguments are now hidden **by position, and only shown for a
        known Bitcoin Core method** (the first, text-matching version leaked
        JSON-valid ones such as a numeric passphrase; the second, position-
        only version leaked the words of an unquoted passphrase after the
        first, `bitcoin-cli.exe ...` lines, named arguments, and a key that
        Core echoes back in an error). Everything from a secret method's
        first secret argument on is hidden as one `[redacted]`; an unknown
        method hides all its arguments; a first word that is not a plain
        word is refused. Old history: erased from the database **file** too
        (`secure_delete`, `VACUUM` once ever), failure quarantines the
        history. **Two adversarial reviews** (19 findings upheld in the
        second), all fixed, each with a test. Verified against a real
        Bitcoin Core (6 live tests, negative controls for each new rule) and
        with the real app (real IPC, real database file).
  - [x] (b) One shared `with_wallet_unlocked` helper. Done 2026-09-26 --
        DECISIONS.md "Making the wallet ready to sign". Owner delegated the
        choice ("Do what you believe better"); the recommended option was
        taken: an unencrypted wallet on a **non-mainnet** chain is ready to
        sign with no passphrase (the dead end where Send/Inscribe/Batch/
        Reinscribe asked for a passphrase that does not exist is gone);
        an unencrypted wallet on **mainnet is refused** (fail closed), also
        for the console's `ord wallet` signing commands; whether a wallet
        is encrypted is asked of Core (`getwalletinfo`'s `unlocked_until`,
        VERIFIED live and pinned to Core's help text). New error code
        `WALLET_NOT_ENCRYPTED` (an addition to the spec's list); a stale
        remembered passphrase is forgotten. Verified against a real Bitcoin
        Core and ord, with negative controls, and in the real app; one
        adversarial review (10 findings upheld), fixed. **Not closed here**
        (tracked in Phase 10M): the create/restore window, a guided
        "encrypt this wallet" action, single-flight signing, the console's
        raw `bitcoin-cli` signing RPCs, and live tests that skip silently.
  - [x] (c) Acquire `SingleInstanceLock` in src-tauri (spec Foundation C;
        it was never wired). Done 2026-09-26 -- DECISIONS.md "Single-
        instance lock and command-history pruning": taken before the
        settings DB opens, a second copy gets a plain-language message
        box and exits, released explicitly on exit (Drop does not run --
        shown), stale locks taken over, and the stale check now confirms
        the pid is really Nodekeeper (start time, name as fallback) so a
        reused pid after a crash/reboot cannot lock the folder forever,
        and the takeover is race-safe (3 real copies launched at once
        over a crashed lock: exactly one runs). Verified live with the
        real app (6 scenarios) and by an adversarial review.
        **Known gaps:** (i) an environment data folder pointed at a
        shared external drive is not locked; (ii) hostname / Linux
        clock-step edge cases on macOS/Linux (DECISIONS.md). Review also
        found that Safe Eject left the app -- and so the lock -- running;
        **Safe Eject now closes Nodekeeper after a successful eject** (4 s
        after the message; owner replied "Continue" to the question, so
        the recommended option was taken -- one spawned task in
        `safe_eject` if it should be reversed). Verified end to end with
        the real app and a real regtest node.
  - [x] (d) Prune `command_history` (spec item 7, last 5,000 per
        environment): every 100 new commands per environment, plus once
        at startup. Done 2026-09-26, tested through the real event
        stream and a reopened file database.
- [x] **Step 2 -- signet/testnet4 live smoke test.** Done 2026-09-26
      (DECISIONS.md "Live smoke test of signet and testnet4"). Both real
      chains started through Nodekeeper's own generated config: the
      `[signet]`/`[testnet4]` sections, network flags and ports work, a
      real peer connected on each, **ord starts and answers on the still-
      syncing node** (chain reported correctly, height 0), and both
      processes stop gracefully. The "also live-verified" comments are
      corrected and the exact names/flags are pinned by a unit test; a
      UI test covers all four chains. (Signet and testnet4 share the `tb`
      address prefix; the UI makes no claim about preventing mix-ups.)
      **Still open for real use of these chains:** 1(b) (an unencrypted
      test-chain wallet cannot send/inscribe) and the Phase 4 "start ord
      after IBD" usefulness task.
- [ ] **Step 3 -- diagnostics export** (redacted support bundle). No
      dependencies, fully verifiable here, no new network/deps (`zip` is
      already in nk-verify). New `nk-diagnostics` crate (ARCHITECTURE.md
      update, owner approval). Build the canary-scanner harness first
      (fake mnemonic/passphrase/cookie/xprv scanned across the zip, the
      raw SQLite file and debug.log, with a negative control) -- steps
      4, 10 and 6 reuse it. Create the Settings screen only together
      with its first real occupant (this export button).
- [ ] **Step 4 -- backups**: default public-descriptor backup with a
      no-private-material guard and positive-control test first; the
      encrypted private backup + restore after owner decisions. Needs
      1(a)+1(b), a bytes-level `nk-secrets` file API with its own magic,
      a sensitivity-aware `nk-rpc` descriptor call, and a `ConfirmDialog`
      extension. Fully verifiable on real regtest. Security self-review
      at the end. (Prior VERIFY in DECISIONS.md already showed the seed/
      fingerprint unchanged by `encryptwallet`; only "does the descriptor
      list gain entries" remains to check.)
- [ ] **Step 5 -- Tor toggle** (backend slice first). Do the shared
      node-config-options refactor first -- Tor, resource limits,
      per-environment ports and services all feed `generate_bitcoin_conf`
      and `NodeManager::start`. The [MANUAL] criterion needs a real Tor
      daemon.
- [ ] **Step 6 -- script scheduling** (in-app scheduler only; built-in
      read-only scripts first). Needs step 0, 1(c), 1(d), an `nk-exec`
      timeout-and-kill option, and `triggering_action`/`background`
      params on `run_script`.
- [ ] **Step 7 -- Core/ord update checker** (read-only slice first:
      strict version parsing, bitcoincore.org RSS + ord releases.atom;
      install/update-all after the Settings shell and step 0).
- [ ] **Step 8 -- packaging, Windows slice** (unsigned MSI only, a
      version-sync test, a draft `release.yml`) + user-docs skeleton;
      user docs last. Needs approval to download WiX and to install/
      uninstall a test MSI here. `targets: "all"` would also emit NSIS/
      RPM -- MSI only (owner to confirm).
- [ ] **Step 9 -- signed self-updates.** A throwaway-key signature
      accept/reject spike can run any time under `spikes/`; real
      integration is blocked on the owner generating the production
      keypair (private key never on this machine), the repo-visibility
      decision (repo is private), one Windows installer type, and
      billing. The opt-in gate must live in Rust; no `updater:*`
      permission for the webview.
- [ ] **Step 10 -- remote mode via SSH tunnel.** Most decision-heavy; sits
      after backups so it reuses their secrets refactor and canary
      harness. Needs per-environment ports persisted, the deferred
      master-password unlock, and a real remote host for [MANUAL].
- [ ] **Step 11 -- OS background services**: only the OS-neutral
      "attach to an already-running bitcoind" primitive (already an open
      Foundation C task); ask the owner whether to defer the rest (spec
      lists services under "Later"). No unverifiable systemd/launchd
      code.

### Cross-cutting foundations (design once, reused above)

- One Settings shell, and an environment-less variant of the shared
  `ConfirmDialog` (it requires an `environment` today; CLAUDE.md forbids
  screens building their own confirmation). Needs owner approval.
- One node-handle/ownership abstraction in `NodeManager` (spawned /
  attached / service-managed / remote): quit, update, safe-eject and the
  close handler must never send `stop` to a node Nodekeeper doesn't own.
- One node-config-options struct behind `generate_bitcoin_conf` /
  `NodeManager::start` (Tor, limits, ports, services); keep the existing
  5-argument call sites stable.
- One new `CommandSource` variant for probes/tunnel/service events
  (ripples into ts-rs bindings, `as_str`, the store column, UI filter);
  `CREATE_NO_WINDOW` on every spawn (see step 0).
- One secret-leak canary harness; one `with_wallet_unlocked`; one
  network-allowlist amendment covering everything new (SSH to the user's
  own host, release-CDN redirect hosts, version feeds, the WebView2
  bootstrapper, local Tor ports) -- the no-telemetry rule has no
  mechanical enforcement today, unlike the executor rule.
- Settings-key allowlists (diagnostics, backups, `bitcoin.conf` keys)
  must stay in sync with new keys; anything gating a security behavior
  is enforced in Rust, never trusted from the frontend.
- Portable-mode matrix, decided once: services refuse in the backend,
  self-update is check-only, remote needs master-password unlock,
  backups warn against saving on the same drive, diagnostics scrub
  paths, schedules count toward the portable close warning.
- Item 9 leftovers not assigned to any feature: per-environment ports,
  resource limits (dbcache, max connections), language selection (SPEC
  ~457/462). Ports persistence is a hard dependency of remote mode, CSP
  regeneration and service drift. Owner to say whether these belong in
  Phase 10.
- A Phase 10 security self-review (CLAUDE.md schedules them only for
  Phases 2/5/7, but backups, remote, self-update, services and Tor each
  add a trust boundary).

### Owner decisions needed (batched; none is needed for step 0's analysis)

Blocking now: step 0 fix choice (above). Blocking their steps: 1(a)
console secrets; 1(b) unencrypted test-chain unlock; step 2 real-P2P
smoke-run approval; step 3 new `nk-diagnostics` crate + fail-closed
policy + privacy level; step 4 restore semantics, export mechanism,
backup-password policy/warning wording, ConfirmDialog second field;
step 5 Tor routing mode/scope; step 6 app-closed behavior, schedule
format, mainnet/imported-script policy; step 7 update policy, network
wording, dependencies; step 8 signing provider/publisher, targets,
LICENSE file (Cargo.toml says MIT, none exists), identifier
`com.nodekeeper.desktop` final, WiX download approval; step 9 updater
keypair (owner-generated), endpoint/repo visibility; step 10 network
allowlist + SSH implementation + wallet scope + key handling; step 11
whether to defer. Full text of each is in the scoping output
(2026-09-26 session); re-ask per step rather than all at once.

## Backlog — ideas not in docs/SPEC.md (not scheduled)

Not part of any phase above and not started. Listed here only so
they're not forgotten; each needs explicit go-ahead before work starts
(CLAUDE.md "Scope discipline"), since none of them are in docs/SPEC.md.

- **Restore a different wallet into an environment that already has
  one.** Today, `restore_wallet` only works before a wallet exists for
  that chain (2026-09-25 conversation) -- there's no way to replace an
  environment's existing wallet with a different seed without first
  wiping its data (only possible on Regtest, via Reset Test Lab).
  Requested scope is narrow: still exactly one wallet per environment
  at a time, just the ability to swap which seed occupies that slot,
  not concurrent multi-wallet support (a separate, much larger idea
  that was also discussed and explicitly not requested -- see below).
  Needs: an explicit destructive confirmation flow (through the shared
  `ConfirmDialog`, extra-worded on mainnet per CLAUDE.md's mainnet
  safety rule, since it discards the current wallet's local Core
  wallet state), and deciding what happens to the old wallet's Bitcoin
  Core data (delete vs. keep-but-unused).
- **Multiple named wallets per environment, switchable in the UI**
  (discussed 2026-09-25, not requested). Larger: `ord` supports named
  wallets (`--name`), but every backend command, `wallet_session`'s
  passphrase cache, and mainnet-encryption enforcement are all keyed
  by chain only today -- this would touch most of the wallet-related
  surface area, comparable in size to the original Wallet screen.
