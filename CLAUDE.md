# Nodekeeper

Nodekeeper is a cross-platform desktop app (Tauri 2 + Rust backend, React +
TypeScript + Tailwind + shadcn/ui frontend, Recharts) that lets non-technical
users install and run a Bitcoin Core full node and the `ord` ordinals
indexer/wallet, and use `ord`'s features through a GUI, while still letting
power users run raw `bitcoin-cli`/`ord` commands and scripts. It runs on
Windows, macOS, and Linux, installed or portable (external SSD), and supports
several networks (mainnet, regtest, signet, testnet4) running side by side,
with every backend command visible live in the UI.

## Source of truth

- [docs/SPEC.md](docs/SPEC.md) is the full spec, saved verbatim. If code and
  spec disagree, the spec wins unless [DECISIONS.md](DECISIONS.md) records an
  approved change.
- [ARCHITECTURE.md](ARCHITECTURE.md) — crate layout, environment model,
  executor/process-manager/security design.
- [DECISIONS.md](DECISIONS.md) — every VERIFY result (exact commands +
  output), pinned-value sources, and approved deviations from the spec.
- [PROGRESS.md](PROGRESS.md) — phase/task checklist with acceptance criteria.
  Update after every completed task so work can resume after a context reset.

## Resume protocol

At the start of every session: read this file, then PROGRESS.md, then
DECISIONS.md, then only the SPEC.md sections relevant to the current phase.

## Non-negotiable rules

- **STOP AND ASK**: if a VERIFY result contradicts the spec, or a required
  feature is impossible as written, stop immediately, explain what was found,
  propose options with trade-offs, and wait for a decision. Never quietly
  improvise a workaround — especially around mainnet wallet encryption,
  graceful ord shutdown, dry-run support, or anything touching seeds, keys, or
  fund movement. Non-blocking questions are collected and asked at the end of
  a phase.
- **VERIFY, don't assume**: ord's flags/defaults/behavior change between
  versions. Every VERIFY item must be checked against the installed version's
  actual `--help` output / docs, and the result + exact commands recorded in
  DECISIONS.md. Never invent or recall pinned values (builder keys, ord
  hashes) from memory — take them only from official sources and list the
  source URL in DECISIONS.md.
- **All commands through the executor**: every ord CLI command, bitcoin-cli
  command, and RPC call runs through the central executor (`nk-exec`), as
  argument arrays (never shell-interpolated strings). This is enforced
  mechanically — `std::process::Command` / `tokio::process::Command` may only
  be used inside `nk-exec`/`nk-proc` (clippy `disallowed-methods` / CI check).
- **Secrets**: mnemonics and wallet-encryption passphrases are never logged,
  stored, or transmitted. Mnemonics flow only through the sensitive-output
  channel (full-screen seed view only — never the monitor, logs, history, or
  exports). Secrets passed to commands via stdin/RPC params, never argv.
  Zeroize secrets in memory after use.
- **Mainnet safety**: every mainnet wallet is encrypted; every fund-moving
  action has a dry-run/PSBT preview and an extra mainnet confirmation step
  (env name in large text) through the shared `ConfirmDialog` — no screen
  implements its own confirmation flow. Core spend commands against ord
  wallets are blocked by default.
- **Binary verification fails closed**: Bitcoin Core verified via SHA256SUMS +
  SHA256SUMS.asc against pinned builder keys (>=3 valid sigs); ord verified
  against pinned SHA-256 hashes. Any verification failure refuses to run.
- **Environment isolation**: no command, script, wallet action, or template
  may cross from one environment (network) to another.
- **No telemetry.** Network calls only to: Bitcoin P2P, bitcoincore.org, the
  official ord GitHub releases, Nodekeeper's own GitHub Releases update
  endpoint (user-initiated/opt-in), the local Tor daemon when enabled, and
  local services (127.0.0.1).
- **Scope discipline**: no features beyond SPEC.md (e.g. no rune send/mint/
  etch) without asking first.
- **Never weaken a test or security rule** to make something pass — explain
  and ask instead.

## Quality gate

Run before every commit and before claiming a task done:

```
just check
```

(cargo fmt --check, cargo clippy -D warnings, cargo test, TypeScript type
check, ESLint, Vitest — see [ARCHITECTURE.md](ARCHITECTURE.md) once the
workspace exists.)

## Working rhythm

One task at a time: implement -> test -> quality gate -> commit (conventional
commit message) -> tick it off in PROGRESS.md. Security self-review at the
end of Phases 2, 5, and 7.
