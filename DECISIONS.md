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

## Phase 4 — VERIFY: ord's CLI surface and sync-status API (2026-09-24)

Re-checked live against the cached ord 0.29.0 binary from Phase 0's
spike (`spikes/ord/ord-0.29.0/ord.exe` — confirmed still `ord 0.29.0`
via `--version`) rather than trusting Phase 0's notes alone, since this
phase actually builds the process-manager code that depends on exact
flag names:

- **Chain selection**: `--chain <mainnet|regtest|signet|testnet|
  testnet4>`, or the shorthand flags `--regtest`/`--signet`/`--testnet`/
  `--testnet4` (no short form for testnet4). Note `testnet` and
  `testnet4` are *separate* values — legacy testnet3 vs. testnet4 are
  not aliases of each other in ord's CLI, so Nodekeeper must pass
  `--testnet4` explicitly, never `--testnet`.
- **Index options**: top-level flags `--index-sats`, `--index-
  addresses`, `--index-runes` (matches Phase 0's mapping). Also
  `--index-transactions`, not currently used by Nodekeeper.
- **Paths**: `--data-dir <DIR>` (ord's index location — already
  confirmed in Phase 1 to nest a chain subfolder under this even when
  given explicitly), `--cookie-file <FILE>` and `--bitcoin-data-dir
  <DIR>` for connecting to Nodekeeper's own bitcoind instead of ord's
  default (`~/.bitcoin`), matching how Nodekeeper's bitcoind is never
  in the default location.
- **`ord server` subcommand**: `--address <ADDR>` (default `0.0.0.0`,
  confirmed again — must be set to `127.0.0.1` explicitly, matches
  Phase 0), `--http` + `--http-port <PORT>` (HTTP is *not* served by
  default — only HTTPS is attempted otherwise, matching Phase 0's "both
  required" finding), `--no-sync` (exists, not used by Nodekeeper —
  always want ord indexing while its server runs), `--polling-interval`
  (default 5s — how often ord checks bitcoind for new blocks).

**Sync-status API**, verified against a real running regtest ord server
(mined 110 blocks on a real regtest bitcoind, pointed a real ord server
at it, queried the running server) — this is new ground Phase 0 didn't
cover: `GET /status` with header `Accept: application/json` (without
that header it serves the HTML explorer UI at the same path, confirmed
by checking the response `Content-Type` — an easy mistake to make).
Real response against the 110-block regtest chain:
```json
{
  "address_index": true, "sat_index": true, "rune_index": false,
  "inscription_index": true, "transaction_index": false,
  "json_api": true, "chain": "regtest", "height": 110,
  "inscriptions": 0, "runes": 0, "blessed_inscriptions": 0,
  "cursed_inscriptions": 0, "lost_sats": 0,
  "unrecoverably_reorged": false,
  "started": "...", "uptime": {"secs": 10, "nanos": ...},
  "initial_sync_time": {"secs": 0, "nanos": ...}
}
```
`height` is ord's own indexed height, not compared against the node's
height by ord itself — "is ord caught up" (docs/SPEC.md's wait-for-sync
logic) means Nodekeeper polling `/status`'s `height` and comparing it
against `getblockchaininfo`'s `blocks` (already available via nk-rpc)
until they match. The `*_index` booleans directly reflect which
`--index-*` flags were passed (confirmed: passed `--index-sats
--index-addresses`, got `sat_index: true, address_index: true,
rune_index: false` back) — a second, independent way to confirm an
environment's actual running index configuration beyond just trusting
what Nodekeeper itself passed as flags.

## Phase 4 — pinned ord 0.29.0 SHA-256 hashes (2026-09-24)

Before pinning any values, re-verified rather than reusing Phase 0's
spike findings from two days ago (`gh api repos/ordinals/ord/releases/
latest`): still **0.29.0** (published 2026-08-05), no newer release.
Confirms Phase 0's other ord 0.29.0 findings (stdin support, dry-run
flags, reinscribe syntax, index-option mapping, etc., all in this file
under the Phase 0 spike entries) are still current.

ord publishes no maintainer-signed checksums file (confirmed again —
no `SHA256SUMS`/`.asc` in the release assets or body), so per docs/
SPEC.md item 1 ("in all cases also check against SHA-256 hashes pinned
inside Nodekeeper for each supported ord version"), Nodekeeper's own
pinned hash *is* the verification, not a supplement to one. Rather than
trusting GitHub's reported per-asset `digest` field alone, downloaded
all 4 release assets for real and computed SHA-256 locally
(`Get-FileHash`) as an independent cross-check:

| Asset | SHA-256 |
|---|---|
| `ord-0.29.0-x86_64-pc-windows-msvc.zip` | `93de82db792ccc37ae385c49646c0f649d38049f4e959499c6e7c5d1a81bf2ad` |
| `ord-0.29.0-x86_64-unknown-linux-gnu.tar.gz` | `f65c758d71549954470aa7fe23b197478688fb4f910e84c2956cf9144078a94e` |
| `ord-0.29.0-x86_64-apple-darwin.tar.gz` | `a0085f296057563a31258402437c1182fc13bb9559826d1f5490feb4be6dbb75` |
| `ord-0.29.0-aarch64-apple-darwin.tar.gz` | `9360e97054a1d96624190634882c187126b02647a889b344cb601627ed1bd80c` |

All 4 matched GitHub's reported `digest` exactly. Source:
https://github.com/ordinals/ord/releases/tag/0.29.0 — the same
[MANUAL] acceptance criterion Bitcoin Core's builder keys got applies
here too: the user independently checks these same 4 hashes against
the official release from a separate machine/browser before Phase 4
closes.

Also checked the archive layout live (listed both the Windows `.zip`
and Linux `.tar.gz`, not assumed identical to Bitcoin Core's): both
nest the binary one level down in a version-named folder
(`ord-0.29.0/ord.exe` / `ord-0.29.0/ord`), same as Bitcoin Core's
release layout, **except** there is no `bin/` subfolder — the binary
sits directly in `ord-0.29.0/`, not `ord-0.29.0/bin/`.

## Phase 3 — windows-latest CI flakiness, take 4: reconsidering the diagnosis (2026-09-23)

CI run 35916368300 hit the exact same `Bitcoind(StartupTimeout)` failure
a fourth time, on windows-latest only, *despite* the `#[serial(real_
bitcoind)]` tagging from the previous fix (which was confirmed CI-green
twice already, on runs 35908060500 and 35914261232 -- so it isn't that
the tagging silently regressed).

Re-examined the timing rather than pushing another timeout increase:
the failing test (`a_fixture_dropped_without_stop_does_not_leave_an_
orphan`) took ~63 seconds to fail, but `cargo test --workspace`'s own
console output shows each test *binary* running to completion before
the next one's "Running unittests" line appears -- no interleaving.
That's consistent with cargo's actual default behavior (test binaries
run sequentially, not concurrently; only tests *within* one binary run
in parallel by default) -- which means the "4 concurrent bitcoind
processes competing for CPU" theory behind the last two fixes was
likely never the real mechanism, even though `#[serial]` tagging
happened to make the CI pass twice (plausibly by coincidence, or by
narrowing a real but different race, e.g. port allocation timing).

Tried to confirm directly: ran the full local workspace suite while
sampling `Get-Process bitcoind` every 1.5s in parallel. Never observed
more than 0-1 processes at a time locally -- but this machine has never
reproduced the failure at all, so a clean local sample doesn't rule out
real concurrency on a slower CI runner either. Inconclusive by itself.

New working hypothesis, matching the actual symptom better (a fixed,
large, intermittent per-run latency spike specific to windows-latest,
not scaling with how many *other* bitcoind processes happen to be
running): Windows Defender's real-time scan of a freshly-extracted,
freshly-executed `bitcoind.exe` is a well-documented source of exactly
this kind of first-run latency on GitHub-hosted Windows runners.
Genuinely untested until now -- added a Windows-only CI step
(`Add-MpPreference -ExclusionPath`) excluding the whole checkout before
anything is written to disk, and left the existing 60s-per-phase
timeout and `#[serial]` tagging in place rather than removing them (no
evidence they're actively harmful, even if the concurrency theory
behind them turns out to have been the wrong mechanism).

**Confirmed**: pushed and watched CI run 35918056335 — green on all 3
OSes. windows-latest's job finished in 6m31s, down from the 8-18 minute
range every previous run in this saga took (including the successful
ones), consistent with the Defender real-time scan being the actual
overhead removed. This is the real fix; the `#[serial]` tagging and
60s timeout from the earlier attempts are left in place as reasonable
belt-and-suspenders (neither is harmful, and the timeout is still a
sane bound for genuinely slow environments), but the Defender exclusion
is what actually resolved the four-attempt saga.

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

## Phase 4 — nk-ord: ord's `/status` HTTP client (2026-09-24)

Built the new `nk-ord` crate's `OrdClient::status()` — `GET /status` with
header `Accept: application/json` (DECISIONS.md's earlier Phase 4 VERIFY
entry has the exact response shape). Two implementation choices worth
recording since they're not fully dictated by PROGRESS.md's "typed
client" phrasing:

- **Returns `serde_json::Value`, not a typed `OrdStatus` struct.**
  Matches `nk-rpc::RpcClient`'s established convention exactly — its
  "typed" methods (`get_blockchain_info`, `get_network_info`, ...) are
  typed only in the sense of a fixed method signature per RPC method;
  they still return `Value`, and the caller (`NodeManager::status`)
  picks out individual fields with `.get(...).and_then(...)` and a
  default. `nk-ord::OrdClient::status()` follows the same pattern so the
  future ord status-aggregator (Dashboard's ord section, `nk-proc`'s
  wait-for-sync loop) reads the same way `NodeManager::status` does.
  Avoids a struct that would need updating every time a new `/status`
  field becomes relevant, or that silently drops fields serde doesn't
  know about.
- **New `CommandSource::OrdApi` variant** (`nk-exec`), instead of
  reusing `OrdCli`. `/status` is an HTTP GET against `ord server`, not
  an `ord` subprocess invocation — same reasoning that already separates
  `Rpc` (HTTP JSON-RPC, displayed as its bitcoin-cli equivalent) from
  `BitcoinCli` (an actual `bitcoin-cli` subprocess). Shown in the Live
  Command Monitor as the equivalent `curl -H "Accept: application/json"
  <url>` a user could run by hand (docs/SPEC.md item 7). Added
  `"ordapi"` to the Live Command Monitor's source filter
  (`LiveCommandMonitor.tsx`) alongside the existing three.

Tests are network-free (pure URL-building logic only), matching how
`nk-rpc`'s own test module has no live-network tests — the real,
against-a-running-server verification is deferred to `nk-testkit`,
which needs `nk-proc`'s `OrdProcess` (not yet built) to actually stand a
real ord server up. Live-verified during this session (separately, by
hand, not as an automated test) that a real regtest ord server's
`/status` response matches the shape this client expects to parse
(same VERIFY run recorded in the "ord's CLI surface and sync-status
API" entry above).

## Phase 4 — nk-proc: OrdProcess, and two real bugs it caught (2026-09-24)

Built `OrdProcess` (`crates/nk-proc/src/ord.rs`), mirroring
`BitcoindProcess`'s shape: `start()` (spawn, refuse a second instance on
the same data dir, pre-flight port check), `start_and_wait_ready()`
(spawn + poll `/status` until it responds or times out), `stop()`
(graceful signal + wait, with a timeout), `kill_sync()`. Two real
differences from bitcoind, both forced by how ord actually behaves
(earlier VERIFY entries in this file):

- ord writes no PID file of its own, so `OrdProcess::start` writes one
  itself (new `Environment::ord_pid_path`, `nk-core`) and
  `detect_running_ord` reads it back, mirroring `detect_running_
  bitcoind`'s own liveness check.
- ord has no RPC-based graceful stop. `stop()` sends a real signal:
  `libc::kill(pid, SIGINT)` on macOS/Linux; on Windows, a hand-rolled
  `extern "system"` binding for `GenerateConsoleCtrlEvent(CTRL_BREAK_
  EVENT, pid)` against a child spawned with `CREATE_NEW_PROCESS_GROUP`
  -- the exact mechanism Phase 0's spike (`spikes/test-createprocess-
  v2.ps1`) proved live via raw PowerShell/CreateProcessW, now exercised
  through real Rust code for the first time. `tokio::process::Command`
  exposes `creation_flags` as an inherent method on Windows, so no
  `windows-sys`/`winapi` dependency was needed (consistent with earlier
  "keep the FFI dependency tree lean" calls this session).

Extended `nk-testkit`'s `RegtestFixture` with `start_ord`/`stop_ord`
(only tests that need ord call them) and added a real end-to-end test,
`ord_starts_indexes_regtest_and_stops_gracefully`: real bitcoind, real
ord pointed at it, `/status` checked for the right chain and index
flags, then a real graceful stop, then confirms `detect_running_ord`
no longer sees it. **Ran live on this Windows machine** (not just
written and assumed): all 5 `nk-testkit` tests passed, including the
graceful Windows `CTRL_BREAK_EVENT` stop actually terminating a real
`ord.exe` process. Linux/macOS coverage of the same test now runs via
CI (see below) -- the "needs a CI job" gap Phase 0 could only note, not
close, is closed as of this commit landing on all 3 OSes.

Two real bugs were caught by this live testing, not by reasoning about
the code -- both are the kind VERIFY-before-implement exists to catch:

1. **Port-reservation race in the test fixture, not app code.**
   `RegtestFixture` originally picked `ord_port` at the same time as
   `rpc_port`/`p2p_port` (fixture construction), but `start_ord` is
   typically called much later (after `mine_blocks`, etc.). On this
   machine that gap was long enough for something else to grab the
   same ephemeral port, and `OrdProcess::start`'s pre-flight check
   correctly refused to proceed (`PortInUse`) -- the check did its job;
   the bug was reserving the port too early. Fixed by picking
   `ord_port` inside `start_ord` itself, right before use, shrinking
   the window back to the same size as `rpc_port`/`p2p_port`'s own
   (picked and used in the same function call). Not a production bug
   -- `nk-testkit`-only -- but worth recording since it could recur for
   any future fixture code that reserves a resource well ahead of
   using it.
2. **Real app bug: `ord_base_args` never told ord bitcoind's actual RPC
   port.** Confirmed live: pointing a real ord server at a regtest
   bitcoind listening on a non-default RPC port, with only
   `--cookie-file`/`--bitcoin-data-dir` passed (no RPC port), produced
   `error: Failed to connect to Bitcoin Core RPC at
   \`127.0.0.1:18443/\`` -- ord silently assumed the chain's *standard*
   RPC port (18443 for regtest) rather than reading it from the cookie
   file or bitcoin.conf. Since Nodekeeper's whole multi-environment
   model (Foundation A) depends on non-default ports being normal, not
   an edge case, this would have silently broken ord for exactly the
   configurations the app is built to support. Confirmed the fix live
   too: `ord --help` documents `--bitcoin-rpc-url <BITCOIN_RPC_URL>`
   (bare `host:port`, no scheme); adding
   `--bitcoin-rpc-url 127.0.0.1:<environment.rpc_port>` to `nk_core::
   ord_conf::ord_base_args` fixed it, verified against a real running
   pair (ord answered `/status` correctly once connected). Regression
   test added: `base_args_point_ord_at_the_environments_actual_rpc_port`.

Added CI caching + a `fetch_ord` example (`crates/nk-verify/examples/
fetch_ord.rs`, mirroring `fetch_bitcoin_core.rs` exactly: real
download+verify+extract via `nk_verify::ord`, cached by OS + pinned
version) so `NK_TEST_ORD` is set in CI the same way `NK_TEST_BITCOIND`
already is -- without this the new ord integration test would only
ever run locally.

## Phase 4 — nk-proc: wait-for-sync (2026-09-24)

Added `wait_until_caught_up(ord, bitcoin_rpc, timeout)`, completing
`nk-proc`'s three-item task list: polls ord's `/status` and bitcoind's
`getblockchaininfo` until ord's `height` reaches the node's `blocks`,
or a timeout elapses. Deliberately a free function rather than an
`OrdProcess` method -- it only needs the two already-running clients,
and its caller decides how long "open-ended" should mean (a bounded
regtest test and a real chain catching up over hours are very
different timescales), unlike `start_and_wait_ready`'s short,
always-bounded startup check.

New `OrdProcessError::SyncTimeout` (distinct from `StartupTimeout`:
ord's HTTP server was already responding, it just hadn't finished
indexing) maps to `AppErrorCode::OrdNotSynced` rather than
`IndexBehind` -- the spec lists both as separate codes with no further
distinction in its own text; read `OrdNotSynced` as the general "ord
hasn't caught up with the chain tip yet" case (this function's exact
job) and reserved `IndexBehind` for a more specific future case (e.g.
a particular `--index-*` feature the user is trying to use isn't
enabled/caught up), since that reading best fits each name.

Real regtest integration test (extends `ord_starts_indexes_regtest_
and_stops_gracefully` in `nk-testkit`): mines 5 blocks, starts ord,
calls `wait_until_caught_up`, then asserts `/status`'s `height` is
exactly 5. **Ran live on this Windows machine, passed** -- this closes
the "[CI] ord indexes regtest with all index options and stays caught
up" acceptance criterion for Windows; Linux/macOS confirmation lands
via the same CI run as the rest of this session's `nk-proc` work.

## Phase 4 — Dashboard: ord section, wired to a real OrdProcess (2026-09-24)

Extended `NodeManager` (`src-tauri/src/node_manager.rs`) to track ord
processes alongside bitcoind, per chain, in a second map
(`running_ord`) rather than folding ord into the existing one --
docs/SPEC.md item 2 treats them as independently startable/stoppable
("Start / stop / restart per service through the process manager"),
not a bundled unit. New methods: `start_ord` (refuses if bitcoind for
that chain isn't running yet -- ord always connects to Nodekeeper's own
bitcoind, never a default one), `stop_ord`, `ord_status`,
`is_ord_running`. `OrdStatus` composes ord's `/status` with bitcoind's
`getblockchaininfo` in the *same* call (not read from two separately-
polled, potentially-stale results) so `caught_up` never compares
against a stale node height.

Renamed `NodeManagerError::AlreadyRunning`/`NotRunning` to
`BitcoindAlreadyRunning`/`BitcoindNotRunning` now that ord has its own
`OrdAlreadyRunning`/`OrdNotRunning` variants -- avoids "is already
running" being ambiguous about which service.

New Tauri commands mirroring the existing bitcoind ones exactly:
`start_ord`/`stop_ord`/`restart_ord`/`ord_status`/`is_ord_running`,
reading a binary path from a new `ord_path` setting (same scoping note
as `bitcoind_path`: no setup-wizard download/verify flow writes it yet,
so this is manually configured for now).

Frontend: a new `useOrdStatus` hook (mirrors `useDashboardStatus`
exactly, deliberately separate rather than parameterized, since ord and
bitcoind are independent services with independent start/stop/restart)
and `OrdSection` component, rendered inside `DashboardScreen` below the
existing bitcoind section. Shows index height vs node height, a status
badge (Stopped/Starting/**Indexing**/Ready -- "Indexing" is
docs/SPEC.md item 2's exact wording, distinct from bitcoind's
"Syncing"), and which index options are actually enabled (reads
`/status`'s own `*_index` booleans via `OrdStatus`, not just what
Nodekeeper configured).

Found and fixed the *same* port-reservation-race bug as `nk-testkit`'s
`RegtestFixture` (see the "nk-proc: OrdProcess" entry above) in this
layer's own new integration test
(`starts_ord_reports_status_and_stops_it`): `ord_port` was picked once
at test setup, alongside `rpc_port`/`p2p_port`, then not used until
after bitcoind's own startup -- reproduced as a real `PortInUse { port
}` failure. Fixed the same way: pick `ord_port` immediately before the
`start_ord` call that actually uses it.

Verified live in the browser (`npm run dev`, the dev-Tauri-mock IPC
layer extended with `is_ord_running`/`start_ord`/`stop_ord`/
`restart_ord`/`ord_status`): started both mock services on Mainnet
(all index options off) and Regtest (all on), watched the ord badge
move Indexing -> Ready as its mocked height caught up, and confirmed
the index-options list renders correctly for both ("None enabled" vs.
"Sats, Runes, Addresses"). Real backend coverage is the
`starts_ord_reports_status_and_stops_it` test above (ran live against
real bitcoind + ord on this Windows machine, passed) -- `just check`
(fmt, clippy, full `cargo test --workspace`, frontend typecheck/lint/
vitest) all green.

## Phase 4 — VERIFY: ord restarts from its persisted index, not a reindex (2026-09-24)

Closed the other half of the "[CI] ord stops gracefully ... and
restarts without reindexing" acceptance criterion (graceful stop itself
was already covered). New `nk-testkit` test,
`ord_restarts_from_its_persisted_index_without_reindexing`: starts ord
against a real bitcoind, mines 5 blocks, waits for ord to catch up,
stops it gracefully, mines 5 *more* blocks, restarts ord against the
same data directory, and asserts its first `/status` response already
reports a height >= 5 -- not reset to (or near) 0, which is what a real
reindex from scratch would show. **Ran live on this Windows machine,
passed**: height was 5 immediately after restart, then reached 10 after
`wait_until_caught_up`.

Why this is a meaningful assertion and not just "ord probably persists
its index": ord's `/status` endpoint can't answer *at all* until its
redb index is opened, and opening an existing index file reads whatever
height was last written to it -- so a height >= 5 right after restart
is only possible if it loaded the existing `index.redb` rather than
creating a fresh one. This is exactly the same data directory across
both `start_ord` calls in the test (the fixture's `Environment` is
reused, not recreated), matching how the real app would restart ord
after a stop (same environment, same `--data-dir`).

## Phase 5 — VERIFY: ord's wallet CLI surface, live against a real encrypted wallet (2026-09-24)

Before writing any Phase 5 code, re-verified live (not trusted from
Phase 0's 2-day-old spike notes, though this confirms them) against the
same cached ord 0.29.0 binary, a real regtest bitcoind, and a real ord
server -- exactly the setup `nk-testkit` uses.

**`ord wallet` CLI surface** (`ord wallet --help` and each
subcommand's own `--help`, this exact build):
- `--server-url <SERVER_URL>` and `--name <NAME>` (default `ord`) are
  flags on the `wallet` subcommand itself, *after* `wallet`, not
  top-level flags before it (`ord <base-args> wallet --server-url
  <url> --name <name> <command>`) -- easy to get wrong, confirmed by
  hitting the "unexpected argument" error live while setting this up.
  Default `--server-url` is `http://localhost:80`, always overridden.
- Subcommands relevant to Phase 5: `create`, `restore`, `balance`,
  `cardinals`, `receive`, `send`, `addresses`, `inscriptions`,
  `transactions`, `dump`, `sign`. (`inscribe`/`batch`/`mint`/etc. are
  Phase 6+.)
- `wallet create [--passphrase <P>]`: prints `{"mnemonic": "...",
  "passphrase": ""}` to stdout -- this JSON, specifically the
  `mnemonic` field, is the sensitive output that must go through
  Foundation B's sensitive channel end to end, never the normal
  command-display/monitor/history path. Per the existing "no BIP39
  passphrase support" approved deviation, `--passphrase` is never
  passed (ord's own default is `""`).
- `wallet restore --from mnemonic --timestamp <now|unix-ts>`: reads
  the mnemonic from stdin (confirmed live: `echo "<12 words>" | ord
  ... wallet --name restored ... restore --from mnemonic --timestamp
  now` exited 0 and produced a working wallet with a real balance).
  `--from descriptor` also exists (not used by Nodekeeper's UI, which
  only offers mnemonic restore).
- `wallet send --dry-run --fee-rate <RATE> <ADDRESS> <ASSET>`: prints
  `{"txid", "psbt", "asset", "fee"}`. Confirmed this is genuinely safe
  to call at any time, including against a **locked** encrypted
  wallet -- it needs no signing key access, so a fee/PSBT preview can
  always be shown without prompting for the wallet passphrase.
- The underlying Bitcoin Core wallet `ord wallet create` makes is a
  real, ordinary Core wallet, named via `--name` (confirmed via
  `listwallets` after `wallet create`) -- not some separate ord-only
  wallet abstraction. This is *why* the encryption story below is just
  "it's a normal Core wallet," not something ord-specific.

**Encrypted-wallet interaction** (docs/SPEC.md item 3's explicit
STOP-AND-ASK trigger if this doesn't work cleanly -- it does, confirmed
live end to end, so no stop needed): created a real wallet via `ord
wallet create`, mined regtest coinbase to it, called bitcoind's
`encryptwallet` RPC directly (the standard Core RPC, not anything
ord-specific), then:
- Read-only commands (`wallet balance`, `wallet receive`) work
  identically whether the wallet is locked or unlocked -- no special
  handling needed for the parts of the UI that only read.
- `wallet send --dry-run` also works while locked (see above).
- A **real** (non-dry-run) `wallet send` while locked fails cleanly:
  `JSON-RPC error: RPC error response: RpcError { code: -13, message:
  "Error: Please enter the wallet passphrase with walletpassphrase
  first." }` -- a real, parseable RPC error, not a hang or a crash.
- Calling bitcoind's `walletpassphrase <passphrase> <timeout_secs>`
  RPC (already available via `nk_rpc::RpcClient::call`, nothing new
  needed there) unlocks it; the same `wallet send` command immediately
  afterward succeeds, producing a real signed txid.
- `encryptwallet`'s own response text ("The keypool has been flushed
  and a new HD seed was generated") sounds alarming but is Core's
  standard message about its *own* internally-generated future
  addresses -- it did not disturb ord's already-imported,
  mnemonic-derived descriptors; the wallet kept signing correctly
  afterward.

**Conclusion**: Nodekeeper's wallet-unlock flow is exactly what
Foundation D/item 3 already assumed: before any real (non-dry-run)
signing action, call `walletpassphrase` with the user-entered
passphrase and a short timeout, run the ord wallet command, then call
`walletlock` (standard Core RPC) immediately afterward rather than
waiting out the timeout. No ord-side special-casing, no workaround, no
STOP AND ASK needed -- this can be built exactly as the spec describes.

## Phase 5 — nk-ord wallet wrapper, and confirming ord's own sync gate (2026-09-24)

Built `nk-ord::wallet` (`WalletTarget` + `create_wallet`/
`restore_wallet`/`wallet_balance`/`wallet_receive`/`wallet_addresses`/
`wallet_inscriptions`/`wallet_transactions`/`wallet_cardinals`/
`wallet_send`), routed through `nk-exec` like every other `ord` CLI
invocation, `create`/`restore` tagged `Sensitivity::Sensitive` since
their I/O carries the mnemonic.

While building the real `nk-testkit` integration test (create wallet,
mine to fund it, check balance), hit a real, live failure immediately
after mining:
```
error: `ord server` 6 blocks behind `bitcoind`, consider using
`--no-sync` to ignore this error
```
`ord wallet balance` (and, by the same mechanism, every other `ord
wallet` subcommand) refuses to run at all while ord's index is behind
bitcoind's current height -- this is exactly docs/SPEC.md item 3's own
warning ("ord wallet commands depend on a running, synced ord server...
Until ord is caught up, show a clear 'Waiting for ord to catch up
(block X of Y)' state instead of errors"), now empirically confirmed
rather than just quoted from the spec. Fixed the test by calling
`nk_proc::wait_until_caught_up` after mining and before any wallet
command -- the real app must do the same: call it (or otherwise confirm
`ord_status().caught_up`) before enabling any wallet action after new
blocks arrive, and show the spec's "waiting to catch up" state rather
than letting a wallet command surface this as a raw error.

Full test (`wallet_cli_create_fund_send_and_restore`) exercises the
whole chain live: create -> receive address -> mine 101 blocks -> wait
for ord to catch up -> balance (nonzero) -> dry-run send (succeeds,
needs no unlock) -> encrypt the wallet -> real send fails while locked
-> `wallet_passphrase` unlocks it -> real send succeeds -> `wallet_lock`
re-locks it -> restore the same mnemonic under a different wallet name
with a full rescan (`--timestamp 0`) -> restored wallet's balance
matches the original's. **Ran live on this Windows machine, passed.**

## Phase 5 — VERIFY: wallets survive a bitcoind/ord restart with no explicit reload (2026-09-24)

Before designing the Wallet screen's "does this chain already have a
wallet" detection, checked live whether an `ord`-created wallet
persists and needs any explicit re-loading after a full restart --
directly relevant, since Nodekeeper's `NodeManager::start` doesn't pass
bitcoind any `-wallet=` autoload flag, and bitcoind normally starts
with zero wallets loaded.

Sequence: created a real wallet via `ord wallet create` against a real
regtest bitcoind+ord pair, confirmed `listwallets` showed `["ord"]`,
then force-killed *both* processes entirely (not a graceful stop) and
restarted them fresh from the same data directories. Immediately after
restart, before touching anything else:
- `listwallets` -> `[]` (confirms bitcoind itself never auto-loads).
- `ord wallet balance` (no explicit `loadwallet` call from Nodekeeper
  anywhere) -> succeeded immediately, and `listwallets` right
  afterward showed `["ord"]` again -- ord issues `loadwallet` itself
  before running any wallet subcommand, transparently reloading the
  existing on-disk wallet. No data loss, no silent duplicate creation.
- Confirmed it's real detect-vs-create behavior, not "any name just
  works": `ord wallet --name never-created balance` against a name
  that was never created fails cleanly: `Failed to load wallet
  never-created: ... "Path does not exist."`

**Conclusion**: Nodekeeper needs no explicit wallet-reload logic
anywhere -- every `ord wallet` command already handles this. The
"does chain X have a wallet yet" check the Wallet screen needs is just
a cheap wallet RPC (`wallet_addresses`) with `NonZeroExit` whose stderr
contains `"Path does not exist"` treated as "no wallet", not an error
(`nk_ord::wallet::wallet_exists`, added following this VERIFY).

## Phase 5 — VERIFY: address validation and fee estimation for Send (2026-09-24)

Before building the Send screen, checked live whether ord's own
`wallet send --dry-run` already validates the destination address's
network (docs/SPEC.md item 3: "reject addresses from the wrong
network"), so Nodekeeper doesn't need its own bitcoin-address-parsing
dependency just for this.

- Dry-run send to a real mainnet address (`bc1q...`) from a regtest
  wallet fails with a clean, distinguishable error: `error: validation
  error\n\nbecause:\n- address bc1q... is not valid on regtest`. ord
  already does this validation (it has to, to construct the
  transaction) -- Nodekeeper's Send screen can just call `wallet_send_
  dry_run` as both the fee/PSBT preview *and* the address-network
  check, surfacing this error directly instead of duplicating address
  parsing in Rust or JS.
- `estimatesmartfee` on regtest (no real fee market): `{"errors":
  ["Insufficient data or no feerate found"], "blocks": 0}` -- no
  `feerate` field at all, confirming docs/SPEC.md item 3's own
  expectation ("If no estimate is available (regtest, freshly synced
  node), use a configurable fallback on regtest and require manual
  entry with guidance on mainnet"). `wallet_fee_estimate` (new Tauri
  command) returns `Option<f64>` -- `None` exactly when `feerate` is
  absent, letting the frontend decide the fallback/manual-entry UI per
  chain rather than guessing from a magic number.

The "non-Taproot inscription" warning from the same SPEC item couldn't
be VERIFY'd yet -- it needs a real inscription to send, which doesn't
exist until Phase 6's inscribe flow is built. Deferred, not skipped.

## Phase 5 — Send screen: browser verification of the ConfirmDialog passphrase flow (2026-09-24)

Built `WalletSendForm` + extended `ConfirmDialog` with an optional
`passphrase` prop, then verified the full flow live in the browser (dev
server, mocked IPC extended with `MOCK_WALLET_PASSPHRASE` and a
`rememberedPassphrases` map in `dev-tauri-mock.ts`) since there is no
Tauri backend in that context to exercise `WalletSession` for real (that
part is already covered by `wallet_unlock_and_lock_gate_a_real_signing_
rpc` and `wallet_cli_create_fund_send_and_restore` in `nk-testkit`,
against a real bitcoind+ord). Sequence, on the mainnet environment:

1. Entered a regtest-prefixed address on mainnet, clicked Preview ->
   friendly error panel; "Show technical details" matched the real
   live-VERIFIED ord error text exactly: `error: validation error\n\n
   because:\n- address bcrt1q... is not valid on mainnet`.
2. Corrected to a valid mainnet address, Preview -> succeeded, fee
   shown, no absurd-fee warning (fee was small relative to the amount).
3. Clicked Send -> `ConfirmDialog` opened with the mainnet warning and
   the mainnet-ack checkbox, no passphrase field yet (the form doesn't
   know the wallet is locked until it tries).
4. Checked the mainnet ack, clicked Send inside the dialog -> mock
   rejected with `WALLET_LOCKED`, dialog stayed open and grew a
   password field with "This wallet is locked; enter its passphrase to
   continue."
5. Entered a wrong passphrase, clicked Send -> rejected inline with the
   real RPC -14 error text, dialog stayed open, mainnet ack stayed
   checked (state preserved across a failed attempt).
6. Entered the correct passphrase, checked "Remember for this session",
   clicked Send -> succeeded, dialog closed, returned to the balance
   view.
7. Started a second, independent send (fresh address/amount) -> Preview
   -> Send -> `ConfirmDialog` opened, mainnet ack checked, Send clicked
   immediately with **no** passphrase field ever appearing -> succeeded
   on the first attempt, confirming the remembered-passphrase path
   (backend tries no-passphrase-provided first, falls back to
   `WalletSession`, per the existing `wallet_send` Tauri command) works
   end to end from the UI's perspective, not just in `WalletSession`'s
   own unit tests.

This is the browser-mock half of the two [MANUAL] Phase 5 acceptance
criteria ("mainnet confirmation appears for a mainnet send"; "encrypted
wallet unlock/lock works... session remember clears after the idle
timeout") -- confirms the UI wiring is correct, but the user still needs
to check these against the real Tauri app (real mainnet-shaped
ConfirmDialog copy review, and the 15-minute idle-timeout expiry, which
isn't practical to wait out in an automated or live-browser check).

## Phase 5 — VERIFY: inscription content serving and rune balance shape (2026-09-24)

Before building the inscriptions gallery (Foundation D) and rune-balance
display, stood up a separate scratch regtest bitcoind + ord pair (not
`nk-testkit`, since this only needed manual CLI/HTTP probing, not an
automated test) to check ord's real output shapes rather than assume
them from its docs.

**`ord wallet inscriptions`** (after a real `wallet inscribe --file
test.html`, confirmed on-chain):
```json
[
  {
    "inscription": "<inscription id>",
    "location": "<txid>:<vout>:<offset>",
    "explorer": "http://localhost/inscription/<id>",
    "postage": 10000
  }
]
```
The `explorer` field is a bare `http://localhost/...` URL regardless of
the real `--server-url` used -- not useful, ignored. `inscription` (not
`id`) is the field name here, unlike the single-inscription detail
endpoint (`/r/inscription/<id>`) which uses `id` -- confirmed both are
real, distinct field names, not a typo.

**Content-serving endpoints** (`GET /content/<id>` vs `GET
/preview/<id>`), checked against both an HTML and a plain-text
inscription:
- `/content/<id>` returns the raw inscription bytes with the
  inscription's actual `content-type`, `access-control-allow-origin: *`
  (CORS-open), and ord's own CSP header allowing `'unsafe-inline'
  'unsafe-eval'` -- i.e. an HTML/SVG inscription's embedded script runs
  freely if this is what gets iframed directly.
- `/preview/<id>` always returns `text/html` -- for non-HTML content
  (checked with plain text) it's ord's own wrapper page (`<pre>` +
  `/static/preview-text.js`) that fetches `/content/<id>` client-side;
  for HTML content it serves the same bytes directly (content is already
  a full HTML document). Crucially, `/preview/<id>` sends a *tighter*
  `content-security-policy: default-src 'self'` -- scoped to ord's own
  origin, not the permissive `unsafe-inline`/`unsafe-eval` one `/content`
  sends. This is the endpoint ord itself designed for cross-embedding
  (matches how ord's own explorer embeds inscriptions), so it's the one
  the sandboxed gallery iframe's `src` uses -- never `/content`.
- The `sandbox="allow-scripts"` (no `allow-same-origin`) iframe this
  requires still works even though `/preview`'s own JS fetches
  `/content` same-origin-to-*ord*: the sandboxed iframe's origin is
  opaque/null regardless, but `/content`'s wildcard CORS header permits
  the null-origin fetch to succeed anyway -- this is exactly the
  cross-origin-embedding case ord's CORS header exists for.

**Rune balance** (`ord wallet balance`), checked with `--index-runes`
off vs on against the same funded wallet:
- Index off: `{"cardinal": ..., "ordinal": ..., "total": ...}` -- no
  `runes`/`runic` keys at all (not present-but-empty; genuinely absent).
- Index on: gains `"runes": {}` (a name -> balance map, empty since this
  wallet owns none) and `"runic": 0` (sats held in runic outputs,
  distinct from `ordinal`). Confirms Foundation F's gating is correct at
  the JSON level, not just the UI level: presence of the fields tracks
  the *running server's* actual index options, not just what Nodekeeper
  thinks it configured.
- **Gap, not resolved**: the non-empty `runes` map's per-entry value
  shape was not confirmed -- etching a real rune on the scratch regtest
  (`wallet batch` with a YAML etching + a `turbo` field ord's schema
  requires but doesn't document in `--help`) got as far as broadcasting
  a commit transaction before the command hung past a 2-minute budget
  and was killed; not worth further scratch-environment time for a
  rendering-precision detail. **Decision**: `wallet_balance`'s rune
  field is parsed and forwarded as an opaque `serde_json::Value` map
  rather than a strongly-typed `{amount, symbol, divisibility}` struct,
  and the frontend renders each entry defensively (name plus a
  best-effort stringified value) rather than a firmly-formatted amount.
  Revisit with a real typed shape once Phase 6 or later work actually
  etches/owns a rune to check against.

## Phase 5 — VERIFY: transaction history data source (2026-09-24)

`ord wallet transactions` (same scratch regtest+ord as the gallery
VERIFY above, restarted against its persisted data) returns only:
```json
[{"transaction": "<txid>", "confirmations": 0}]
```
No amount, direction, or timestamp -- too little for a useful history
list on its own. Checked bitcoind's own wallet-scoped `gettransaction
<txid>` against the same wallet (`-rpcwallet=<name>`) for what it adds:
```json
{
  "amount": 0.00000000, "confirmations": 0, "generated": true,
  "trusted": false, "txid": "...", "time": 1790275528,
  "timereceived": 1790275528, "details": [ {"address": "...",
  "category": "orphan", "amount": 50.0, "vout": 0, "abandoned": true} ],
  "hex": "...", "lastprocessedblock": {...}
}
```
Confirmed the top-level `amount` field is *already netted* across
every output for the wallet (no need to sum `details[]` manually) --
positive for a receive, negative for a send. **Decision**: the
transaction-history screen joins `ord wallet transactions`'s txid list
against `gettransaction` per txid for amount/time/confirmations/
`generated`, rather than trying to get everything from one call. Also
confirmed `confirmations` can be observed at 0 for old, previously-
mined transactions after a force-killed (not gracefully stopped)
regtest restart in this scratch environment -- an artifact of an
unclean shutdown corrupting that particular scratch chainstate back to
genesis height, not a real Nodekeeper concern: the real app only ever
force-kills as `nk-proc`'s documented cleanup-only fallback, never as
its primary stop path (Phase 2), and this was purely a throwaway manual
VERIFY environment, not `nk-testkit`.

## Phase 5 — security self-review (2026-09-25)

Per CLAUDE.md/docs/SPEC.md's "Security self-review at the end of Phases 2,
5, and 7": going through docs/SPEC.md's SECURITY RULES (mandatory) line by
line, what enforces each today, and what's still a gap. Numbering matches
the Phase 2 self-review above.

1. **RPC and ord server bind to 127.0.0.1 only; ord's address flag passed
   explicitly.**
   Unchanged from Phase 4: `nk_core::ord_conf::ord_server_args` always
   passes `--address 127.0.0.1`, tested to never emit `0.0.0.0`.

2. **Cookie auth; secrets stored per Foundation E.**
   Unchanged from Phase 1/2 -- no new secret-storage surface this phase
   (wallet passphrases are explicitly *not* persisted per rule 4, so
   `nk-secrets` isn't involved in the wallet flow at all).

3. **Never log/store/transmit seed phrases or private keys; sensitive
   output channel only.**
   **Closed with a real flow, not just the mechanism** (Phase 2's open
   item): `create_wallet`/`restore_wallet` use `Sensitivity::Sensitive`
   end to end, and `create_and_restore_wallet_never_leak_the_mnemonic_
   to_the_broadcast_stream` (`nk-testkit`) proves it with both a real
   ord-generated mnemonic and a known BIP39 test vector -- neither ever
   appears in any broadcast `ExecEvent`, run live against a real ord.
   `SensitiveSeedView` is the only place a mnemonic is ever rendered.
   **Residual gap, narrow**: the plain-text mnemonic `String` inside
   `create_wallet`'s Tauri command handler (and the one the frontend
   holds in React state before/during `SensitiveSeedView`) is never
   wrapped in `Zeroizing` -- it's return-value/UI-state data that has to
   exist in plain form to be displayed at all, and serde_json's own
   serialization step would create an unzeroized copy on the wire
   regardless of anything done on the Rust side, so wrapping it there
   has limited real benefit. Accepted as-is; flagging rather than
   pretending it's fully closed.

4. **Wallet-encryption passphrases never persisted; in-memory mnemonics/
   passphrases zeroized after use.**
   `nk-exec`'s stdin-buffer zeroization (Phase 2) now has a real secret
   flowing through it (`restore_wallet`'s mnemonic via stdin). Found and
   fixed a real gap in this session's own Send-flow code while writing
   this review: `WalletSession::remember`/`get` took/returned a plain
   `String`, forcing every caller (`wallet_send`) to hold an unzeroized
   copy of the passphrase for the whole call. Changed both to take/
   return `Zeroizing<String>` directly (`src-tauri/src/wallet_session.rs`,
   `src-tauri/src/lib.rs`'s `wallet_send`) -- the passphrase from IPC is
   wrapped in `Zeroizing` as the first thing `wallet_send` does with it,
   and every downstream use (the RPC call, the remembered copy) works
   with that wrapper, not a bare `String`. Tests updated and passing.
   **Residual gap, same shape as item 3**: the raw `String` Tauri's own
   IPC deserialization produces for the `passphrase` parameter, before
   `wallet_send`'s first line wraps it, isn't itself zeroized -- closing
   that would need the `zeroize` crate's `serde` feature enabled and the
   Tauri command parameter typed as `Zeroizing<String>` directly, which
   wasn't attempted this session (untried feature-flag combination,
   traded off against the very narrow benefit: this is a single
   deserialization pass in the user's own trusted local process, not a
   network boundary). Documented rather than guessed at.

5. **Backups contain only public descriptors unless explicitly encrypted.**
   No backup feature exists yet (not in Phase 5's task list either).
   Correctly deferred.

6. **All commands go through the central executor; secrets via stdin/RPC,
   never argv.**
   **Closed with real call sites** (Phase 2's open item): `restore_
   wallet`'s mnemonic goes via stdin (`nk_ord::wallet::restore_wallet`);
   `wallet_passphrase`/`encrypt_wallet` pass the passphrase as an RPC
   JSON param over HTTP, redacted from `command_display` (tested,
   Phase 5's `nk-rpc` unit tests). Every `ord wallet`/RPC call in this
   phase's new code goes through `nk-exec`'s `execute`/`record` -- no
   direct `Command::new` anywhere in `nk-ord`/`nk-rpc`/`src-tauri`,
   still mechanically enforced by `clippy::disallowed-methods`.

7. **Inscription content sandboxed per Foundation D; CSP lists only exact
   ord server origins.**
   `tauri.conf.json`'s `frame-src` lists exactly the 4 default ord ports
   (confirmed by reading the file directly this review, not just
   trusting PROGRESS.md's claim). `InscriptionGallery`'s `<iframe
   sandbox="allow-scripts">` has no `allow-same-origin`/`allow-forms`/
   `allow-popups`/top-navigation. **Gap, honestly restated from
   PROGRESS.md**: the actual "a malicious test HTML/SVG inscription
   cannot call Tauri IPC or read app data" acceptance criterion has not
   been exercised against the real Tauri webview -- this session's
   tooling can drive a browser tab (no real Tauri IPC to attempt calling
   at all) but not the native Tauri window, so this remains reasoned-
   through at the code level, not live-tested. Needs a human ([MANUAL])
   or a differently-tooled session to actually try it.

8. **All mainnet wallets are encrypted.**
   **Real gap found by this review, not previously flagged as clearly**:
   `create_wallet` (`src-tauri/src/lib.rs`) creates a plain, unencrypted
   ord/Core wallet for *every* chain including mainnet -- nothing calls
   `nk_rpc::RpcClient::encrypt_wallet` (built in the wallet-unlock-RPCs
   task) from any Tauri command; grepped the whole file to confirm zero
   call sites. The Send flow's passphrase-unlock UI is real and tested,
   but only against a wallet some *other* path already encrypted (in
   `nk-testkit`'s tests, the test itself calls `encrypt_wallet` directly
   -- the app never does). A user creating a real mainnet wallet through
   Nodekeeper today gets an **unencrypted** wallet with no prompt to fix
   that. This is exactly the topic CLAUDE.md names as a STOP-AND-ASK
   area ("especially around mainnet wallet encryption... never quietly
   improvise a workaround"), and the fix isn't a trivial one-liner: it
   needs a real UX decision (encrypt as part of `create_wallet` itself,
   before the mnemonic is even shown? A separate mandatory step right
   after? What happens to `restore_wallet`, which recovers an existing
   wallet that might already be encrypted or might not be?) plus a live
   VERIFY of bitcoin Core's `encryptwallet` RPC against an ord-managed
   wallet specifically -- Phase 0's VERIFY only covered *unlock/lock*
   against an *already-encrypted* wallet, not the encrypt-a-freshly-
   created-wallet transition, which has its own known Core quirk
   (`encryptwallet` reloads the wallet internally) not yet checked
   against ord's own wrapping. **Not fixed in this session** -- raised
   to the user instead of improvised.

9. **Verify all binaries; fail closed.**
   Unchanged from Phase 2/4 -- no new binary-handling code this phase.

10. **Fund-moving actions need a preview and explicit confirmation;
    mainnet needs an extra step; Core spend against ord wallets blocked
    by default.**
    Preview: `wallet_send_dry_run` always precedes a real send in
    `WalletSendForm`'s flow, and ord's own dry-run needs no unlock
    (VERIFIED live, Phase 5). Mainnet extra step: `ConfirmDialog`'s
    `isMainnet = environment.chain === "mainnet"` gate, confirmed by
    reading the component directly this review -- only mainnet requires
    the acknowledgement checkbox, matching the rule's "mainnet requires
    an extra... step" (not every environment). "Core spend commands
    against ord wallets blocked by default": still not applicable --
    no raw-RPC console or script runner exists yet (Phase 7) for a Core
    spend command to even be issued through; nothing to block yet,
    same as Phase 2's conclusion.

11. **Environments are fully isolated.**
    Stronger claim possible now than Phase 2's "mechanism only": every
    real wallet/RPC call this phase (`bitcoin_rpc_context`,
    `wallet_context`) derives its RPC URL/ord server URL/wallet paths
    from the same `chain` parameter the frontend passed into that one
    Tauri command invocation -- there is no code path today where a
    call built for one chain's environment could reach another's, since
    each command re-derives `Environment::new_default(chain, ...)` from
    scratch rather than looking up a cached/ambient "current
    environment." **Gap unchanged from Phase 2**: this is isolation by
    construction, not by an active runtime guard/test -- still worth a
    dedicated regression test once Foundation A's "more than one
    environment per chain" or Phase 8's Test Lab exist, since a future
    change to how `Environment` is resolved could silently break this
    invariant with nothing catching it.

12. **No telemetry; network calls only to the allowed list.**
    Reviewed every new outbound call this phase: `nk-ord`'s HTTP calls
    target `127.0.0.1:<ord-port>` only (local service); every RPC call
    targets `127.0.0.1:<rpc-port>` only. No new external endpoints, no
    analytics/telemetry crate added. Consistent with the rule.

13. **Scripts are trusted code; the runner enforces environment
    restrictions.**
    No script runner yet (Phase 7). Not applicable.

14. **A VERIFY result conflicting with any of these rules: STOP AND ASK.**
    Item 8 above is exactly this: a real, mandatory rule that today's
    code doesn't satisfy, on a topic CLAUDE.md names explicitly as
    STOP-AND-ASK territory. Raised to the user rather than resolved
    unilaterally in this session.

**Summary of open gaps carried forward:**
- **Mainnet wallets are not actually encrypted by the app today (item
  8)** -- the headline finding of this review. Needs a UX decision plus
  a live VERIFY of `encryptwallet` against an ord-managed wallet before
  implementing; flagged to the user, not improvised.
- Two narrow, accepted residual zeroization gaps (items 3, 4): plain-
  text copies exist briefly at the Tauri IPC boundary and in frontend
  React state, both traded off deliberately rather than chased for
  marginal benefit.
- The sandboxed-iframe/malicious-inscription acceptance criterion is
  reasoned through but not live-tested against the real Tauri webview
  (item 7) -- needs a human or different tooling.
- Cross-environment isolation is still "by construction," not by an
  active regression test (item 11, unchanged from Phase 2) -- revisit
  once multiple environments of the same chain or the Test Lab exist.
- Core-spend-blocked-by-default (item 10) and the backups rule (item 5)
  remain correctly not-yet-applicable; no code path exists for either
  yet.

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
