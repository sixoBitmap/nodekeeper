# Decisions log

Records every VERIFY result (exact commands + relevant output), pinned-value
sources, and any approved deviation from docs/SPEC.md.

## Status

Phase 0 feasibility spike: **complete and approved**. Both STOP AND ASK
items below were decided by the project owner on 2026-09-22; see "Approved
deviations" at the end of this file.

Spike environment: Windows 11, done in `spikes/` (throwaway, not committed —
see `.gitignore`). Binaries used: **Bitcoin Core 31.1** (win64),
**ord 0.29.0** (x86_64-pc-windows-msvc). These are the current latest stable
releases as of 2026-09-22, confirmed via the actual GitHub "latest release"
redirect, not recalled from memory:
```
curl -sI -o /dev/null -w "%{redirect_url}" https://github.com/bitcoin/bitcoin/releases/latest
  -> https://github.com/bitcoin/bitcoin/releases/tag/v31.1
curl -sI -o /dev/null -w "%{redirect_url}" https://github.com/ordinals/ord/releases/latest
  -> https://github.com/ordinals/ord/releases/tag/0.29.0
```
(A web search claimed ord's latest was "0.27.1" — that was wrong/stale; the
GitHub redirect is the authoritative source and was used instead. This is
exactly why the spec requires checking live sources, not recalling versions.)

## Pinned values collected during the spike

These were independently downloaded and cryptographically checked in this
session. **Please re-check them yourself from a separate machine or browser**
before they're wired into `nk-verify` in Phase 2, per the spec's Phase 2
[MANUAL] acceptance criterion.

### Bitcoin Core 31.1 (win64)
- File: `bitcoin-31.1-win64.zip`
- Source: https://bitcoincore.org/bin/bitcoin-core-31.1/
- SHA256: `c99ef173471c58e6766d9eebd12e6c35349082eeed3939bc99eed58ef57db587`
  (matches the published `SHA256SUMS` file at the same URL)
- `SHA256SUMS.asc` verified with GnuPG against builder keys fetched live from
  https://github.com/bitcoin-core/guix.sigs/tree/main/builder-keys (39 keys
  imported). Result: **11 good signatures** (spec requires >=3), including
  Ava Chow, fanquake, hebasto, and 8 others. Commands run:
  ```
  curl -s https://api.github.com/repos/bitcoin-core/guix.sigs/contents/builder-keys
  # -> download each *.gpg into a fresh GNUPGHOME, gpg --import
  gpg --verify SHA256SUMS.asc SHA256SUMS
  ```
- Conclusion: the pinned-builder-key + SHA256SUMS.asc verification design in
  the spec works exactly as intended, achievable with a plain `gpg` binary
  (Sequoia-PGP in the real nk-verify crate should produce the same result;
  not separately spiked here since gpg was available and sufficient to prove
  the verification design).

### ord 0.29.0 (x86_64-pc-windows-msvc)
- File: `ord-0.29.0-x86_64-pc-windows-msvc.zip`
- Source: https://github.com/ordinals/ord/releases/download/0.29.0/ord-0.29.0-x86_64-pc-windows-msvc.zip
- SHA256: `93de82db792ccc37ae385c49646c0f649d38049f4e959499c6e7c5d1a81bf2ad`
  (matches the `digest` field GitHub's Releases API reports for this asset)
- **ord does not publish its own maintainer-signed checksums file** (no
  SHA256SUMS/.asc in the release assets, no checksums in the release body —
  checked via `curl https://api.github.com/repos/ordinals/ord/releases/tags/0.29.0`).
  The only checksum available is GitHub's own server-computed `digest` per
  asset, which is **not signed by the ord maintainers** — it only proves the
  download wasn't corrupted/tampered in transit from GitHub, not that GitHub
  itself (or a compromised maintainer account) didn't ship a bad build.
  This makes Nodekeeper's own pinned SHA-256 hash (updated by us, per
  Nodekeeper release, after our own review of each ord release) the *primary*
  line of defense for ord, exactly as the spec already assumes ("in all
  cases also check against SHA-256 hashes pinned inside Nodekeeper").

## VERIFY items

### 1. Encrypted Core wallet + ord wallet commands — CONFIRMED, no conflict
`ord wallet` commands work cleanly against an encrypted Bitcoin Core wallet,
using Core's standard `walletpassphrase` RPC — exactly the flow the spec
describes (nk-exec calls `walletpassphrase <pass> <timeout>` via RPC before a
signing action, then the ord command, with no special ord-side handling
needed).
- Reads (`wallet balance`, `wallet addresses`) work while locked.
- A real (non-dry-run) `wallet send` while locked fails cleanly with Core's
  standard error: `RpcError { code: -13, message: "Error: Please enter the
  wallet passphrase with walletpassphrase first." }`.
- After `bitcoin-cli walletpassphrase <pass> 30`, the same `wallet send`
  succeeds and returns `{"txid": ..., "psbt": ..., "asset": ..., "fee": ...}`.
- Note: `encryptwallet` prints "a new HD seed was generated" — this is Core's
  generic legacy-wallet message text; for a **descriptor** wallet (what ord
  uses) the existing descriptors/keys are *not* actually replaced. Verified
  empirically: balance and existing UTXOs (including an inscribed sat) were
  intact after encryption, and a real signed send from those same coins
  succeeded post-unlock.
- Conclusion: no workaround needed, no conflict with the spec.

### 2. ord graceful shutdown (SIGINT / CTRL_BREAK_EVENT) — CONFIRMED on Windows
Spawned `ord.exe server` with Win32 `CreateProcessW(..., CREATE_NEW_PROCESS_GROUP, ...)`
(the exact mechanism `nk-proc` will use from Rust via
`CommandExt::creation_flags`), then called
`GenerateConsoleCtrlEvent(CTRL_BREAK_EVENT, pid)`. Result:
```
Shutting down gracefully. Press <CTRL-C> again to shutdown immediately.
```
Process exited with code 0 within ~2 seconds (well under any reasonable
timeout). Full repro script: `spikes/test-createprocess-v2.ps1`.
- **Windows: confirmed, works exactly as the spec assumes.**
- **macOS/Linux SIGINT: not tested — no machine of either OS available in
  this session.** Marked "needs CI" per the spec's own allowance. Add a CI
  job in Phase 1/2 that starts `ord server` on Linux/macOS runners, sends
  SIGINT, and asserts clean exit + exit code, mirroring this Windows repro.

### 3. Which ord commands accept secrets via stdin — CONFIRMED
`ord wallet restore --from mnemonic` (and `--from descriptor`) reads the
secret from **stdin**, not argv:
```
echo "<12 words>" | ord.exe --regtest ... wallet --name restored restore --from mnemonic --timestamp now
# exit 0, wallet restored, no secret ever appears in argv
```
`ord wallet create` has no stdin input (it *generates* a new mnemonic and
prints it as JSON to stdout) — this output must be routed exclusively to the
sensitive-output channel, per Foundation B; confirmed the output shape is a
plain JSON object `{"mnemonic": "...", "passphrase": ""}` with nothing else
mixed into stdout, so it's cleanly interceptable.

**Conflict found (STOP AND ASK — see below): the optional BIP39 `--passphrase`
value on both `wallet create` and `wallet restore` is CLI-**argv**-only.**
There is no stdin or env-var alternative in ord 0.29.0's CLI for this flag.

### 4. ord server default bind address — CONFIRMED, matches spec's assumption
`ord server --help` shows `--address <ADDRESS>` defaults to `0.0.0.0`.
Confirmed the spec's assumption exactly: **ord binds all interfaces by
default and Nodekeeper must always pass `--address 127.0.0.1` explicitly.**
Also: the server did not actually serve HTTP traffic until `--http` was
passed in addition to `--address`/`--http-port` (config generation in nk-ord
must always include `--http --address 127.0.0.1 --http-port <port>`
together).

### 5. `ord wallet send --dry-run` / `ord wallet inscribe --dry-run` — CONFIRMED, exist
Both exist. `--dry-run` skips signing/broadcast and returns the same JSON
shape as a real run, e.g. for `inscribe --dry-run`:
```json
{
  "commit": "...", "commit_psbt": "<base64 PSBT>",
  "inscriptions": [{"destination": "...", "id": "...", "location": "..."}],
  "reveal": "...", "reveal_broadcast": false, "reveal_psbt": "<base64 PSBT>",
  "total_fees": 592
}
```
`send --dry-run` similarly returns `{"txid", "psbt", "asset", "fee"}` without
`reveal_broadcast`/broadcasting. This is exactly the cost-breakdown data
Inscribe Studio and the wallet's send review screen need.

### 6. Reinscribe syntax + batch support — CONFIRMED
Single: `ord wallet inscribe --file <FILE> --satpoint <SATPOINT> --reinscribe --fee-rate <N>`
(`--sat <SAT>` also accepted instead of `--satpoint`). Full round trip tested
on regtest: dry-run, then real broadcast, then confirmed via the JSON API:
- `/r/sat/<sat>` → ordered list of every inscription id on that sat:
  `{"ids": ["<original>", "<reinscription>"], "more": false, "page": 0}`
- `/r/sat/<sat>/at/-1` → the latest (topmost) inscription on that sat.
- The reinscription's own detail (`/inscription/<id>`) carries
  `"charms": [..., "reinscription", ...]` and `"previous": "<original id>"`;
  the original gains `"next": "<reinscription id>"`.
These map directly onto the reinscribe-mode gallery/labeling requirements
in spec item 4.

Batch: `ord wallet batch --batch <FILE.yaml>` supports a top-level
`reinscribe: true` boolean in the YAML (per docs.ordinals.com's batch guide,
cross-checked against the CLI's YAML-driven design — the schema itself isn't
exposed via `--help` since it's a file format, not flags). **Confirmed batch
reinscribe is supported in 0.29.0**, so it doesn't need to be hidden.

### 7. Index option → feature mapping (Foundation F) — CONFIRMED empirically
Ran two ord servers on the same regtest chain from the same underlying
bitcoind: one with `--index-sats --index-runes --index-addresses`, one with
none of them, and probed the relevant endpoints directly:

| Endpoint | With index off | Behavior |
|---|---|---|
| `/r/sat/<sat>` (sat-level, incl. "all inscriptions on this sat") | `index-sats` | Hard error: `"this server has no sat index"` |
| `/address/<addr>` (explorer address lookups) | `index-addresses` | Hard error: `"this server has no address index"` |
| `/runes` (rune balances/listing) | `index-runes` | **No error** — silently returns `{"entries": [], ...}` |
| `/inscription/<id>` (base inscription detail) | none needed | Always works; `"sat"` is `null` and rarity charms (`coin`/`uncommon`/etc.) are omitted without `index-sats`, since rarity is derived from the sat number |

Confirms the spec's Foundation F mapping (sat views -> index-sats, address
lookups -> index-addresses, rune balances -> index-runes) exactly. One
implementation note: **ord does not error for rune features when
index-runes is off — it just returns empty** — so Nodekeeper's own UI gating
(hide/explain per Foundation F) is the only thing preventing a confusing
"you have no runes" instead of "this feature needs the runes index," and is
not optional.

### 8. Per-chain data paths — CONFIRMED
Bitcoin Core (regtest, live-checked): cookie at
`<datadir>/regtest/.cookie` (chain subfolder), confirmed by directly
inspecting a running regtest datadir. Mainnet/signet/testnet4 follow the same
pattern (`<datadir>/.cookie` for mainnet with no subfolder,
`<datadir>/signet/.cookie`, `<datadir>/testnet4/.cookie`) per Bitcoin Core's
well-documented, unchanged-in-decades behavior.

ord (via `ord settings`):
```
no --data-dir override:  regtest -> %APPDATA%\ord\regtest   mainnet -> %APPDATA%\ord
--data-dir=X override:   regtest -> X\regtest                mainnet -> X   (X unchanged)
```
Same per-chain-subfolder pattern as Core, confirmed also for signet and
testnet4 (all four: `ord --<chain> --data-dir=X settings` and the
equivalent live `bitcoind -<chain> -datadir=X` run, both checked directly
against files on disk, not just `settings` output — see the Phase 1 entry
below).

**Correction to the note originally here**: it previously claimed that
because Nodekeeper always passes an explicit `--data-dir`, ord's per-chain
default "never actually applies in the app." That was wrong — untested at
the time. **ord appends its own chain subfolder to `--data-dir` even when
it's given explicitly** (confirmed above), for every non-mainnet chain.
Since each Nodekeeper environment already maps 1:1 to one chain, this means
ord's *actual* on-disk index directory is one level deeper than the
`--data-dir` value Nodekeeper passes it — e.g. passing
`--data-dir=data/regtest/ord` on the regtest environment means the index
really lands in `data/regtest/ord/regtest/`, not `data/regtest/ord/`
directly. `nk-core`'s path resolution (added in Phase 1) distinguishes
"the `--data-dir` argument to pass ord" from "where ord's files actually
end up," because disk-usage reporting and "Reset Test Lab" (regtest-only
delete) need the real path, not the argument value.

### 9. testnet4 support; ord release checksums — CONFIRMED
`ord --help` lists `testnet4` as a valid `--chain` value and has a dedicated
`--testnet4` flag in 0.29.0 — **testnet4 should NOT be hidden**, it's
supported. Checksums: see "Pinned values" above — no maintainer-published
checksums file for ord; GitHub's per-asset `digest` is the only built-in
check, and Nodekeeper's own pinned hash remains the primary defense.

### 10. Built-in regtest command (`ord env`) — CONFIRMED, exists
`ord env [DIRECTORY]` — "Start a regtest ord and bitcoind instance," spawning
both processes itself in one command. Per the spec, Nodekeeper's own Test Lab
controls (process manager, PID tracking, multi-environment isolation) are
used instead; `ord env` is not used internally, since it would bypass
nk-proc's process tracking and single-instance locking entirely.

## Other findings worth recording

- **No `-daemon` flag on the Windows build** of bitcoind
  (`Error: -daemon is not supported on this operating system`). Not a
  conflict — nk-proc was always going to spawn bitcoind as a tracked
  foreground child process (never `-daemon`) so it owns the process handle
  and can PID-track it; this just confirms that's the only option on
  Windows anyway, so the same code path can be used on all three OSes.
- `ord wallet send`'s real (non-dry-run) response includes a `"psbt"` field
  even after broadcasting — useful for a "technical details" panel.

## STOP AND ASK — decisions (resolved 2026-09-22, see Approved deviations)

### A. BIP39 passphrase can only be passed to ord via a CLI argument
`ord wallet create --passphrase <PASSPHRASE>` and
`ord wallet restore --passphrase <PASSPHRASE>` are the **only** ways to
supply the optional BIP39 passphrase in ord 0.29.0's CLI — there is no stdin
or environment-variable alternative (confirmed via `--help` for both
commands; only the mnemonic/descriptor itself has a `--from stdin` option).
This directly conflicts with the spec's absolute rule: "passes secrets
(passphrases, mnemonics) through stdin or RPC parameters, never as
command-line arguments, which other processes can read."

The BIP39 passphrase is optional (default `""`) and distinct from both the
mnemonic and the Core wallet-encryption passphrase, but it's still explicitly
called out in the spec as sensitive ("The UI must clearly distinguish the
encryption password from an optional BIP39 passphrase"). If a user sets one,
it would briefly appear in that process's argv (visible via Task Manager /
`ps` / `/proc/<pid>/cmdline` to anything with permission to inspect the
process list on that machine) for the life of the `wallet create`/`restore`
call.

Options, without me picking one:
1. **Don't support the BIP39 passphrase feature at all** — never surface the
   `--passphrase` flag in the UI. Simplest, fully honors the "never in argv"
   rule, but removes an optional (and rarely used) feature.
2. **Support it, but document/accept the brief argv exposure** as a
   known, narrow, local-only risk (the same machine already trusts the user
   running Nodekeeper; the exposure window is only the few seconds the
   `wallet create`/`restore` process runs, and only to other processes with
   permission to read that user's process list).
3. **Support it with mitigation**: on Linux, overwrite `argv[0]` memory after
   spawn to obscure it from `/proc/<pid>/cmdline` (doesn't work reliably on
   Windows/macOS, so cross-platform coverage would be inconsistent); adds
   real complexity for partial protection.

My default lean is option 1 (skip the feature) since it's the only one that
doesn't touch the "never via argv" rule at all, but this is exactly the kind
of key-handling tradeoff the spec says not to improvise — please pick.

### B. Code signing (explicitly requested by the spec for Phase 0)
Will Nodekeeper be code-signed and notarized?
- **macOS**: requires an Apple Developer ID ($99/yr) for notarization.
  Unsigned builds work but trigger Gatekeeper warnings, and the portable-mode
  App Translocation behavior (spec item 12) is worse for unsigned apps.
- **Windows**: requires a code-signing certificate (EV certs avoid
  SmartScreen warnings immediately; standard certs still show warnings until
  enough reputation accrues). Unsigned `.exe`/`.msi` trigger SmartScreen.
Please answer yes/no for each platform (they're independent) so the macOS
first-launch flow and Windows installer/SmartScreen messaging can be
designed correctly starting in Phase 1.

## Phase 1 — environment model / path resolution (2026-09-22)

Phase 0's per-chain path check (VERIFY item #8) only actually tested
regtest. Before writing `nk-core`'s path resolution, checked signet and
testnet4 too, live, with the same Bitcoin Core 31.1 / ord 0.29.0 binaries
from the Phase 0 spike (`spikes/bitcoin`, `spikes/ord` — not re-downloaded):

- `bitcoind -signet -datadir=X` -> `X/signet/` (cookie, blocks, chainstate,
  etc. all under it). `bitcoind -testnet4 -datadir=X` -> `X/testnet4/`,
  same pattern. Both confirmed by inspecting the actual directory contents
  after a live (brief) run, not just log lines.
- `ord --signet`/`--testnet4 --data-dir=X settings` -> `data_dir` reported
  as `X/signet` / `X/testnet4` respectively — see the corrected note under
  "Per-chain data paths" above for the more important finding (ord nests
  its own chain subfolder under `--data-dir` even when given explicitly).

Conclusion: bitcoind and ord both use the *exact same* subfolder name for
a given chain — literally `regtest`/`signet`/`testnet4`, no subfolder for
mainnet — for both their default and explicit-`--data-dir` behavior. `nk-core`'s
`Chain::data_subdir()` encodes this once, shared by both.

**Windows long paths**: the `longPathAware` manifest element's namespace
matters and is easy to get wrong silently (an unrecognized namespace is
just ignored, not an error) — tauri-build's own doc example uses
`http://schemas.microsoft.com/SMI/2005/WindowsSettings`, but Microsoft's
own current docs specify `.../2016/WindowsSettings` for `longPathAware`
specifically (2005 is for older settings like `dpiAware`). Used the 2016
one (Microsoft's own page, not the tauri-build example), and verified the
built `.exe`'s embedded manifest directly with the Windows SDK's `mt.exe`
rather than trusting the build succeeded silently. Also confirmed via
Microsoft's docs that `longPathAware` alone is insufficient without a
machine-wide `LongPathsEnabled` registry value Nodekeeper can't set for
the user — so `nk_core::paths::to_verbatim` (`\\?\`-prefixing, which
bypasses MAX_PATH unconditionally) is the mechanism actually relied on for
Nodekeeper's own file I/O, with the manifest as a secondary opt-in.

## Phase 1 — dev environment and Cargo workspace (2026-09-22)

- **This machine had no Rust toolchain and no C++ linker at all** (neither
  MSVC `link.exe` nor MinGW `gcc`). Installed Rust stable via the official
  rustup installer (`rustc 1.98.1`, MSVC host), and Visual Studio Build
  Tools 2022 (C++ workload: `Microsoft.VisualStudio.Component.VC.Tools.x86.x64`
  + `Microsoft.VisualStudio.Component.Windows11SDK.22621`, MSVC toolset
  14.44.35207) for the linker. First attempt failed on low disk space
  (needed 6.71 GB, had 5.61 GB); user freed space via Windows Update
  cleanup, retry with a leaner component set (no `--includeRecommended`)
  succeeded. Verified with a real `cargo build` smoke test (compiles and
  links) before touching the real workspace.
- **Node.js 20.17.0 (pre-installed) was too old** for current frontend
  tooling — Vite 8 and ESLint 9's dependencies require Node `^20.19.0 ||
  >=22.12.0`, and downgrading every affected package individually was
  going to be a recurring problem. Upgraded to the current Node.js LTS via
  the official installer instead: **Node v24.21.0 ("Krypton")**, from
  `https://nodejs.org/dist/v24.21.0/node-v24.21.0-x64.msi` (fetched the
  latest-LTS entry live from `https://nodejs.org/dist/index.json` rather
  than assuming a version).
- **`tauri.conf.json`'s `beforeDevCommand`/`beforeBuildCommand` object form
  uses a `script` field, not `command`** (confirmed against the live
  schema at `https://schema.tauri.app/config/2`, `HookCommand` definition)
  — used because `src-tauri/` lives at the repo root while the frontend
  lives in `ui/`, per the spec's directory layout, so the hook needs an
  explicit `cwd`.
- **Disallowed-methods clippy check, verified once per the spec's Phase 1
  acceptance criterion**: added a deliberate
  `std::process::Command::new(...)` call to `nk-core` (a crate other than
  nk-exec/nk-proc), ran `cargo clippy --workspace --all-targets -- -D
  warnings -W clippy::disallowed-methods`, and confirmed it failed the
  build:
  ```
  error: use of a disallowed method `std::process::Command::new`
  error: could not compile `nk-core` (lib) due to 1 previous error
  ```
  Removed the violation immediately after confirming this. Clean workspace
  now passes the same command with exit 0. This check runs via `just
  check` and in CI on every push.
- **`just` needs `sh` on PATH on Windows** (it shells out to `sh` for every
  recipe, which is how it runs POSIX-style recipes like `cd ui && npm run
  ...` — this is documented `just` behavior, not a bug). Git for Windows
  ships one at `C:\Program Files\Git\usr\bin\sh.exe`, but that directory
  isn't necessarily on PATH in every shell (it wasn't in this one). Add it
  to PATH if `just check` fails with "could not find the shell `sh`".
  GitHub's `windows-latest` runner has Git for Windows on PATH by default,
  so this shouldn't affect CI.
- Ran the full local quality gate (`just check`: cargo fmt --check, cargo
  clippy -D warnings, cargo test --workspace, tsc, eslint, vitest) clean
  end to end after all of the above, and a real `tauri build --debug
  --no-bundle` (Rust backend + frontend linked together) with no warnings.
- **`chacha20poly1305` 0.11 / `aead` 0.6's nonce/key generation API**: a
  docs.rs fetch for the current version returned an example that didn't
  compile (`OsRng`/`generate_nonce` don't exist at those paths in this
  version). Rather than guessing again, read the actual crate source
  already downloaded into the local cargo registry
  (`~/.cargo/registry/src/.../aead-0.6.1`,
  `~/.cargo/registry/src/.../crypto-common-0.2.2/src/generate.rs`) to find
  the real current API: a `Generate` trait (re-exported when the
  `rand_core` feature is active, which `chacha20poly1305`'s default
  `getrandom` feature enables transitively) with `Type::generate()` —
  e.g. `XNonce::generate()`, `<[u8; 16]>::generate()` — using the system
  CSPRNG internally, no explicit `OsRng` needed. `keyring` 4.2.0's default
  `v1` feature was checked the same way (its own Cargo.toml on GitHub)
  and already covers Windows/macOS/Linux Secret Service, so no extra
  feature flags were needed there either.

## Phase 1 — frontend shell, shared components, CSP (2026-09-22)

- **shadcn/ui's `init` writes its own design tokens into `index.css`**
  (`--background`, `--card`, `--primary`, etc., Nova preset) alongside
  whatever was already there. Left as two parallel token systems this
  would have been confusing long-term, so they were consolidated: shadcn's
  tokens stayed as the general UI palette, the hand-written
  `--color-env-*`/`--color-success`/etc. tokens (which shadcn has no
  equivalent for) were kept as Nodekeeper-specific additions layered on
  top, and the now-redundant hand-written `--color-bg`/`--color-surface`/
  `--color-text`/etc. tokens were removed in favor of shadcn's.
- **Tailwind's build-time class scanner requires literal class-name
  strings in source** — `` `bg-env-${chain}` `` produces no CSS at all,
  since Tailwind doesn't evaluate JavaScript, only regex-scans source text
  for complete-looking class tokens. `ui/src/lib/environment-colors.ts`
  uses `Record<Chain, string>` lookup tables with every class name written
  out in full for exactly this reason.
- **CSP hardened from `null` to a `default-src 'self'` baseline** now,
  not deferred to when inscription rendering lands — a strict default is
  worth having regardless of phase, and `null` meant no CSP was applied
  at all. No `frame-src` is set, meaning no iframes are possible yet,
  which is the correct fail-closed state; Phase 4 must add each
  environment's `http://127.0.0.1:<ord-port>` explicitly when inscription
  rendering is built. `capabilities/default.json`'s scaffold default
  (`"windows": ["main"]`) was reviewed and is already sufficient for one
  trusted window with no untrusted content source yet. Full detail in
  ARCHITECTURE.md "Webview security model". **Not verified against the
  real Tauri webview** — only that `tauri build` accepts the config;
  browser-based UI verification (below) doesn't exercise this CSP at all,
  since it only applies inside the actual Tauri window, not a plain
  browser hitting the Vite dev server.
- **UI verification**: automated tests (Vitest + Testing Library) cover
  the safety-critical logic (the mainnet extra-step gating in
  `ConfirmDialog`, the word-confirmation gating in `SensitiveSeedView`,
  the disclaimer-then-main-shell flow in `App`). Beyond that, rendered the
  real app in an actual browser with Tauri's IPC mocked
  (`@tauri-apps/api/mocks`' `mockIPC`, wired up in
  `ui/src/lib/dev-tauri-mock.ts`, active only under `npm run dev` outside
  a real Tauri context — also useful going forward for anyone iterating on
  the UI without the full Tauri shell) and visually confirmed the
  disclaimer screen and, after acknowledging it, the main shell (orange
  MAINNET banner, environment switcher, real system-check numbers). One
  interaction (the theme-toggle button) produced a screenshot that stayed
  on the old (dark) frame after clicking, despite `getComputedStyle`,
  `document.documentElement.classList`, and `localStorage` all correctly
  showing the light theme applied — traced to the preview tool's
  screenshot pipeline (the tab's `document.visibilityState` was
  `"hidden"` at the time), not the app; re-verified against computed
  styles directly rather than trusting that one screenshot.

## Phase 1 — CI verification (2026-09-22)

Pushed to a new private GitHub repo (https://github.com/sixoBitmap/nodekeeper,
created with `gh repo create`) so `.github/workflows/ci.yml` could actually
run — it had only ever been syntax-written before this, never exercised.

First run (`35777802145`) failed identically on all 3 OSes, at the same
step, same error — a real bug, not flakiness:
```
Error: [vitest-pool]: Failed to start forks worker for test files ....test.tsx
Caused by: TypeError: webidl.util.markAsUncloneable is not a function
  at new CacheStorage node_modules/undici/lib/web/cache/cachestorage.js:20:17
  at Object.<anonymous> node_modules/jsdom/lib/api.js:12:33
```
Cause: the workflow pinned `actions/setup-node@v4` to `node-version:
"20"`, but local dev on this machine had been upgraded to Node 24.21
earlier in this session (see the Cargo-workspace entry above) — every
local `just check`/`npm test` run had therefore only ever exercised Node
24, never 20. jsdom's fetch/Cache-API polyfill (via `undici`) broke
against Node 20's older internals. Fixed by changing `node-version` to
`"24"` to match local dev exactly, rather than picking a version neither
environment actually uses.

While investigating, also fixed the "App builds" CI step before it could
fail the same way: it ran `npm run tauri -- build --debug --no-bundle`
with `working-directory: ui`, which has the exact same bug already fixed
locally in the Justfile's `build-app` recipe (the Tauri CLI only finds
`src-tauri/` in subfolders of the current directory, and `src-tauri`
lives at the repo root, not inside `ui/`). Changed to invoke
`./ui/node_modules/.bin/tauri` directly from the repo root, matching the
Justfile.

Second run (`35779301209`): green on windows-latest, macos-latest, and
ubuntu-latest — every step, including the full `tauri build` on each OS.
https://github.com/sixoBitmap/nodekeeper/actions/runs/35779301209

## Phase 2 — pinned Bitcoin Core builder keys (2026-09-22)

`crates/nk-verify/pinned-keys/bitcoin-core-builder-keys.gpg` is 39 builder
public keys, fetched fresh for Phase 2 (not reused from the Phase 0 spike
copies, to make sure the pinned set is current as of when it's actually
being embedded into the app):
```
curl -s https://api.github.com/repos/bitcoin-core/guix.sigs/contents/builder-keys
# -> download_url of every entry, concatenated into one file
```
Source: https://github.com/bitcoin-core/guix.sigs/tree/main/builder-keys
(commit at fetch time not separately pinned — see the note below).
Sanity-checked by importing into a fresh, throwaway GPG keyring: 39
entries processed, 38 distinct keys (one file maps to a key already
covered by another file's cert — not a problem, `CertParser` doesn't
mind duplicates and the signature-counting logic in nk-verify counts
distinct key fingerprints, not files). This exact file is what
`nk-verify` embeds via `include_bytes!` and verifies Bitcoin Core release
signatures against.

**Please independently check these against bitcoin-core/guix.sigs from a
separate machine or browser** — this is the Phase 2 [MANUAL] acceptance
criterion ("I have checked every pinned builder-key fingerprint"). The
fingerprints actually embedded are listed by running (from
`crates/nk-verify/pinned-keys/`): `gpg --with-colons --show-keys
bitcoin-core-builder-keys.gpg | grep ^fpr`.

**Not pinned to a specific guix.sigs commit**: the fetch above always
gets the *current* builder-keys directory contents, not a hash-pinned
snapshot. This matches the spec's "PINNED inside Nodekeeper... updated
only through Nodekeeper updates" in spirit (the file is committed to
this repo and only changes when a human updates it and ships a new
Nodekeeper release), but there's no cryptographic proof this fetch
wasn't tampered with in transit beyond GitHub's own TLS — recording it
here specifically so it's easy to re-derive and re-check.

## Phase 2 — PGP library: rpgp instead of sequoia-openpgp (2026-09-22)

**STOP AND ASK, resolved.** The spec calls for "Sequoia-PGP with a
pure-Rust crypto backend... (or another library that works on all three
OSes without an external gpg)". Tried this first, via `cargo build`, not
just reading docs:
```
sequoia-openpgp = { version = "2", default-features = false, features = ["crypto-rust"] }
```
Build failed with:
```
Selected cryptographic backend: RustCrypto
The cryptographic backend RustCrypto is not considered production ready.
If you know what you are doing, you can opt-in to using experimental
cryptographic backends using the feature flag
    allow-experimental-crypto
```
Sequoia's *default* backend (`crypto-nettle`) is production-ready but
needs system `nettle`+`gmp` C libraries — fine on Linux, uncertain-to-
awkward on Windows (no standard system package; would need vcpkg or a
vendored build) — the same kind of cross-platform bundling problem the
spec is explicitly trying to avoid by asking for pure-Rust in the first
place. `crypto-openssl` avoids the *system*-library problem (OpenSSL can
be statically vendored) but adds a large C dependency to the supply
chain for exactly the kind of security-critical code the app leans on
most.

Presented the trade-offs (rpgp / vendored-OpenSSL sequoia / experimental
pure-Rust sequoia anyway) rather than picking unilaterally, since this
gates Bitcoin Core binary verification — a fail-closed security
mechanism, not a cosmetic choice. **Decision: use the `pgp` crate
(rpgp)** — pure Rust, no C dependencies, ~6.3M downloads vs sequoia's
~1.9M (crates.io, checked same day), no "not production ready"
disclaimer. Trade-off accepted knowingly: rpgp is lower-level than
sequoia's policy-based verifier (its own docs: "requires... at least a
basic understanding of cryptography"), so `nk-verify`'s verification
logic carries more of the correctness burden itself (e.g. explicitly
checking the signature's target/expiry, not getting that from a
built-in `StandardPolicy`) — noted here so a future review of
`nk-verify` knows to look at that code carefully rather than assume a
library handled it.

## Phase 2 — bitcoin.conf network-specific settings need a [section] (2026-09-23)

Caught by `nk-testkit`'s real end-to-end test failing with a startup
timeout, not by inspection. The first version of
`nk-core::bitcoin_conf::generate_bitcoin_conf` put every setting
(`txindex`, `prune`, `server`, `dbcache`, `rpcbind`, `rpcallowip`,
`rpcport`, `port`) at the top level of the file. Reproduced directly
against real `bitcoind` 31.1 (not just read from the test failure) to
find the actual cause:
```
Error: Config setting for -port only applied on regtest network when in [regtest] section.
Config setting for -rpcbind only applied on regtest network when in [regtest] section.
Config setting for -rpcport only applied on regtest network when in [regtest] section.
```
This is a **fatal startup error** in 31.1, not a warning — bitcoind
refuses to start at all. It happens even though the network is already
selected via the `-regtest` CLI flag; `bitcoin.conf`'s per-network
settings apparently aren't associated with the CLI-selected chain unless
they're explicitly inside that chain's `[section]`. `txindex`/`prune`/
`server`/`dbcache` are *not* network-specific and stayed fine at the top
level (confirmed by removing the section entirely and checking only
those four applied with no complaint).

Fixed by adding every network-specific setting under `[<section>]`, and
checked live which section name each chain actually needs rather than
guessing — in particular, mainnet's section is **`[main]`, not
`[mainnet]`** (tested directly: `[main]` with a custom `rpcport`
actually bound there and answered RPC; the section name doesn't follow
the same naming as everywhere else in the codebase, where mainnet's
`dir_name()` is `"mainnet"`). `Chain::conf_section_name()` now holds this
mapping in one place. `generate_bitcoin_conf`'s signature changed to take
`Chain` plus the RPC/P2P ports (previously ports were appended by
`nk-testkit` as a separate string concatenation, which is exactly how
this bug could have shipped unnoticed — folding port generation into the
same function that knows about sectioning removes that seam).

This is exactly why `nk-testkit`'s real bitcoind test exists per the
spec, rather than trusting a conf-generator's unit tests (which the
first, broken version also passed — they only checked "does the string
contain this line," not "does a real bitcoind accept this file").

## Phase 2 — CI-only bug: relative NK_TEST_BITCOIND path (2026-09-23)

First real GitHub Actions run for Phase 2 (`35794069542`) failed on all
3 OSes at `cargo test`, but the preceding "Fetch and verify Bitcoin
Core" step had succeeded on all 3 — so the download/verify path itself
was fine:

```
thread 'tests::starts_mines_101_blocks_and_stops_cleanly' panicked at crates/nk-testkit/src/lib.rs:188:14:
bitcoind should start: Bitcoind(Spawn(Os { code: 2, kind: NotFound, message: "No such file or directory" }))
```

Root cause: `cargo test` runs each crate's test binary with its cwd set
to *that crate's own* manifest directory, not the workspace root — not
previously known, confirmed by checking `std::env::current_dir()` from
inside a `nk-testkit` test. The CI step that captures
`fetch_bitcoin_core`'s stdout into `NK_TEST_BITCOIND` runs with
cwd=repo root, and the example printed a path relative to that cwd
(`target/nodekeeper-bitcoin-core-31.1/...`). When `nk-testkit`'s test
(cwd=`crates/nk-testkit/`) later read that same relative path back, it
resolved to a nonexistent location, so the spawn failed with `NotFound`.
This is a CI-only failure mode: every local run so far had cwd=repo
root for both the fetch step and manual `cargo test` invocations, so the
mismatch never showed up until the real CI run.

Fixed by canonicalizing the printed path (both the cache-hit and
fresh-download branches of `fetch_bitcoin_core.rs`) before printing it.
Used `dunce::canonicalize`, not `std::fs::canonicalize` — the latter
returns a `\\?\`-prefixed verbatim path on Windows, the same failure
mode already hit once and fixed in `nk-core::system_check` (verbatim
paths don't string-prefix-match the way naive code expects, and aren't
always safe to hand to `CreateProcess`/external tools). Added
`dunce = "1"` to `crates/nk-verify/Cargo.toml`.

Verified before pushing: ran the example locally to get the (cached)
absolute path, then ran `cargo test -p nk-testkit` from the workspace
root with `NK_TEST_BITCOIND` set to that path — both integration tests,
including the real regtest bitcoind spawn, passed. Pushed and confirmed
green on all 3 OSes: run `35795672861`, with `cargo test` actually
executing (not skipping) the real regtest integration test on every OS.

## Phase 3 — VERIFY: debug.log's location (2026-09-23)

Before wiring the log viewer's Tauri commands, checked live (throwaway
regtest `bitcoind` 31.1, started manually, inspected its data
directory, torn down after) rather than assuming: `debug.log` lives at
`<-datadir>/<chain-subdir>/debug.log` — the exact same directory as
`.cookie`, `bitcoind.pid`, and `wallets/` (i.e. `Environment::
bitcoin_chain_dir()`, already resolved correctly for every chain
including mainnet's no-subfolder case). Added
`Environment::bitcoin_debug_log_path()` alongside the existing
`bitcoin_cookie_path()` or that basis.

## Phase 3 — real app process-manager wiring (2026-09-23)

Built `src-tauri::node_manager::NodeManager`, the real (not test-only)
start/stop/restart/status orchestration for the dashboard, and along the
way consolidated a piece of duplicated logic: `nk-testkit`'s
`RegtestFixture` had its own private `wait_for_cookie`/
`wait_for_rpc_ready`/`poll_until` functions for "spawn bitcoind, then
wait until it's actually ready to take RPC calls." Real app code needs
the exact same sequence, and copying it a second time (into
`node_manager.rs`) would have created two copies to keep in sync.
Extracted it into `nk_proc::BitcoindProcess::start_and_wait_ready`
instead — `nk-proc` is the right owner since "knowing when a started
process is actually ready" is squarely the process manager's job, not a
test-only concern. `RegtestFixture::start` now calls the same function
real app code does; its own private polling helpers were deleted rather
than left as unused dead code.

**Scoping decision** (not a STOP AND ASK — an ordinary sequencing call,
no security/fund-safety implication): `NodeManager::start` takes an
already-known `binary_path` argument rather than locating or downloading
one itself. Bitcoin Core download-and-verify-and-install-to-a-real-
location is docs/SPEC.md item 1's setup wizard — Phase 2 built the
verify-and-download *mechanism* (`nk-verify`) but no UI flow writes a
verified binary to a real, persistent install location and remembers
its path; today that only happens in the CI/dev helper
(`fetch_bitcoin_core.rs`, explicitly `#![allow(clippy::disallowed_
methods)]`-marked as CI/dev-only) and in `nk-testkit`'s ephemeral
fixtures. Building that whole flow now would mean either a real in-app
archive extractor (no more shelling out to `unzip`/`tar`, which the
disallowed-methods rule and "never bundled or run by Nodekeeper itself"
comment both rule out for shipped code) or expanding Phase 3's scope
into the setup wizard's — neither was in this phase's task breakdown.
Instead, `start_node` reads the binary path from the existing settings
table (`bitcoind_path`, via Phase 1's `get_setting`/`set_setting`) and
returns `AppErrorCode::BinaryNotVerified` if it's unset. This makes the
dashboard's start/stop/restart controls genuinely functional end-to-end
today (verified against a real regtest node) without blocking on
building the full wizard first — set the setting manually (e.g. to the
path `fetch_bitcoin_core` prints) until a real wizard screen exists to
write it.

**CI-only bug found via the real CI run** (not caught locally — passed
every local run on this machine): the consolidation above quietly
halved a timeout budget. The original `nk-testkit` code gave the
cookie-file wait its own 30s deadline and the RPC-ready wait its own
*separate* 30s deadline (up to 60s combined, worst case). The
consolidated `start_and_wait_ready` computed one shared deadline up
front and reused it for both phases — up to 30s combined instead of 60s.
Never showed up locally (this machine is fast enough, and rarely runs
more than one regtest fixture at a time), but CI runs `cargo test
--workspace` with multiple crates' real-bitcoind tests in parallel
(now 4 concurrent `bitcoind` starts across `nk-testkit`'s 3 tests plus
`node_manager`'s new one, up from 3), and macOS/Windows runners hit the
new, tighter 30s combined budget: `panicked ... bitcoind should start:
Bitcoind(StartupTimeout)` (`nk-testkit/src/lib.rs:150` on macOS,
`:216` on Windows) — ubuntu-latest's job passed fully, only macOS/
Windows failed, consistent with a timing-margin issue rather than a
logic bug. Fixed by giving each phase its own independent deadline
again (see the updated doc comment on `start_and_wait_ready`), restoring
the original 60s worst-case combined budget. This is exactly the kind
of failure mode `DECISIONS.md`'s house rule already exists for: verify
against the real, contended CI environment, not just a fast local
machine running one thing at a time.

**Second CI-only failure, same underlying cause**: the per-phase fix
above fixed ubuntu-latest and macos-latest, but windows-latest still
failed the *next* run — a different test this time
(`dashboard_rpc_methods_return_the_expected_fields`), also
`Bitcoind(StartupTimeout)`, finishing at 32.16s against the (still)
30s-per-phase deadline. Not a logic bug: `windows-latest` GitHub-hosted
runners are known to be slower under process-spawning/I/O-heavy load
than the ubuntu/macOS runners, and this run had 4 concurrent real
`bitcoind` starts competing for the runner's limited cores. Raised
`ready_timeout` from 30s to 60s per phase at both call sites
(`RegtestFixture::start`, `NodeManager::start`) — comfortably above the
~32s observed failure point, and a reasonable margin for real users'
machines too (antivirus scanning, a slow disk, concurrent
environments), not just CI.

**Third CI-only failure — same test binary hit the same wall again**,
even at 60s: `a_fixture_dropped_without_stop_does_not_leave_an_orphan`,
`Bitcoind(StartupTimeout)`, finishing at 62.83s. The pattern across all
three failures (each one fails at just past whatever the current
deadline is) pointed away from "the timeout is too short" and toward
the real root cause: 4 real `bitcoind` processes all starting at once
is more concurrent load than a constrained CI runner can service in
any reasonable time budget, not a number worth chasing upward
indefinitely. Fixed properly this time: added `serial_test` (with its
`file_locks` feature, for cross-process — not just cross-thread —
locking) and tagged every test across `nk-testkit` and
`node_manager.rs` that starts a real `bitcoind` with
`#[serial(real_bitcoind)]`, so at most one real node starts at a time
project-wide regardless of which crate's test binary it's in. Verified
locally: one run immediately after a fresh 2m20s compile hit the same
`StartupTimeout` once (Windows Defender / disk contention from the
compile itself, still competing with the first serialized test) — a
second run moments later, nothing else competing, passed all 3
`nk-testkit` tests in 7.79s — sequential is noticeably slower per run
than the original concurrent version, but actually reliable, which
concurrent-but-flaky wasn't. **Confirmed**: pushed and watched a fourth
CI run (35908060500) — green on all 3 OSes, windows-latest's job
taking 18m13s (up from ~7-8m when concurrent, the expected cost of
serializing 4 real bitcoind lifecycles instead of racing them), but
reliably so.

## Phase 3 — VERIFY: dashboard RPC field names (2026-09-23)

Before writing the dashboard status aggregator, checked the real RPC
response shapes against a throwaway regtest `bitcoind` 31.1 (started
manually, cookie-authenticated `bitcoin-cli`, torn down after) rather
than assuming field names from memory:

`getblockchaininfo`: `blocks`, `headers`, `verificationprogress`,
`initialblockdownload` (bool), `size_on_disk`, `pruned`, `warnings`
(array). No `connections` field here (confirmed — it's on
`getnetworkinfo`, not `getblockchaininfo`, in 31.1).

`getnetworkinfo`: `connections`, `connections_in`, `connections_out`.

`getmempoolinfo`: `size` (tx count), `bytes`, `usage`, plus fee-related
fields not needed for the dashboard.

These three RPCs together cover docs/SPEC.md item 2's dashboard fields:
block height vs header height (`blocks`/`headers`), verification
progress % (`verificationprogress`), peers (`getnetworkinfo`'s
`connections`), mempool (`getmempoolinfo`'s `size`/`bytes`), disk used
(`size_on_disk`, though the spec also wants *projected* usage including
the ord index, which isn't an RPC field — computed separately from the
filesystem). Uptime isn't an RPC field either — tracked by nk-proc from
the process's own start time.

## Phase 2 — security self-review (2026-09-23)

Per CLAUDE.md/docs/SPEC.md's "Security self-review at the end of Phases 2,
5, and 7": going through docs/SPEC.md's SECURITY RULES (mandatory) line by
line, what enforces each today, and what's still a gap.

1. **RPC and ord server bind to 127.0.0.1 only; ord's address flag must be
   passed explicitly (VERIFY: defaults to 0.0.0.0).**
   RPC: `nk-core::bitcoin_conf::generate_bitcoin_conf` always writes
   `rpcbind=127.0.0.1` *and* `rpcallowip=127.0.0.1` under the chain's
   `[section]` (verified live that `rpcbind` alone does nothing without
   `rpcallowip`), tested. ord server doesn't exist yet — Phase 4 per spec.
   **Reminder for Phase 4**: Phase 0's VERIFY already found ord defaults to
   `0.0.0.0`; the ord server wrapper must pass `--address`/`--http`
   explicitly from day one.

2. **Cookie auth; secrets stored per Foundation E.**
   `generate_bitcoin_conf` never writes `rpcuser`/`rpcpassword` (tested).
   `nk-rpc::from_cookie_file` reads the real `__cookie__:<password>`
   format. `nk-secrets` (Phase 1): OS keychain (`keyring`) + Argon2id/
   XChaCha20-Poly1305 encrypted-file fallback, tested (round-trip, wrong
   password fails, tampered ciphertext fails, fresh salt+nonce per write).
   Not yet wired to a real secret-producing flow (wallet creation is
   Phase 5) — mechanism built and tested ahead of use, by design.

3. **Never log/store/transmit seed phrases or private keys; sensitive
   output channel only.**
   `nk-exec`'s `Sensitivity::Sensitive`: the broadcast stream gets
   `"[sensitive output hidden]"`, never the real content (even redacted)
   — only `execute()`'s direct return value carries it; tested. No log
   call anywhere is given `Sensitive` output — the mechanism withholds it
   at the source, not via downstream filtering. `SensitiveSeedView` (view
   → confirm-3-random-words) is built and tested but not yet wired to a
   real mnemonic-producing command. **Gap**: verified as a mechanism only
   — no real seed has flowed through the app yet, so this needs re-
   verification with a real flow at the Phase 5 self-review.

4. **Wallet-encryption passphrases never persisted; in-memory mnemonics/
   passphrases zeroized after use.**
   `nk-exec::executor.rs` zeroizes the stdin secret buffer immediately
   after writing it to the child (`stdin_data.zeroize()`). `nk-secrets`
   wraps derived keys and decrypted plaintext in `Zeroizing<...>` so they
   wipe on drop, including on an early return or panic. No real passphrase
   flow exists yet (Phase 5) — mechanism-only, correctly deferred.

5. **Backups contain only public descriptors unless the user explicitly
   creates an encrypted private backup.**
   Not started — no backup feature exists (Phase 5+). Nothing to review.

6. **All commands go through the central executor; secrets via stdin/RPC,
   never argv.**
   Mechanically enforced: `clippy.toml`'s `disallowed-methods` bans
   `std::process::Command::new`/`tokio::process::Command::new` everywhere
   except `nk-exec`/`nk-proc` (each with a documented
   `#![allow(clippy::disallowed_methods)]`), `just check` runs clippy with
   `-D warnings` so a violation fails the build — deliberately verified
   once in Phase 1 (added a violation, confirmed the build broke, removed
   it). `nk-rpc` routes every call through `Executor::record()`, never a
   bare HTTP call outside it. Tested that stdin-passed secrets never
   appear in the command display. **Gap**: no command yet actually carries
   a real secret (no ord wallet commands exist), so there's no real call
   site yet to audit for an argv violation beyond the mechanism itself.

7. **Inscription content sandboxed per Foundation D; CSP lists only exact
   ord server origins.**
   Phase 1 set `tauri.conf.json`'s CSP to `default-src 'self'` with no
   `frame-src` — the correct fail-closed baseline, since no ord origin
   should be trusted before Phase 4 adds it explicitly (documented in
   ARCHITECTURE.md "Webview security model", including what Phase 4 must
   add). Not yet applicable otherwise — inscription rendering is Phase
   4/6.

8. **All mainnet wallets are encrypted.**
   No wallet creation flow exists yet (Phase 5). Correctly deferred; flagged
   again here as a CLAUDE.md STOP-AND-ASK area, not to be improvised.

9. **Verify all binaries (pinned keys/hashes, official sources, checked by
   me); fail closed.**
   Bitcoin Core: fully implemented and tested in `nk-verify` — `>=3` valid
   signatures required from *distinct* pinned keys
   (`REQUIRED_VALID_SIGNATURES: usize = 3`), hash checked against
   `SHA256SUMS`, every error path (missing sig, too few valid sigs, hash
   mismatch, corrupt download, unparseable pinned-key bundle) fails closed
   with a typed error — tested against the real live 31.1 release (tampered
   file rejected; too few signatures rejected; a filename missing from
   `SHA256SUMS` rejected). Builder keys pinned from a fresh fetch of
   `bitcoin-core/guix.sigs` (not reused from Phase 0), source URL and fetch
   date recorded in this file. The "checked by me" half — the user
   independently verifying every fingerprint — is the still-open [MANUAL]
   item in PROGRESS.md. ord binary verification is explicitly Phase 4 per
   the spec itself (docs/SPEC.md: "ord and index options in Phase 4") —
   not a Phase 2 gap.

10. **Fund-moving actions need a preview (dry run/PSBT) and explicit
    confirmation; mainnet needs an extra step from the first fund-moving
    phase; Core spend against ord wallets blocked by default.**
    No fund-moving action exists yet (Phase 5/6). The shared `ConfirmDialog`
    (Phase 1) already has the mainnet extra step (a required checkbox that
    disables Confirm until checked) ready for Phase 5 to use, tested.
    Correctly deferred otherwise.

11. **Environments are fully isolated: no command/script/wallet
    action/template may cross environments.**
    The `Environment` model and per-chain path resolution are built and
    tested for all 4 chains, and every `nk-exec` command is tagged with its
    `environment` field. **Gap**: nothing yet actively *prevents*, at
    runtime, a command built for one environment from being issued against
    another's client — there's no such cross-environment code path to
    misuse yet, since only one environment's worth of end-to-end plumbing
    exists so far. Needs a real regression test once Phase 3 has multiple
    environments running side by side, hardened further by Phase 8's Test
    Lab.

12. **No telemetry; network calls only to the allowed list.**
    Reviewed every outbound call added so far: `nk-verify`'s `reqwest`
    calls target real bitcoincore.org release URLs only (binary +
    `SHA256SUMS` + `SHA256SUMS.asc`); `nk-rpc`'s HTTP calls target the
    local RPC URL only (127.0.0.1). No analytics/telemetry crate anywhere
    in the workspace. No auto-update code yet (Phase 10). Consistent with
    the rule.

13. **Scripts are trusted code; the runner enforces environment
    restrictions.**
    No script runner exists yet (Phase 7). Not applicable yet.

14. **A VERIFY result conflicting with any of these rules: STOP AND ASK.**
    Followed this phase: sequoia-openpgp's production-readiness gap was a
    STOP AND ASK (user chose rpgp). No SECURITY RULES conflict has arisen
    that wasn't escalated.

**Summary of open gaps carried forward** (none block Phase 2 closing, since
each is scoped to a later phase by the spec itself, but listed so they
aren't forgotten):
- ord server bind-address must be set explicitly once the Phase 4 wrapper
  exists (item 1).
- Sensitive-output/zeroization paths are mechanism-tested only, not yet
  exercised by a real secret — re-verify at the Phase 5 self-review (items
  3, 4).
- No active runtime guard against cross-environment command misuse yet —
  add a regression test once Phase 3+ has multiple live environments (item
  11).
- The two Phase 2 [MANUAL] items (mainnet bitcoind smoke test; independent
  builder-key fingerprint check) remain open in PROGRESS.md — they're the
  user's part of this phase, not something completable in-session.

## Approved deviations from SPEC.md

Decided by the project owner on 2026-09-22:

- **A. BIP39 passphrase: not supported.** Nodekeeper will not surface a
  BIP39-passphrase field anywhere in the UI (wallet create, restore, or
  elsewhere). Only the plain mnemonic (via `--from stdin`) and the separate
  Bitcoin Core wallet-encryption passphrase (via RPC `walletpassphrase`,
  never argv) are supported, both fully honoring the "secrets never via
  argv" rule with no exceptions. This is a deviation from the spec's implied
  assumption that the BIP39 passphrase is a supported, UI-exposed feature
  (spec item 3: "clearly distinguish the encryption password from an
  optional BIP39 passphrase") — it is now simply not offered. `nk-ord`
  should not expose a passphrase parameter on its wallet-create/restore
  wrappers, and `ord wallet create`/`restore` are always invoked without
  `--passphrase` (equivalent to the ord default of `""`).
- **B. Code signing: Windows yes, macOS no (for now).**
  - Windows: Nodekeeper will be code-signed. Phase 1's installer/build setup
    should assume a signing step exists in CI (cert details/secrets TBD
    when that's set up — do not block Phase 1 on acquiring the actual
    certificate, but design the packaging pipeline in Phase 10 to sign).
  - macOS: unsigned for now. The macOS first-launch flow (Phase 1 onward,
    and specifically the portable-mode App Translocation handling in
    Phase 9) must guide users through Gatekeeper's unsigned-app warning and
    quarantine-attribute removal (`xattr -dr com.apple.quarantine`), per
    spec item 12. Revisit if an Apple Developer ID is obtained later.
