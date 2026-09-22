# Progress

Checklist of every phase, its tasks, and its acceptance criteria. Update
after every completed task.

## Phase 0 — Feasibility spike (no app code)

Tasks:
- [x] Save spec verbatim as docs/SPEC.md
- [x] Create CLAUDE.md, ARCHITECTURE.md (initial draft), DECISIONS.md,
      PROGRESS.md
- [ ] In spikes/, download current Bitcoin Core and ord, run on regtest
- [ ] VERIFY: encrypted Core wallet + ord wallet commands, unlock method
- [ ] VERIFY: ord graceful shutdown (SIGINT / CTRL_BREAK_EVENT)
- [ ] VERIFY: which ord commands accept secrets via stdin
- [ ] VERIFY: ord server default bind address + flag to change it
- [ ] VERIFY: `ord wallet send --dry-run` existence/output
- [ ] VERIFY: reinscribe syntax, batch reinscribe support
- [ ] VERIFY: index-option -> feature mapping
- [ ] VERIFY: per-chain data paths (Bitcoin Core, ord)
- [ ] VERIFY: testnet4 support; ord release checksums
- [ ] VERIFY: built-in regtest command (e.g. `ord env`)
- [ ] Record all VERIFY results in DECISIONS.md with exact commands + output
- [ ] Present summary, ask code-signing question, flag any spec conflicts
- [ ] [MANUAL] User has read DECISIONS.md, answered code-signing question,
      approved

Done when: every VERIFY item above has an answer or is explicitly marked
"needs CI/Windows"; user has approved.

## Phase 1 — Foundation

Not started. Build: Cargo workspace + crate skeletons, quality gate command,
disallowed-methods check, CI matrix, UI shell/navigation, design tokens +
shared components (EnvBanner, StatusBadge, ErrorPanel, ConfirmDialog
skeleton), i18n setup, SQLite + migrations, environment model + per-chain
path resolution (incl. Windows long paths), minimal environment switcher,
single-instance lock, secrets storage, first-run disclaimer, system check.

Acceptance criteria: see docs/SPEC.md Phase 1.

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
