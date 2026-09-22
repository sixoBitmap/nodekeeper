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
toolchain and no C++ linker at all. Installed Rust stable (MSVC host) via
rustup; Visual Studio Build Tools (C++ workload, MSVC + Windows 11 SDK) is
being installed to provide `link.exe` (blocked once on low disk space —
user freed space via Windows Update cleanup, retried with a leaner
component set).

Tasks (one at a time: implement -> test -> quality gate -> commit -> tick):

Environment / tooling
- [ ] Confirm `cargo build` links successfully (MSVC toolchain smoke test)
- [ ] Install `just` (cargo install just) for the quality-gate command

Cargo workspace
- [ ] Root `Cargo.toml` workspace + `crates/nk-core`, `nk-exec`, `nk-proc`,
      `nk-verify`, `nk-rpc`, `nk-ord`, `nk-secrets`, `nk-store`,
      `nk-testkit` skeletons (lib.rs stub + Cargo.toml each, compiles)
- [ ] `src-tauri/` thin Tauri 2 app skeleton (no business logic), wired to
      the workspace
- [ ] clippy `disallowed-methods` config banning
      `std::process::Command`/`tokio::process::Command` outside
      nk-exec/nk-proc; deliberately verify it fails the build once with a
      violation added to another crate, record the result, then remove it
- [ ] `Justfile` with `just check` (cargo fmt --check, cargo clippy -D
      warnings, cargo test, tsc, eslint, vitest)

CI
- [ ] `.github/workflows/ci.yml`: matrix over windows-latest, macos-latest,
      ubuntu-latest running the quality gate

Frontend shell
- [ ] Vite + React + TypeScript + Tailwind + shadcn/ui scaffold in `ui/`,
      wired into the Tauri app
- [ ] Typed IPC scaffold (tauri-specta or ts-rs) with one no-op Tauri
      command proven end-to-end (Rust type -> generated TS type -> UI call)
- [ ] Locked-down Tauri capabilities file (IPC only for the main window's
      own origin) — Foundation D groundwork, established now even though
      inscription rendering is a later phase
- [ ] Design tokens: per-environment colors (mainnet orange, regtest
      purple, etc.), spacing, typography; dark-mode-first with a light
      theme
- [ ] Shared components: EnvBanner, StatusBadge, ErrorPanel, ConfirmDialog
      skeleton (mainnet extra-step + Learn mode plumbing, even if no
      fund-moving action calls it yet), SensitiveSeedView skeleton
- [ ] i18n setup (react-i18next), English locale file, no hard-coded
      user-facing strings from this point on
- [ ] Minimal environment switcher (top bar, shows configured environments
      + status placeholder; switching only changes the displayed
      environment)
- [ ] First-run disclaimer screen (self-custody warning, acknowledge once,
      persisted)
- [ ] System check screen (OS/CPU/RAM/disk space)

Storage
- [ ] `nk-store`: rusqlite + a migration tool, initial schema (settings
      table at minimum), migration runner with tests

Environment model / path resolution (nk-core)
- [ ] Environment struct (name, color, chain, ports, paths) for mainnet/
      regtest/signet/testnet4
- [ ] Per-chain path resolution module (cookie/wallet/index paths) with
      unit tests for every chain
- [ ] Windows long-path support (app manifest `longPathAware`,
      `\\?\`-prefixed paths where needed); test with a deeply nested path

Process / secrets
- [ ] `nk-proc`: single-instance lock file (hostname/PID/timestamp) with
      stale-lock detection; test that a second instance on the same data
      folder is refused
- [ ] `nk-secrets`: OS keychain (`keyring` crate) + encrypted secrets-file
      fallback (Argon2id + XChaCha20-Poly1305); round-trip test and
      wrong-master-password-fails test

Acceptance criteria (from docs/SPEC.md Phase 1 "Done when"):
- [ ] [CI] app builds and launches on all 3 OSes; the quality gate passes
- [ ] [CI] a deliberate process spawn outside nk-exec/nk-proc fails the
      build (verified once, then removed)
- [ ] [CI] path resolution tests pass for all chains, incl. a deeply
      nested Windows path
- [ ] [CI] a second app instance on the same data folder is refused
- [ ] [CI] encrypted secrets file round-trips; wrong master password fails
- [ ] [MANUAL] banner shows the current environment; disclaimer appears on
      first run only

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
