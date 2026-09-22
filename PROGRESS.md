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
      cleanly (no full sync required)
- [ ] [MANUAL] I have checked every pinned builder-key fingerprint
      against bitcoin-core/guix.sigs from a separate machine or browser
- [x] Security self-review completed (go through SECURITY RULES line by
      line, point to the code/test enforcing each, list any gaps) — see
      DECISIONS.md "Phase 2 — security self-review"

## Phase 3 — Dashboard and monitor

Not started. See docs/SPEC.md Phase 3.

## Phase 4 — ord integration

Not started. See docs/SPEC.md Phase 4.

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
