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

## Phase 1 — Foundation

In progress. Dev-environment setup (2026-09-22): this machine had no Rust
toolchain, no C++ linker, and an outdated Node.js at all. Installed Rust
stable (MSVC host) via rustup; Visual Studio Build Tools (C++ workload,
MSVC + Windows 11 SDK) for `link.exe` (blocked once on low disk space —
user freed space via Windows Update cleanup, retried with a leaner
component set, succeeded); Node.js upgraded 20.17.0 -> 24.21.0 LTS (current
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
      ubuntu-latest running the quality gate — written, **not yet
      verified by an actual GitHub Actions run** (no remote/PR pushed yet)

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
- [~] [CI] app builds and launches on all 3 OSes; the quality gate passes
      — verified locally on Windows only (`just check` + `tauri build
      --debug --no-bundle`, both clean); macOS/Linux need the actual
      GitHub Actions run once this is pushed (workflow is written, not
      yet exercised — see the CI item above)
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

Not started. See docs/SPEC.md Phase 2.

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
