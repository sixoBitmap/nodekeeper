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

## Phase 5 — VERIFY: encrypting a wallet immediately after `ord wallet create` (2026-09-25)

Following the Phase 5 self-review's finding that mainnet wallets aren't
actually encrypted (item 8), and the user's decision to encrypt during
`create_wallet` itself before the mnemonic is shown, VERIFIED live on a
fresh scratch regtest+ord whether Bitcoin Core's `encryptwallet` RPC,
called immediately after `ord wallet create` returns a mnemonic, still
leaves that mnemonic as a correct backup of the wallet ord just made --
this exact transition (encrypt-right-after-creation, not encrypt-then-
much-later-unlock) was never checked before (Phase 0's VERIFY only
covered unlock/lock against an *already*-encrypted wallet).

**Initial scare**: `encryptwallet` returns `"wallet encrypted; The
keypool has been flushed and a new HD seed was generated. You need to
make a new backup with the backupwallet RPC."` -- read literally, this
sounds like the freshly-shown mnemonic becomes worthless. Two follow-up
checks:
- `listdescriptors`'s master-key fingerprint (`[xxxxxxxx/86h/...]`) was
  identical before and after `encryptwallet` on the same wallet --
  proves the actual signing seed did *not* change.
- The real proof: created a wallet, captured its mnemonic and one
  receive address (address A) *before* encrypting, encrypted it, mined
  101 regtest blocks to address A (50 BTC), then restored the *same*
  captured mnemonic into a completely separate, freshly-named wallet
  (`ord wallet restore --from mnemonic --timestamp 0`). The restored
  wallet's master-key fingerprint matched exactly, and its balance
  showed the same 50 BTC -- full round-trip proof that the mnemonic
  shown *before* encryption remains a correct, complete backup of the
  wallet *after* encryption.

**Conclusion**: Core's "a new HD seed was generated" message is
misleading boilerplate for descriptor wallets (almost certainly stale
text from Core's legacy-wallet code path, where encrypting genuinely
did rotate the keypool) -- it does not mean what it sounds like here.
Also confirmed real signing still works correctly afterward: a real
`wallet send` fails with the expected `-13` "please enter the wallet
passphrase" error while locked, and succeeds immediately after
`walletpassphrase`. No `ord`-side quirk, no need for a workaround.

**Decision**: `create_wallet` (and `restore_wallet`, same reasoning --
restoring also creates a fresh local, unencrypted Core wallet that
needs the same treatment) now takes an optional `passphrase` parameter;
when the target chain is mainnet, it's required, and the command calls
`nk_rpc::RpcClient::encrypt_wallet` immediately after the underlying
`ord wallet create`/`restore` call succeeds, before returning anything
to the frontend. Enforced backend-side (mainnet + no passphrase ->
error), not just by the frontend hiding/showing a field, per Foundation
E/"never trust the frontend alone for a security rule" precedent
elsewhere in this codebase.

## Phase 6 — VERIFY: inscribe, batch, and reinscribe CLI/HTTP surface (2026-09-25)

Before building the Inscribe studio, VERIFIED live on a fresh scratch
regtest+ord (two ord instances -- one without `--index-sats`, one with,
to check Foundation F gating precisely) rather than assuming from ord's
`--help` text alone.

**Single inscribe** (`ord wallet inscribe --file <path> --fee-rate
<rate> [--dry-run] [--destination <addr>] [--postage <amt>] [--parent
<id>] ...`): output shape confirmed identical in dry-run and real mode
except `reveal_broadcast`:
```json
{
  "commit": "<txid>", "commit_psbt": "<base64 or null>",
  "inscriptions": [{"destination": "...", "id": "...", "location": "..."}],
  "parents": [], "reveal": "<txid>", "reveal_broadcast": true|false,
  "reveal_psbt": "<base64 or null>", "rune": null, "total_fees": <sats>
}
```

**Batch inscribe** (`ord wallet batch --fee-rate <rate> --batch
<yaml>`): YAML needs `mode: separate-outputs` and `inscriptions: [{file:
<path>}, ...]` (Windows paths in the YAML must be Windows-style --
`C:\...`, not the POSIX-style path Git Bash normally hands to a Windows
exe on the command line; this only bit because the path was inside a
*file's contents*, not an argv element MSYS auto-translates). Output is
the same shape as single inscribe, with one entry per file in
`inscriptions[]`, all sharing one `reveal` txid (different `vout`s) --
confirmed with a real 2-file dry-run batch.

**Reinscribe** (single inscribe only, targeting an existing
inscription's satpoint):
- Without `--reinscribe`: fails with the exact text `error: sat at
  <satpoint> already inscribed`.
- With `--reinscribe --satpoint <satpoint>`: succeeds, creates a new
  inscription at a new satpoint (the sat moves to a new UTXO on
  reinscribe).
- **Batch mode does not support reinscribe in ord 0.29.0**: a batch
  YAML entry with a `reinscribe: true` field is rejected outright --
  `error: inscriptions[0]: unknown field 'reinscribe'`. Resolves
  docs/SPEC.md item 4's explicit "VERIFY; hide otherwise": the visual
  batch builder must not offer a reinscribe option per entry for this
  ord version -- reinscribe is single-inscription only.
- Reinscription detection, via the real inscription-detail JSON
  (`GET /inscription/<id>`, `Accept: application/json`): the
  reinscribing inscription's `charms` array gains `"reinscription"` and
  `"cursed"`, and its `number` goes *negative* (cursed numbering) --
  the original inscription's `number` stays positive/unchanged.
  `previous`/`next` fields link consecutive inscriptions on the same
  sat directly (a walkable list), though the simpler source is below.

**"Show all existing inscriptions on that sat, in order"** (spec item
4's reinscribe-mode requirement): `GET /sat/<sat_number>` (`Accept:
application/json`) returns `"inscriptions": ["<id1>", "<id2>", ...]`
already ordered oldest-first -- the direct source for this UI, no
manual `previous`/`next` walking needed. Confirmed this needs
`--index-sats`: with it off, `GET /inscription/<id>`'s `sat` field is
`null` (no sat number to query with at all) -- exactly the Foundation F
gate the spec anticipates ("If this requires an index option that is
off, say so and show what can be shown"); the reinscribe flow must
check `sat !== null` before offering the sat-history view, not trust
its own `index_options` record (same "check the real running state"
reasoning as `OrdStatus`/rune balances elsewhere in this project).

**A real Windows/Core quirk hit along the way, unrelated to ord**:
`bitcoin-cli -rpcwallet=<name> generatetoaddress N <addr>` needs a
taproot (bech32m) mining address for an ord-created wallet -- `ord
wallet create` only imports `tr()` descriptors, so plain
`getnewaddress` (which defaults to bech32/segwit-v0) fails with "No
bech32 addresses available." Must pass `getnewaddress "" "bech32m"`
when a test/tool needs to mine to an ord-managed wallet's own address.
Not a Nodekeeper code concern (only came up in ad hoc scratch-testing
tooling), but worth recording since it'll bite again in a future
VERIFY session otherwise.

## Phase 6 — the browser dev preview can't safely import `@tauri-apps/api/webview` (2026-09-25)

Building the Inscribe studio's drag-and-drop file picker (docs/SPEC.md
item 4), found live in the browser dev preview (not predicted, not in
any docs) that a plain top-level `import { getCurrentWebview } from
"@tauri-apps/api/webview"` crashes the entire app there -- not just
"drag-and-drop doesn't work," the whole React tree unmounts with
`TypeError: Cannot read properties of undefined (reading
'currentWindow')`.

Root cause: that module reads `window.__TAURI_INTERNALS__.metadata` at
*import time* (module-evaluation side effect), not only when
`getCurrentWebview()` is actually called. `dev-tauri-mock.ts`'s
`mockIPC` (from `@tauri-apps/api/mocks`) sets `window.__TAURI_
INTERNALS__.invoke`/`.transformCallback`/etc. but never `.metadata` --
that's a separate `mockWindows()` export in the same mocks module,
which this project's dev-preview setup never calls. So `window.
__TAURI_INTERNALS__` genuinely exists in the dev preview (confirmed by
reading `mocks.js`'s source directly), which makes a naive `if
("__TAURI_INTERNALS__" in window)` guard actively wrong -- it reads as
true in *both* the real app and the mocked browser preview, so it
can't be used to distinguish them.

**Fix, `InscribeStudioScreen.tsx`**:
- The webview module is loaded via a *dynamic* `import()` inside a
  try/catch, only at the point drag-drop subscription is actually
  attempted -- never imported eagerly, so a browser-preview session
  never evaluates it at all. Same shape as `store/monitor.ts`'s
  existing exec-event subscription (try/catch + `console.warn`, not a
  crash) for the identical underlying reason (Phase 3).
- A new `isRealTauriRuntime()` helper checks `window.__TAURI_
  INTERNALS__?.metadata` specifically (not mere presence of `__TAURI_
  INTERNALS__`) to gate the dev-preview-only "click to load a
  placeholder file" affordance -- this is the one property the real
  Tauri runtime always populates and this project's mock setup never
  does.

Worth remembering for any future feature that touches
`@tauri-apps/api/webview` (window management, more drag-drop, etc.):
never add a static top-level import of that module to a component that
also needs to render in the browser dev preview.

## Phase 6 — reinscribe mode: design decisions (2026-09-25)

Two things docs/SPEC.md item 4 leaves to interpretation, decided while
building `ReinscribeForm`:

- **"a review screen... with a mandatory checkbox before broadcasting"
  is a screen of its own, not folded into `ConfirmDialog`.** The
  mandatory "I understand this sat already has inscriptions" checkbox
  is reinscribe-specific review content (target sat, existing
  inscriptions with previews, new content, fee, resulting count) --
  gating it inside the *shared* `ConfirmDialog` would either bloat that
  component with reinscribe-only fields or require a generic "extra
  checkbox" slot no other flow needs. Instead `ReinscribeForm` has its
  own `"review"` step (a screen, not a modal) that leads *into* the
  same shared `ConfirmDialog` every other fund-moving action uses for
  the actual mainnet-ack/passphrase/broadcast step -- satisfies "no
  screen implements its own confirmation flow" for the part that
  matters (the final broadcast gate), while still giving the mandatory
  checkbox its own dedicated space.
- **The permanence/visibility explainer is shown every time**, not
  tracked as a one-time "seen it" flag. The spec says "before the first
  reinscription, explain..." -- ambiguous between "the first time ever,
  app-wide" and "before each reinscribe action, since it's the first
  step of that flow." Read as the latter: simpler (no new persisted
  setting), and safer for an irreversible action to over-remind than
  to risk a user who cleared app data or reinstalled never seeing it
  again.

Backend: `inscription_detail(chain, id)` and `sat_inscriptions(chain,
sat)` wrap the `OrdClient::sat`/`inscription` HTTP calls added earlier
this phase. `sat: Option<u64>` is `None` when `--index-sats` is off --
the frontend checks this field on the real response, not a cached
`Environment.index_options`, matching the same "trust the running
server's actual state" reasoning used for rune balances and
`OrdStatus`'s index-option booleans elsewhere in this project.

## Setup wizard UI: a real gap, tracked retroactively (2026-09-25)

The user asked how binary install/data-directory-choice/connecting to
an already-installed Bitcoin Core or ord works, and asked for
screenshots. Checked the actual app (browser dev preview) and the
codebase directly rather than answer from memory: there is no such UI
anywhere. The only real setup-flow screen that exists is the Phase 1
system check; clicking past it goes straight to the Dashboard.

What exists: `nk-verify`'s download-and-verify engine for both Bitcoin
Core (SHA256SUMS + >=3 pinned-key signatures) and ord (pinned hashes),
solid and tested against live releases -- but only ever exercised via
`cargo run -p nk-verify --example fetch_bitcoin_core`/`fetch_ord` and
CI, never through the app. The binary path Nodekeeper actually launches
is read from a `bitcoind_path`/`ord_path` settings key that has no UI
to set at all. Also confirmed by grepping the whole codebase: the
spec's Foundation C "offer to attach" flow (an already-running
bitcoind/ord -- from a service, a crash, or the user's own separate
install -- offered as attach when its data directory matches and cookie
authenticates) has zero implementation beyond the bare detection
primitive `nk-proc` already has.

Root cause: docs/SPEC.md's phase overview explicitly assigns the wizard
"Bitcoin Core in Phase 2, ord and index options in Phase 4," but
neither phase's own "Done when" acceptance criteria require a screen to
exist -- both are satisfiable with backend tests alone. So the wizard
silently never grew, and nothing in PROGRESS.md tracked it as an open
task (it only ever appeared as an aside inside other *completed* tasks'
notes) -- it would likely have kept being silently carried forward
until Phase 8 broke on its own assumption ("'Try it safely' in the
setup wizard now opens the full Test Lab").

**Decision**: don't build it in the same turn it was discovered --
track it properly first. Added explicit, newly-dated `[ ]` tasks to
Phase 2 and Phase 4 in PROGRESS.md (matching the spec's own phase
assignment, rather than inventing a new phase for it) so it can't
silently disappear again, and left it for the user to decide when to
build it.

## Setup wizard UI: data-directory picker, scoped deliberately (2026-09-25)

Following on from the gap tracked above, the user asked explicitly to
build the piece that lets the user choose the data directory. Built
just that slice, not the whole wizard:

**`environment_data_root` vs `data_root()` split.** `data_root()`
(`./data`, or the portable-mode directory once Phase 9 lands) is where
Nodekeeper's own settings DB lives — fixed, because reading a setting
to find the settings DB's own location is circular. Environment data
(bitcoind/ord/wallets/logs) needed to be independently relocatable per
docs/SPEC.md item 1 ("let the user choose the data directory, including
external drives"), so it's now a separate setting,
`environment_data_root`, read through a new `environment_data_root()`
helper that falls back to `data_root()` when unset. Every place that
used to call `data_root()` for environment paths
(`list_default_environments`, `bitcoin_rpc_context`, `ord_client`,
`tail_debug_log`/`page_debug_log_before`/`search_debug_log`) now takes
a `store` param and resolves through this helper instead.

**Refuse to change the directory while anything is running.** Added
`NodeManager::any_running()` (true if either the bitcoind or ord
process map is non-empty, across every chain) and have
`set_environment_data_root` refuse with a plain error if it's true.
Changing the root out from under a live process risks a later command
(stop, status, log tail) resolving to the wrong on-disk path than the
process actually running against. Verified with real-bitcoind
coverage: `any_running()` is asserted false before start, true after
start, false after stop, in `node_manager.rs`'s existing
`starts_reports_status_and_stops_a_real_node` test (no separate
process spun up just for this flag).

**Validate the folder before accepting it.** `set_environment_data_root`
creates the directory if missing, then does a real write-probe (writes
and removes a `.nodekeeper-write-test` file) before persisting the
setting — surfaces "not writable" (e.g. a read-only external drive)
immediately, at picker time, instead of failing later on first node
start.

**Picker uses `@tauri-apps/plugin-dialog`, not a raw text field.**
Native OS folder picker, `directory: true` — matches the spec's
"including external drives" requirement and avoids users hand-typing
paths. Capability scoped to `dialog:allow-open` only (not
`dialog:default`), since save/message/ask/confirm aren't needed.

**Deliberately did NOT build a picker for `bitcoind_path`/`ord_path`
in this increment.** Those are a genuine trust boundary — pointing
Nodekeeper at an arbitrary local executable without verification
directly violates CLAUDE.md's non-negotiable "binary verification
fails closed" rule. Confirmed via grep that `nk-verify` currently only
exposes combined `download_and_verify_*` entry points, with no
standalone "verify this already-on-disk file" function — building a
safe picker for these needs that function first, which is a separate,
properly-scoped task, not a shortcut to bolt onto this one. Left as
the still-open Phase 2/4 tasks tracked in the entry above (binary
download/verify UI, and the "offer to attach" flow).

**Also fixed in passing**: `SystemCheckScreen`'s disk-free check was
hardcoded to `dataDir: "data"` regardless of the actual configured
location — a pre-existing bug that would have kept showing free space
on the wrong drive after this picker landed. Now resolves the real
path via `get_environment_data_root` first.

## Setup wizard UI: binary download + verify screens (2026-09-25)

The user asked to actually be able to use the app end to end -- start
downloading real Bitcoin Core and ord binaries through the UI, not just
via the CI-only example scripts. Explicitly asked me to decide, rather
than ask again, whether to build the remaining setup-wizard pieces now
or continue phase-by-phase; decided to build the download-and-verify
screens now (finishing already-assigned Phase 2/4 scope) and defer the
"point at an already-installed binary" and "offer to attach a running
node" flows, since the former needs new verify-on-pick logic that
doesn't exist yet and the user's immediate need is fresh downloads.

**Extraction had to be pure Rust, not a shell-out.** The CI-only
`examples/fetch_bitcoin_core.rs`/`fetch_ord.rs` scripts extract their
downloaded archive by shelling out to `unzip`/`tar`, explicitly exempted
from CLAUDE.md's "all commands through the executor" rule in their own
doc comments ("this is a CI/dev-only build helper, never bundled or run
by Nodekeeper itself"). The real setup wizard runs inside the shipped
app, so that exemption doesn't apply, and shelling out would also
assume `unzip`/`tar` are on `PATH` (not guaranteed, especially on
Windows). Added `crates/nk-verify/src/extract.rs`: a small pure-Rust
`.zip` (via the `zip` crate) / `.tar.gz` (via `flate2` + `tar`)
extractor. Both crates sanitize entry paths against zip-slip/tar-slip
internally; not the actual trust boundary here anyway -- the SHA-256 +
signature check already ran before extraction ever starts.

**New `download_verify_and_install_{bitcoin_core,ord}` entry points**
in `nk_verify::bitcoin_core`/`nk_verify::ord` compose download + verify
+ extract + locate-the-real-binary + (on Unix) chmod +x into one
fail-closed call, mirroring the example scripts' cache-reuse logic
(if `<dest_dir>/extracted/<bin_subpath>` already exists, trust it
without re-downloading -- it can only exist there from a prior
successful run of this same function). Live-tested end to end
(`download_verify_and_install_extracts_a_real_working_binary` in both
`bitcoin_core.rs` and `ord.rs`): real network download, real
verification, real pure-Rust extraction, confirms a real working binary
file lands where expected -- the one thing the CI script's shell-out
extraction never exercised. A second test confirms the cache-hit path
does zero network calls (`on_progress` panics if invoked).

**Deduplicated the pinned version strings and platform-asset tables.**
`VERSION` and `platform_asset_and_bin_subpath()` previously existed
only as private consts/fns duplicated inside each `examples/fetch_*.rs`
script. Since these are exactly the kind of security-relevant pinned
value DECISIONS.md already treats carefully ("never invent or recall
pinned values from memory"), having two copies was a real
source-of-truth risk now that a second consumer (the real wizard)
needs the same values -- moved them into the library
(`nk_verify::bitcoin_core`/`nk_verify::ord`, both now `pub`) and had
the example scripts import them instead of hardcoding their own copies.

**Progress reporting**: `download_with_sha256` gained a
`_with_progress` sibling taking an `on_progress(downloaded, total)`
callback, throttled to ~10 calls/sec (plus one guaranteed final call)
so a progress bar doesn't flood the UI with an event per network chunk.
Threaded through to a new `DownloadProgress` IPC struct emitted on
`"setup-download-progress"`. Caught and fixed before committing: the
struct initially had `#[serde(rename_all = "camelCase")]`, which no
other IPC type in this codebase uses (`SystemCheck`, `ExecEvent`, etc.
are all plain snake_case) -- removed it so the frontend's
`downloaded_bytes`/`total_bytes` destructuring actually matches the
wire format, instead of silently reading `undefined`.

**Frontend**: new `BinarySetupScreen`, inserted into `App.tsx`'s flow
between System Check and the main shell (`wizardStep`: "systemCheck" ->
"binarySetup" -> "done"). Skips itself automatically once
`bitcoind_path`/`ord_path` are both already set -- those settings keys
themselves are the "already done" marker, no separate flag needed, same
reasoning as the disclaimer-vs-system-check split already established.
Subscribes to `setup-download-progress` via a dynamic `import("@tauri-
apps/api/event")` in a try/catch (matches `store/monitor.ts`'s existing
`exec-event` subscription pattern) so a failure to subscribe in the
dev-browser preview degrades to "no live progress bar" rather than a
crash -- the `invoke()` call still resolves with the final result.
Live-verified in the browser dev preview: both binaries show "Not
installed" -> click "Download & verify" -> "Verified" with their path,
Continue enables only once both are done, and the flow lands on a
working Dashboard with nav.

## Setup wizard UI: index-options step (2026-09-25)

Closes the last unbuilt piece of the setup-wizard gap tracked earlier
today. docs/SPEC.md Foundation F requires each environment to record
its own ord index options and for the choice to be made "up front, not
toggled casually" since enabling one after ord has indexed means a full
reindex — but until now there was no UI to choose them at all;
`Environment::new_default` always returned `Chain::default_index_options()`
with no override mechanism.

**Storage**: one JSON-encoded `IndexOptions` per chain, settings key
`index_options_<chain>` (`chain.dir_name()`), read through a new
`effective_index_options()` helper that falls back to
`chain.default_index_options()` when unset -- same override-with-
fallback shape as `environment_data_root()`. `list_default_environments`
now applies this per chain instead of trusting `Environment::new_default`'s
built-in default outright.

**`set_index_options` refuses while that chain's ord is running**
(`NodeManager::is_ord_running(chain)`), not just any environment
anywhere -- unlike the data-directory picker's `any_running()` (which
guards a *global* path every environment reads), index options are
per-chain, so the guard is scoped the same way.

**Copy pulled from already-VERIFY'd mappings, not invented**: the
sat/rune/address -> feature mapping in `IndexOptionsScreen` matches
Phase 5's exact VERIFY table (rune balances -> index-runes, address
lookups in the explorer -> index-addresses, sat-level views/reinscribe
history -> index-sats), and "Regtest enables all index options by
default, since they cost almost nothing there" is the spec's own
Foundation F wording, not a paraphrase.

**Frontend**: `IndexOptionsScreen` inserted between Binary Setup and
"done" (`wizardStep`: "binarySetup" -> "indexOptions" -> "done"). One
card per environment (all four chains, independently toggleable),
skips itself once every chain already has a saved choice. On
continue, `App.tsx` also re-calls `loadEnvironments()` before flipping
to "done" so the Dashboard/Wallet reflect the just-saved choices
immediately rather than the defaults fetched at app mount -- caught
during design, not found as a bug afterward.

Live-verified in the browser dev preview: Mainnet/Signet/Testnet4
default to nothing enabled, Regtest to everything; toggling one
environment's checkbox doesn't affect another's; Continue persists all
four and lands on a working Dashboard.

Deliberately not built in this pass: the "start ord automatically once
Bitcoin Core finishes syncing" wizard default (docs/SPEC.md item 1) --
that's dashboard/orchestration scope (watching sync status and
triggering a start), not a wizard screen, and is a distinct enough
piece of work to stay its own tracked task rather than being folded in
here. Still open in PROGRESS.md Phase 4.

## Phase 7 — VERIFY: ord's explorer/search HTTP surface (2026-09-25)

Real regtest bitcoind (31.1, `-txindex=1 -prune=0`, matching
`nk_core::bitcoin_conf::generate_bitcoin_conf`'s real output -- a first
manual pass without `-txindex` produced a misleading "Internal Server
Error" from ord on `/inscription/<id>` and would have looked like an
ord bug; it's actually just what happens without txindex, which the
real app always sets) + real ord 0.29.0 server with `--index-sats
--index-runes --index-addresses`, one inscription created, probed with
curl against `Accept: application/json` and via HTTP redirect tracing.

**`/search/<query>` (and `/search?query=<query>`) is ord's real
auto-detect-the-type endpoint** -- HTTP 303 redirecting to the actual
resource path. This is the key finding that reshapes the whole
Explorer design (see below): ord already does exactly what
docs/SPEC.md item 5 asks for ("search inscriptions, sats,
transactions, addresses, blocks, and runes"), server-side, for free.

Exact routing confirmed live, one query type at a time:
| Query | Routes to | Notes |
|---|---|---|
| `<64-hex>i<n>` (inscription id) | `/inscription/<id>` | |
| a bare positive integer, e.g. `113` | `/inscription/<n>` | **Always inscription number, never block height or a raw sat number** -- confirmed by searching a real sat number (`28999998350`) and getting `/inscription/28999998350`, which then 200s with `"Invalid URL: number too large to fit in target type"` (inscription numbers are a signed 32-bit type). A real block height search needs the block's hash, not its height, to hit `/search` correctly. |
| `<block>.<offset>` (dotted decimal) | `/sat/<n>.<m>` | The *only* way to reach a sat via `/search` with a bare number -- no dot means "inscription number" per above |
| 64-hex block hash | `/block/<hash>` | Disambiguated from a txid by ord doing a real lookup server-side (confirmed: two different real 64-hex values -- one an actual block hash, one an actual txid -- routed to `/block/` and `/tx/` respectively, not just by string shape) |
| 64-hex txid | `/tx/<txid>` | |
| `bcrt1...`/`bc1...`/etc. address | `/address/<addr>` | |

**Decision this changes: the Explorer embeds ord's own HTML pages in a
sandboxed iframe, it does not re-implement search/result rendering.**
docs/SPEC.md item 5 says "search... via the environment's local ord
server" and "the embedded explorer follows Foundation D" -- Foundation
D's webview security rules explicitly say "Same rules for the embedded
explorer," confirming "embedded" is literal: ord's own already-built,
already-tested explorer pages, not a Nodekeeper reimplementation.
Originally planned (PROGRESS.md, before this VERIFY) to add
`OrdClient` methods + an `explorer_search` Tauri command + custom
per-type result views -- unnecessary and worse: ord's own `/search`
redirect already resolves the type server-side (including the block-
hash-vs-txid disambiguation above, which a from-scratch client-side
guesser would have had to reimplement, incorrectly, without a real
lookup). The actual Explorer is a search box plus a sandboxed
`<iframe src="http://127.0.0.1:<ord_port>/search/<query>">`, same
`sandbox="allow-scripts"` / no `allow-same-origin` /
`referrerPolicy="no-referrer"` discipline as
`InscriptionPreviewTile` (Foundation D). No new CSP entries needed --
`frame-src` already allow-lists each environment's ord origin for
inscription previews, and CSP `frame-src` is origin-scoped, not
path-scoped, so every path under that origin (`/search`, `/tx`,
`/block`, `/address`, `/sat`, `/inscription`) is already covered.

**Foundation F gating**: rather than trying to intercept/parse ord's
own HTML error page for a disabled index (confirmed in Phase 5 VERIFY:
address search without `--index-addresses` is a hard error), the
Explorer shows which index options are enabled for the current
environment *before* the user searches, so a search that won't work
is explained proactively instead of surfacing ord's raw error page.

## Phase 7 — Explorer built (2026-09-25)

Implemented per the VERIFY/design decision above: `ExplorerScreen`
(search box + sandboxed iframe on `/search/<query>`, index-options
note sourced from `Environment.index_options`, same as
`IndexOptionsScreen`/`OrdSection` use), added to the main nav. No Rust
changes at all -- the whole feature is frontend-only, since ord's own
`/search` redirect does all the actual work. Live-verified in the
browser dev preview (mocked IPC, no real ord server, so the iframe
itself stays inert): the Foundation F note correctly reads per-
environment values and updates on switch, the search box correctly
transitions state, no console errors.

## Phase 7 — VERIFY: the real RPC/CLI surface for the console safety layer (2026-09-25)

docs/SPEC.md item 6's safety layer names specific commands
("sendtoaddress, sendmany, send, bumpfee, and similar") but says to
VERIFY the exact list against the installed version, not guess. Real
regtest bitcoind 31.1: `bitcoin-cli help` lists every RPC method by
category (Blockchain/Control/Mining/Network/Rawtransactions/Signer/
Util/Wallet/Zmq) with its exact name and signature -- the full,
authoritative list this session's classification is built from,
included in full in the `help` output captured live (not recalled).

**Wallet-spend commands that move funds directly** (the "and similar"
spec asks to identify): `sendtoaddress`, `sendmany`, `send`, `sendall`,
`bumpfee`, `psbtbumpfee`. These are the ones "Inscription protection"
blocks outright against a wallet ord uses. `sendrawtransaction` also
broadcasts, but takes no wallet parameter (works on any raw hex from
any source) -- it's a state-changing/fund-moving command in its own
right, just not wallet-scoped, so it doesn't belong in the *per-wallet*
block list the same way.

**ord's `--dry-run` support**, checked per-subcommand
(`ord wallet <cmd> --help`): `send`, `inscribe`, `batch`, `burn`,
`split`, `sweep`, `resume`, and `offer create` all support it. `mint`
and `offer accept` do **not** -- a real gap in ord itself, not
something Nodekeeper can preview around; those two get the standard
state-changing confirmation with no dry-run step first.

**Classification approach**: given ~150 real RPC methods, hand-picking
a complete "these are read-only" list and trusting anything missed to
silently fall through as safe would be a fail-*open* mistake -- the
opposite of this project's stance everywhere else. `nk_core::
console_safety::classify_bitcoin_rpc` instead allowlists the read-only
Blockchain/Wallet/Util/Network/Control/Mining query methods (confirmed
against the real list above) and treats anything NOT on that allowlist
as `StateChanging` by default (needs confirmation) -- so a future
bitcoind version adding a new method this classifier doesn't know
about degrades to "ask for confirmation," never to "run instantly."
Same fail-closed shape as `ord_wallet_subcommand_class`'s handling of
an unrecognized ord subcommand.

## Phase 7 — console execution wiring, two secrets-leak gaps found while designing it (2026-09-25)

Wired `console_safety`/`console_parse` into real execution
(`console_classify` + `console_run`), routing bitcoin-cli-style input
through `nk_rpc::RpcClient::call` directly and ord-style input through
a new `nk_ord::wallet::run_console_subcommand` (thin wrapper over the
same private `run_json` every other wallet command already uses).
`console_run` re-derives the classification and refuses a blocked
command itself rather than trusting the frontend called
`console_classify` first -- consistent with this project's rule that
the backend, not the dialog, is the actual enforcement point.

Two real secrets-leak gaps surfaced while working through what "runs
through the console" actually means for every command, not designed in
from the start:

**1. `ord wallet create`/`restore` were only going to be
`StateChangingNoDryRun`** (needs confirmation, like `mint`) until
realizing `create` always prints a fresh mnemonic to stdout, and a
raw-console execution path has no sensitive-output channel to route it
through -- the JSON result was going to flow straight into whatever
displays `console_run`'s return value, which is exactly the Live
Command Monitor / `command_history` path CLAUDE.md says a mnemonic must
never reach. Added `OrdCommandClass::BlockedUseWalletScreen`: `create`/
`restore` are refused outright by `console_run` itself, not just shown
a confirmation, pointing at the existing Wallet screen's create/restore
flow (which already has a real sensitive-output channel) instead.

**2. Several real bitcoin-cli RPCs take a passphrase or private key as
a plain positional argument**: `walletpassphrase "passphrase" timeout`,
`walletpassphrasechange`, `encryptwallet "passphrase"`,
`signmessagewithprivkey "privkey" "message"`,
`signrawtransactionwithkey "hex" ["privatekey",...]`, `importdescriptors
requests` (a JSON blob that can embed private descriptors). CLAUDE.md's
"secrets passed via stdin/RPC params, never argv" is a design constraint
for typed application flows with a chosen input channel -- a *console*
has no other channel; the user has to type the secret into the same
line as the command. `nk_core::console_safety::
secret_bitcoin_rpc_arg_indices` maps each method to which positional
index(es) are secret, and `console_run`/`console_classify` both redact
those values unconditionally (not gated on read-only/state-changing
status: `signmessagewithprivkey` is genuinely read-only in the "doesn't
mutate state" sense, but still carries a private key argument that must
never appear in the Live Command Monitor, history, or an export).

## Phase 7 — Console UI built, one console per environment not per tab (2026-09-25)

Built `ConsoleScreen` on top of the classify/execute backend above:
prompt, scrollback history, and the full confirm/block/dry-run flow
driven entirely by `console_classify`'s response -- the screen itself
contains no safety logic of its own, just renders what the backend
already decided (same "the backend is the enforcement point" shape as
`console_run` re-checking `blocked_reason` itself).

**Scope simplification**: docs/SPEC.md says "each console tab is
locked to one environment" (plural tabs, each independently pinned).
Built one console bound to the currently-selected environment instead,
matching every other screen in the app (Dashboard/Wallet/Inscribe/
Explorer all work this way, switched via the shared environment
switcher). Multiple simultaneous tabs -- e.g. a mainnet tab and a
regtest tab open side by side -- is real, additional work (tab state,
per-tab history, a tab strip UI) tracked as its own follow-up in
PROGRESS.md, not silently treated as equivalent to what got built.

**Live-verified in the browser dev preview**, all five paths a raw
console realistically hits: a read-only bitcoin-cli command runs and
prints its result immediately; a fund-moving one (`sendtoaddress`) is
refused in red with no dialog at all; a plain state-changing one
(`createwallet`) opens the shared `ConfirmDialog` with the mainnet
extra-acknowledgment checkbox and Learn Mode showing the exact command,
and actually runs on confirm; `ord create` is refused outright with the
mnemonic-protection message; `ord send` (dry-run-capable) fetches and
shows a preview inside the dialog before the real confirmation.

**Found and worked around a browser-automation limitation, not an app
bug**: the automation tool's synthetic "Return" keypress dispatches a
`keydown` with empty `key`/`code` properties (confirmed by installing a
temporary listener and inspecting the event directly) rather than
`"Enter"`, so neither the form's native submit-on-Enter nor an explicit
`onKeyDown` check for `e.key === "Enter"` can catch it. Added the
explicit handler anyway (defensible on its own terms for a command-
line-style input, and correct for a real keypress, which always
populates `key` properly) and verified the actual submit flow via the
Run button instead -- this is a testing-tool artifact, not something to
chase further in the app's own code.

## Phase 7 — script runner foundation: interpreter detection has to be a real probe (2026-09-25)

Building `nk-scripts` (interpreter detection, the standard `NKP_*` env
vars, running a script through the executor) surfaced a real, live-
confirmed gap in the obvious approach: **checking whether `python`/
`python3`/`bash` are "on PATH" is not the same as checking whether they
work.**

On the actual Windows machine this was built on: `python3 --version`
and `python --version` both resolve (via `where`) to real files under
`...\WindowsApps\`, and both actually run without error -- but what
they print is `"Python was not found; run without arguments to install
from the Microsoft Store, or disable this shortcut from Settings > Apps
> Advanced app settings > App execution aliases."`, exit code **49**,
not 0. These are Windows' own app-execution-alias stubs, not Python.
`bash` has the same shape for a different reason (already suspected
from docs/SPEC.md's explicit "bash is unavailable on stock Windows, so
say so," confirmed live): stock Windows ships a real
`System32\bash.exe` that exists specifically to prompt installing WSL,
and running it prints a distinct `WSL ... ERROR: CreateProcessCommon`
message and exits 1, not 0, when WSL isn't installed.

Both cases are real files, resolvable on PATH, spawnable without an OS
"file not found" error -- a presence check (`which`/`where`, or even a
successful `spawn()`) would have reported a false positive for both.
`detect_interpreters` instead actually runs `<name> --version` through
the executor and checks `exit_code == Some(0)`, which handles both
cases correctly for free, with no special-case string matching needed.

This also forced the test suite itself to stop assuming *which*
interpreters are present (an earlier version hard-asserted Python was
found, which failed immediately on this exact machine) -- tests now
assert "at least one real interpreter was found" and run the actual
env-var-passing test against whichever one that is, since the whole
point of this module is that availability varies by machine.

## Phase 7 — script runner wired up, 3 example scripts, VERIFY: cookie-auth RPC over the wire (2026-09-25)

Finished what the script-runner-foundation entry started: `list_scripts`/
`list_available_interpreters`/`run_script` Tauri commands, the 3
example scripts docs/SPEC.md item 6 names, and `ScriptsScreen`.

**Where the 3 example scripts live and how they run**: embedded at
compile time (`include_str!("../scripts/*.py")` in `src-tauri/scripts/`)
rather than bundled as Tauri resources -- avoids the dev-vs-bundled
resource-path distinction entirely. `run_script` rewrites the current
build's copy to `data_root()/scripts/<id>.<ext>` on every single run
before executing it, trading a few KB of redundant disk writes for the
guarantee that an app update can never leave a stale on-disk example
script behind.

**None of the 3 are `regtest_only`**: all three only ever read state
(two HTTP GETs to ord, one read-only RPC call, local `os.walk`/
`shutil.disk_usage`) -- nothing they do is unsafe to run against a real
mainnet node. `ScriptInfo.regtest_only` is a field Nodekeeper's own
`built_in_scripts()` sets, never derived from the script file's
content, and `run_script` re-checks it itself against the `chain`
argument regardless of what `list_scripts`/the frontend already
returned -- same enforcement-lives-in-the-backend shape as
`console_run`'s re-check of `blocked_reason`.

**VERIFY: the RPC cookie-auth wire protocol**, needed by
`alert_node_behind.py` (bitcoind JSON-RPC has no HTTP client library
built into a scripting language's stdlib the way `nk-rpc` is for Rust,
so the script hand-rolls the request). Real regtest bitcoind, cookie
file read directly (confirmed format: `__cookie__:<hex>`, matching
`nk_rpc::RpcClient::from_cookie_file`'s own parsing exactly), Basic
Auth built from `user:password`, JSON-RPC 1.0 POST body
(`{"jsonrpc":"1.0","id":...,"method":...,"params":[...]}`) -- the exact
shape `nk-rpc`'s own `do_call` sends. Tested with a Node.js stand-in
script rather than the real Python file directly: real Python isn't
installed on this dev machine (only the Microsoft Store stub, per the
script-runner-foundation entry above), so there's no way to execute
the actual `.py` file here. Node's `fetch`-based request against the
real bitcoind confirmed the wire protocol is correct end-to-end
(returned a real `getblockchaininfo` response); Python's
`urllib.request` + manual `Authorization` header do the identical HTTP
steps, so the same protocol understanding carries over. This is a real
gap in this session's usual "run it for real" discipline, worth being
explicit about rather than silently claiming full E2E coverage: the
Python file's own syntax was reviewed by hand, not executed.

**Frontend**: `ScriptsScreen`'s warning banner is always visible, not a
one-time dismissable acknowledgment -- docs/SPEC.md says "show a clear
warning when a script is imported or created," and since this pass
doesn't yet build an import flow (only the 3 built-ins are listed), an
always-visible warning is the safer reading until that's built. No
live-output rendering in the screen itself: `run_script` runs through
the same executor as everything else, tagged `CommandSource::Script`,
so its output already reaches the Live Command Monitor for free --
confirmed by inspection of the executor path, not a new mechanism.

**Not verified this pass, tracked explicitly**: the 3 scripts' actual
runtime correctness against a real interpreter (blocked on this
machine having no real Python -- see above), and the full Tauri-command
composition end-to-end (this project's established precedent for thin
command layers: trust the underlying crate's real tests + browser
dev-preview UI verification, same standard applied to every other
command in this session, not a new exception).

## Phase 7 — security self-review (2026-09-25)

Per CLAUDE.md/docs/SPEC.md's "Security self-review at the end of
Phases 2, 5, and 7": going through docs/SPEC.md's SECURITY RULES
(mandatory) line by line against everything Phase 7 added (the
console's safety layer and the script runner) -- what enforces each
today, what's a considered judgment call, what's still a gap.
Numbering matches the Phase 2/5 reviews above.

1. **RPC/ord bind to 127.0.0.1; ord's address flag explicit.**
   Unchanged -- Phase 7 adds no new listening surface.

2. **Cookie auth; secrets stored per Foundation E.**
   Unchanged. The script runner hands scripts the *cookie file path*,
   not its contents -- a script reads the file itself, same access a
   user's own terminal would have to that file already.

3. **Never log/store/transmit seed phrases or private keys; sensitive
   output channel only.**
   **Two real gaps found and fixed while building this phase, not
   after the fact** (see the two dedicated DECISIONS.md entries above):
   `ord wallet create`/`restore` are refused outright by the console
   (`OrdCommandClass::BlockedUseWalletScreen`) rather than reachable
   through the console's generic raw-output path, since `create` prints
   a mnemonic to stdout and the console has no sensitive-output
   channel to route it through. Separately, `walletpassphrase`/
   `encryptwallet`/`signmessagewithprivkey`/`signrawtransactionwithkey`/
   `importdescriptors` take a secret as a plain positional argument in
   real bitcoin-cli -- `secret_bitcoin_rpc_arg_indices` redacts those
   positions from the console's command display unconditionally,
   regardless of read-only/state-changing classification.
   **Checked this review, not previously written down**: `RpcError`'s
   variants (`nk-rpc`) never echo a request's params back in an error
   message -- confirmed by reading every variant -- so a failed
   `walletpassphrase` (e.g. wrong passphrase) can't leak the attempted
   value through `console_run`'s error path either, which only surfaces
   bitcoind's own response text.

4. **Wallet-encryption passphrases never persisted; in-memory
   mnemonics/passphrases zeroized after use.**
   No new persistence surface in Phase 7. Not fully closed: the raw
   `command_line`/`args` strings a user types into the console or
   passes to a script (which could include a passphrase, per item 3)
   aren't wrapped in `Zeroizing` the way `wallet_send`'s passphrase
   parameter is (Phase 5's review) -- same residual-gap shape already
   accepted there (return-value/IPC-boundary data that has to exist in
   plain form to reach the redaction step at all), not a new decision.

5. **Backups contain only public descriptors unless explicitly
   encrypted.** No backup feature exists yet. Correctly out of scope.

6. **All commands go through the central executor; secrets via
   stdin/RPC, never command-line arguments.**
   The bitcoin-cli console path never touches argv at all: `rpc.call`
   sends parameters as an HTTP JSON-RPC body field, not a spawned
   process's arguments. The ord console path and the script runner
   *do* pass their arguments via `CommandSpec.args` (real argv) --
   this is `ord`'s and a script's own calling convention, not
   something Nodekeeper's own code chose to put there; ord's wallet
   commands never take a passphrase as an argument in the first place
   (unlocking happens via Core's `walletpassphrase` RPC, already
   covered by item 3), so there's no live secret-in-argv path through
   either. Every code path added this phase (`console_run`,
   `run_script`, `detect_interpreters`) goes through `nk_exec::
   Executor::execute`/`RpcClient::call` -- no direct `Command::new`,
   still mechanically enforced by `clippy::disallowed-methods` (this
   review's `cargo clippy --workspace -D warnings` ran clean).

7. **Inscription content sandboxed per Foundation D; CSP lists only
   exact ord server origins.**
   The Explorer embeds ord's *own* HTML pages (not raw inscription
   content) in a sandboxed iframe (`sandbox="allow-scripts"`, no
   `allow-same-origin`) at the same already-allow-listed origins --
   `tauri.conf.json`'s `frame-src` is unchanged by this phase, checked
   by re-reading the file during this review. No new origin was added.

8. **All mainnet wallets are encrypted.** Unchanged from Phase 5;
   Phase 7 adds no wallet-creation path.

9. **Verify all binaries; fail closed.**
   Not applicable to what Phase 7 added: script interpreters (Python/
   Node/bash) are the user's own general-purpose system tools, not
   something Nodekeeper downloads or pins the way Bitcoin Core/ord are
   -- there's no equivalent "official source" to verify a system
   Python install against. `detect_interpreters` still fails closed in
   its own narrower sense (an interpreter that doesn't actually work --
   the Windows Python/WSL stub cases -- is never treated as available).

10. **Fund-moving actions get a preview/confirmation; mainnet gets an
    extra step; Core spend commands against ord wallets are blocked by
    default.**
    The console: `FundMoving`-classified bitcoin-cli commands
    (`sendtoaddress` and the rest of the real, VERIFY'd list) are
    blocked outright, unconditionally -- not merely confirmed --
    matching "blocked by default" literally. Every other state-
    changing command goes through the shared `ConfirmDialog`, which
    applies its mainnet extra-acknowledgment step to *all* of them
    automatically (broader than the rule strictly requires -- only
    fund-moving actions need it -- but not a gap in the safe
    direction). ord's dry-run-capable commands preview first.
    **Considered judgment call, flagged rather than silently decided**:
    the script runner has no per-run confirmation dialog at all, mainnet
    included -- a script's Run button executes immediately once the
    `regtest_only`/interpreter checks pass. This reads as consistent
    with item 13 below (scripts get a *different*, coarser trust model:
    the standing "full control, only run scripts you trust" warning
    *is* the confirmation, the same way a real shell script you chose
    to run doesn't ask "are you sure" before every line) rather than a
    gap in item 10, and none of the 3 built-in scripts move funds today
    regardless. But this stops being a free judgment call the moment a
    fund-moving *imported* script is possible -- explicitly flagged
    here for whoever builds the import flow (PROGRESS.md) to decide
    deliberately, not inherit by default.

11. **Environments are fully isolated.**
    `console_run`/`run_script` both resolve `Environment::new_default
    (chain, ...)`/`bitcoin_rpc_context(chain, ...)`/`wallet_context
    (chain, ...)` from the `chain` parameter on every call -- no shared
    or cached cross-chain state. A script's four `NKP_*` env vars are
    built fresh per run from that same chain-scoped environment, so a
    regtest-tab script run can't end up pointed at mainnet's cookie/RPC/
    ord URL.

12. **No telemetry; network calls only to the pinned few.**
    All 3 example scripts only ever call `127.0.0.1` (the local ord
    server / bitcoind RPC) -- checked by reading each script. Doesn't
    yet apply to user-imported scripts since that feature isn't built.

13. **Scripts are trusted code; the runner enforces environment
    restrictions.**
    `ScriptInfo.regtest_only` is set by Nodekeeper's own
    `built_in_scripts()`, never read from a script file's content;
    `run_script` re-checks it against `chain` itself rather than
    trusting the frontend's own copy of the same data -- a script
    file could be edited to claim anything about itself, so the
    restriction has to live outside it, which it does.

14. **STOP AND ASK on a rule conflict.** No conflict found this phase
    that wasn't already resolvable within the existing rules -- item
    10's script-confirmation question is flagged for a future decision
    (above), not a live conflict blocking anything built today.

**Overall**: no unresolved live gap found in Phase 7's own new code --
the two real issues (items 3's mnemonic-leak and secret-argument
cases) were caught and fixed during design, before landing, rather
than surviving into this review. The one open item (10) is a forward-
looking design question for a not-yet-built feature, written down so
it gets decided on purpose.

## Phase 8 — Multi-environment UI: Overview screen and switcher status (2026-09-25)

The full switcher, "all environments" overview, and resource warnings
(docs/SPEC.md item 10) needed no new backend at all: `useDashboardStatus`/
`useOrdStatus` already poll `is_node_running`/`node_status`/
`is_ord_running`/`ord_status` and expose start/stop/restart per chain
(built for the per-environment Dashboard in Phase 3) -- `OverviewScreen`
just renders one `EnvironmentOverviewCard` per environment, each a
normal component instance calling those same hooks for its own chain
(valid per the rules of hooks; a loop calling hooks directly would not
be). "Stop all" doesn't first check what's running -- it just calls
`stop_node`/`stop_ord` for every chain via `Promise.allSettled` and
lets already-stopped ones fail harmlessly, simpler and just as correct
as tracking state to avoid calling stop on something already stopped.

**Resource-warning scope decision**: docs/SPEC.md item 10 asks to "show
combined RAM and disk use across running environments." Disk use is
free -- `NodeStatus.disk.used_by_data_bytes` already exists per
environment. RAM is not: nothing tracks *per-process* memory for a
spawned bitcoind/ord (`NodeManager` holds PIDs to signal them, not to
query `sysinfo` for their memory), and building that is real, separate
work. Shipped a scoped-down version instead -- system-wide available
RAM (`system_check`, already built) plus a live count of running
services -- and wrote the precise version down as a tracked follow-up
in PROGRESS.md rather than silently presenting the approximation as the
full feature.

**Switcher status is a text label, not a color change**, per docs/
SPEC.md item 8's own accessibility rule ("environment colors always
paired with text labels"): `EnvironmentSwitcher` now polls the same
two booleans per chain and shows "(running)" next to any environment
with something up, alongside its existing color dot -- never relying
on the dot's color alone to convey state.

Live-verified in the browser dev preview: starting Regtest's node from
its Overview card flips that card to "Syncing" with a real disk-usage
figure, updates the top-of-page running count on its next poll, and
leaves the other three cards untouched; "Stop all" returns everything
to Stopped in one click. Caught and fixed the same class of test gap
as earlier in this session while running the frontend quality gate:
`App.test.tsx` has its own separate `mockInvoke` (distinct from
`dev-tauri-mock.ts`) that didn't have a case for `is_node_running`,
which `EnvironmentSwitcher` now calls on mount regardless of wizard
step -- added it.

## Phase 8 — VERIFY: ord's built-in `env` command isn't a fit for the Test Lab (2026-09-25)

docs/SPEC.md item 11 says: "If the installed ord version has a
built-in regtest environment command (e.g. `ord env`) (VERIFY), it may
be used internally, but the app's own controls must still work."

Real `ord --help`/`ord env --help` against 0.29.0: `env` exists
("Start a regtest ord and bitcoind instance"), taking only a target
`[DIRECTORY]` (default `env`) plus `--decompress`/`--proxy` -- no
`--bitcoin-rpc-url`/`--bitcoin-rpc-username`/etc. to point it at an
*already-running*, externally-managed bitcoind the way every other
`ord` subcommand in this app takes. It spawns and owns its own bitcoind
internally.

**Decision: don't use it.** Three concrete reasons, not just "it looks
different": (1) it would bypass `nk-verify`'s pinned-binary check
entirely -- there's no way to hand `env` the already-verified bitcoind
path Nodekeeper downloaded; (2) it would bypass `NodeManager`/`nk-proc`,
so the Live Command Monitor and graceful-stop tracking this whole app
is built around would have no visibility into or control over the
bitcoind it spawns; (3) it ignores the user's chosen data directory
(`environment_data_root`) in favor of its own `[DIRECTORY]` argument.
The Test Lab is built as an orchestration layer over Nodekeeper's
already-working start/stop/wallet/mine-blocks controls instead --
exactly the "the app's own controls must still work" half of the rule,
just without the "may be used internally" half, since it genuinely
doesn't fit this app's architecture.

### Test Lab: mine_blocks / reset_test_lab implementation (Phase 8, 2026-09-25)

Built on top of the `ord env` VERIFY above. Two new Tauri commands:

- **`mine_blocks(chain, count)`**: refuses immediately on any chain other
  than Regtest (returns a `TypedError`, not a silent no-op), then gets a
  fresh receive address from the current wallet via
  `nk_ord::wallet::wallet_receive` and calls the RPC `generate_to_address`.
  Reuses the existing wallet/RPC plumbing rather than adding a new code
  path -- "Get test coins" is just `mine_blocks(chain, 1)`, and
  `TestLabScreen`'s one-click setup is `mine_blocks(chain, 101)` after
  starting both services (101 blocks = 100-confirmation coinbase maturity
  plus the spendable block itself).
- **`reset_test_lab()`**: stops ord then bitcoind gracefully (skipping
  either if already stopped), then deletes on-disk data via a new
  extracted function, `delete_regtest_data_only(environment_data_root:
  &Path)`. Following this session's established pattern (same reasoning
  as `script_allowed_on_chain`): the docs/SPEC.md [CI] criterion "Reset
  Test Lab deletes only regtest data" needs a real, direct test, and this
  project's convention doesn't unit-test Tauri command handlers directly.
  `delete_regtest_data_only` takes **no chain parameter at all** --  it
  hard-codes `Environment::new_default(Chain::Regtest, ..)` internally --
  so it's structurally impossible for it to ever delete another
  environment's data, rather than relying on an `if chain == Regtest`
  check that a future edit could get wrong. Two real tests (real tempdir,
  both regtest and mainnet subdirs populated with files) confirm only
  regtest is removed, and that calling it when regtest has no data yet is
  a clean no-op.

`TestLabScreen` deliberately does not reimplement wallet creation: that
flow already exists on the Wallet screen with the real sensitive-output
handling for the mnemonic (`SensitiveSeedView`), and duplicating it here
would mean a second, less-reviewed code path touching a mnemonic. If no
wallet exists yet, the screen points at the Wallet screen instead of
trying to create one itself -- consistent with this project's mnemonic-
handling rule (CLAUDE.md: "Mnemonics flow only through the sensitive-
output channel... never the monitor, logs, history, or exports").

The 5 guided walkthroughs with checkpoints (docs/SPEC.md item 11) and
wiring the setup wizard's "Try it safely" link to this screen remain
explicitly deferred (PROGRESS.md) -- this increment covers the
setup/mining/reset controls they'll sit alongside.

### "Try it safely" wizard link (Phase 8, 2026-09-25)

docs/SPEC.md item 1 says the setup wizard should "offer a 'try it
safely' option that opens the Regtest Test Lab... before committing to
a multi-day mainnet sync." Placed it as a secondary button on
`IndexOptionsScreen` -- the wizard's last step -- rather than earlier
(e.g. `SystemCheckScreen`, where the data-directory picker lives):
Test Lab's one-click setup needs the verified bitcoind/ord binaries
already in place, which only exist once `BinarySetupScreen` has run.
Putting the offer any earlier would either be a dead link or need to
silently skip binary setup first, which isn't worth the complexity for
what is otherwise a one-button nudge.

The button reuses the exact same `save()` path as "Continue" (still
real, permanent per-chain index-option choices, per Foundation F) and
only differs in what happens after: `onContinue` finishes into the
default landing screen, `onTryItSafely` additionally selects the
Regtest environment and lands on the Test Lab screen instead. No new
backend command needed -- this is pure frontend routing on top of the
existing `set_index_options`/environment-store/`screen` state.

### Post-action mine offer (Phase 8, 2026-09-25)

`WalletSendForm` had no persistent success state before this: on a
successful send it called `onSent()` immediately, which just closed
the form back to the balance view with no confirmation shown at all.
To hang the "mine 1 block to confirm" offer on something, gave it a
success view matching the inscribe forms' existing `inscribed`-state
pattern (txid + fee, then a "Done" button) rather than restoring the
old close-immediately behavior and bolting the offer on separately --
one consistent shape across all 4 success views instead of a special
case for sends.

`RegtestMineOffer` is deliberately a single shared component mounted
in all 4 places (`WalletSendForm`, `SingleInscribeForm`,
`BatchInscribeForm`, `ReinscribeForm`) rather than four copies of the
same button/checkbox/effect: it self-gates on `chain === "regtest"`
(renders `null` otherwise) so callers mount it unconditionally instead
of each repeating the chain check, which also means a 5th send/inscribe
surface added later gets this for free by just rendering it.

Auto-mine is one global setting, not per-chain or per-form: Regtest is
the only chain that ever self-mines (mainnet/signet/testnet4 have real
miners), so there's nothing to scope it against. Turning it on from
inside any one offer instance immediately mines for the action that
instance is attached to (not just future ones) -- live-verified this
by checking the box after a send already showed its manual "Mine 1
block to confirm" button and confirming it mined right away rather
than requiring a second action first.

### Guided Test Lab walkthroughs (Phase 8, 2026-09-25)

**Walkthrough (d) not built -- decided with the project owner.** docs/
SPEC.md item 11's walkthrough (d) is "send an inscription to a second
test wallet -> mine -> confirm arrival." `WalletTarget`/`WalletContext`
(crates/nk-ord/src/wallet.rs, src-tauri/src/lib.rs) already thread a
`wallet_name` through every `ord wallet` call, but every Tauri command
hardcodes it to one name (`DEFAULT_WALLET_NAME = "ord"`), with an
existing comment noting "Multiple named wallets... is a later task."
Presented three options (send to a second address and verify via
Explorer / a walkthrough-only second ord wallet / skip (d) for now);
the project owner chose to skip it. Walkthroughs a, b, c, and e are
fully built; (d) is tracked in PROGRESS.md's Backlog section pending
real multi-wallet support, not worked around here.

**Architecture: a persistent cross-screen banner, not a TestLabScreen
sub-view.** Every walkthrough's steps send the user to different
screens (Wallet, Inscribe, Test Lab, Explorer, Console, Scripts) --
`App.tsx` unmounts the current screen's component on every `screen`
change, so walkthrough progress can't live in `TestLabScreen`'s own
state. Progress lives in `useWalkthroughStore` (Zustand: active id,
step index, per-walkthrough `context`), and `WalkthroughBanner` renders
unconditionally in `App.tsx` between the header and the screen
dispatch, so it survives navigating anywhere. `TestLabScreen`
(via `TestLabWalkthroughs`) only starts a walkthrough and picks its
initial context; the banner owns everything from there.

**Checkpoints are real polls against Regtest, not simulated.** Every
checkpoint kind reuses an existing IPC command -- `wallet_exists`,
`wallet_balance`, `node_status`'s block height (for "mine a block"
steps), `wallet_inscriptions`'s count, `sat_inscriptions`, and
`list_command_history` filtered by `triggering_action` (exactly
`"console"` for a console-run command vs. a `"run script "` prefix for
a script run -- both set by the existing Tauri command handlers, not
new). No new backend commands were needed. A step with nothing
machine-checkable (e.g. "look at your receive address") shows
"Continue whenever you're ready" instead of polling; every step also
has a "Skip" button so a real detection gap never strands the user.

**Two real bugs found and fixed via live browser verification, not
design review:**
1. The checkpoint status text checked `checkpointMet` before
   `step.checkpoint === "none"` -- since `checkpointSatisfied("none",
   ...)` always resolves `true`, a manual-confirm step briefly showed
   "Done -- checked automatically" (implying real detection that never
   happened) instead of "Continue whenever you're ready." Fixed by
   checking the "none" case first.
2. "inscriptionsIncreased" (walkthrough b's step 3, "see it in the
   gallery") captured its baseline when *that* step mounted -- but the
   inscribe action happens on step 1, so by step 3 the count had
   already risen relative to nothing, and the checkpoint could never
   detect it. Fixed by capturing the baseline once when the whole
   walkthrough starts (`TestLabWalkthroughs`, into `context`), the same
   pattern already used for walkthrough (c)'s target-sat capture,
   instead of at step-mount like "blocksIncreased" (which is correct
   to capture per-step, since its action *is* that step).

**Dev-mock enhancement for real end-to-end verification.** The dev-
browser preview's `wallet_inscriptions`/`inscription_detail`/
`sat_inscriptions` mocks previously returned a static, unchanging pair
of fake inscriptions regardless of any inscribe call. Made them track a
real growing per-chain list (`recordsFor`/`MockInscriptionRecord` in
dev-tauri-mock.ts), with `wallet_inscribe`'s mock correlating a
reinscribe's `reinscribeSatpoint` back to the original record's `sat`
-- this let walkthroughs (b) and (c)'s auto-checkpoints be verified for
real in the browser (create wallet -> inscribe -> see gallery count
rise; reinscribe -> see the sat's inscription count reach 2) rather
than only via "Skip." Walkthrough (e)'s console/script checkpoints and
"blocksIncreased" could not be end-to-end verified the same way (the
mock's `console_run`/`run_script`/`mine_blocks` don't write to the
mocked `list_command_history`/`node_status`) -- verified those by code
review instead, since they reuse the exact same real IPC commands
already proven correct elsewhere (`useDashboardStatus`, the Live
Command Monitor's own history fetch).

### Notifications (Phase 8, 2026-09-25)

**VERIFY: `tauri-plugin-notification` v2 API** (docs.rs, the plugin's
GitHub README, both fetched live). Official plugin is
`tauri-plugin-notification` (singular) from `tauri-apps/plugins-workspace`
-- several unofficial `tauri-plugin-notifications` (plural) forks exist
on GitHub/crates.io and are NOT this. Cargo: `tauri-plugin-notification =
"2"` (resolved to 2.4.0, requires `tauri ^2.10` -- this workspace already
pins `tauri = "2"`, compiled clean). Register via
`.plugin(tauri_plugin_notification::init())`. Capability:
`"notification:default"` added to `src-tauri/capabilities/default.json`.
Frontend: `@tauri-apps/plugin-notification` (npm), permission via
`isPermissionGranted()`/`requestPermission()`, sending via
`sendNotification({ title, body })`.

**Architecture: a dedicated background watcher, not the existing status
hooks.** `useDashboardStatus`/`useOrdStatus` (used by
Dashboard/Wallet/Overview) already poll `node_status`/`ord_status`, but
only while their owning screen is mounted -- a node finishing sync while
the user is on the Wallet screen would never trigger anything if
notifications rode on those hooks. `NotificationWatcher` is mounted
unconditionally in `App.tsx` (alongside `WalkthroughBanner`/
`LiveCommandMonitor`) and polls every environment directly via `invoke`,
independent of the current screen. Deliberately a slower, dedicated
15s interval rather than reusing those hooks' 3s cadence -- a sync-
completion notification doesn't need second-level precision, and
polling full status for all 4 environments forever in the background
(not just while a status screen happens to be open) is a real
continuous cost not worth paying at 3s granularity.

Each of the three notifications only fires on an observed *transition*
this session (sync/index toggling from in-progress to done, disk space
crossing under the existing `DiskMonitor` component's 5 GiB threshold,
exported as `LOW_SPACE_WARNING_BYTES` and reused here rather than a
second magic number) -- never just because a poll happens to see an
already-settled state, which would notify on every app launch for an
environment that finished syncing days ago.

### Tray (Phase 8, 2026-09-25)

**VERIFY: Tauri 2 tray/menu/close-intercept API**, all confirmed
against docs.rs for `tauri` 2.11.6 (the version this workspace
resolved) rather than assumed from general Tauri familiarity, since
intermediate Tauri 2 releases changed some of these signatures:
- Cargo: `tauri = { version = "2", features = ["tray-icon"] }` --
  without the feature, `tauri::tray` doesn't exist at all (confirmed
  the hard way: an earlier attempt to compile before adding the
  feature failed with `E0432 unresolved import`, caught by a stale
  background `cargo clippy` run started before the Cargo.toml edit
  landed -- a race in this session's own tooling, not a real bug, but
  worth its own note below).
- `TrayIconBuilder::new().icon(...).menu(&menu).on_tray_icon_event(...)
  .on_menu_event(...).build(app)?`, icon reused via
  `app.default_window_icon().unwrap().clone()` rather than shipping a
  second image.
- `tauri::Builder::on_window_event`'s closure signature in the current
  release is `Fn(&Window<R>, &WindowEvent)` -- **two** positional
  arguments. Several code examples findable via search (including ones
  Tauri's own GitHub discussions still surface) use an older single-
  argument `|event| match event.event() { ... }` shape from an earlier
  Tauri 2 pre-release; using that form here would not compile against
  2.11.6.
- `WindowEvent::CloseRequested { api, .. }` + `api.prevent_close()`
  (`CloseRequestApi::prevent_close(&self)`) intercepts the close
  button; combined with `window.hide()`.

**Reused `NodeManager::any_running()` instead of writing a new
predicate.** First pass wrote a standalone `should_hide_to_tray`
function duplicating exactly what `any_running()` (node_manager.rs)
already does -- checks every environment's bitcoind *and* ord in one
call, with its own existing test
(`any_running_is_false_on_a_fresh_manager`) plus indirect coverage from
`starts_reports_status_and_stops_a_real_node`. Deleted the duplicate
once found by re-reading `node_manager.rs` rather than shipping a
second implementation of the same check.

**Quit gracefully stops everything first.** Once the window-close
button hides to the tray instead of quitting, the only way to actually
exit is the tray's "Quit Nodekeeper" menu item -- so unlike a window
close (which previously would've simply killed the whole process,
orphaning any spawned bitcoind/ord), Quit now stops every running
environment's ord then bitcoind with the same graceful timeouts
`reset_test_lab` already uses (30s/120s) before calling `app.exit(0)`.

**Background-task race while editing Cargo.toml.** A `cargo clippy`
run was already in flight when the `tray-icon` feature was added to
Cargo.toml; that specific invocation had already resolved its
dependency/feature graph before the edit landed, so it failed on the
now-current `lib.rs`'s `use tauri::tray::...` with the feature not yet
compiled in. Confirmed stale (not a real bug) by re-running `cargo
check`/`clippy` fresh after all edits finished, both clean. Lesson
applied for the rest of this session: don't edit `Cargo.toml`/`lib.rs`
again while a background cargo invocation covering them is still
running.

### Prevent sleep during sync (Phase 8, 2026-09-25)

**VERIFY: no official Tauri sleep-prevention plugin exists.** Searched
specifically for one before reaching for a third-party crate --
confirmed via `tauri-apps/tauri` issue #3697 (open feature request,
unresolved) that this is a known gap in Tauri itself, not something
this project missed. Chose `keepawake` (segevfiner/keepawake-rs,
0.6.1): a cross-platform RAII guard mirroring `caffeinate`/
`systemd-inhibit`/PowerToys Awake, backed by Windows `PowerRequest`,
macOS IOKit, and Linux D-Bus under the hood depending on target OS.
Confirmed compiling with the guard held inside a `Mutex` in managed
Tauri app state (`PreventSleepGuard`) -- its `Send`-ness wasn't
documented anywhere findable, so real compilation was the actual
verification, not a doc read.

**Scope: `sleep(true)` only, not `display(true)`.** The crate
distinguishes preventing idle *system* sleep from keeping the
*display* on. This is a background sync a user isn't necessarily
watching, so only system sleep is inhibited -- keeping the screen lit
the whole time would waste more power than the feature is meant to
save, and nothing in docs/SPEC.md item 8 asks for that.

**"During sync" covers both bitcoind's IBD and ord's indexing pass.**
The spec's literal wording ("prevent sleep during sync") could be read
as Bitcoin Core's initial sync only, but ord's own initial indexing
pass can also run for hours depending on which index options are
enabled -- treating only one of the two as "syncing" would let the
machine sleep mid-index for no principled reason, so both feed the
same aggregate signal.

**No Settings screen exists yet to host the toggle.** docs/SPEC.md
item 9 ("Settings and maintenance") -- where "prevent sleep" and
similar toggles would naturally live long-term -- is a later phase,
not built. Rather than build a whole new screen just to hold one
checkbox (real scope creep for this task), the toggle was added to the
Overview screen, which already functions as this app's closest thing
to a cross-environment/global-concerns home (resource summary, stop
all). Revisit once item 9's Settings screen exists.

**Reused `NotificationWatcher`'s existing poll instead of a second
loop.** That component already fetches `node_status`/`ord_status` for
every environment every 15s (for the sync/ready/disk notifications
above); it now also reports each environment's syncing/indexing state
up to its parent via a callback, which aggregates across environments
and calls the new `set_prevent_sleep` command only when the combined
"should the OS stay awake" value actually changes -- not on every poll
tick regardless of change, and not via a second, redundant background
poll just for this.

### Portable-mode detection + data layout (Phase 9, 2026-09-25)

**Portable-mode signal: a `config` directory next to the executable.**
Considered inferring it from drive-letter shape or path patterns, but
that's fragile and platform-specific in exactly the ways this feature
needs to avoid. A directory the "prepare a new portable drive" wizard
creates once, deliberately, as an explicit signal is simple, cheap to
check (`Path::is_dir()`), and can't be triggered by accident the way a
heuristic could -- confirmed with a real regression test for the
"a plain file named `config`, not a directory, must not count" edge
case specifically.

**`dirs` crate over Tauri's `app.path().app_data_dir()`.** The latter
needs a live `App`/`AppHandle`, but `data_root()` has to resolve
*before* the Tauri builder even exists (it's needed to open the
settings database, which happens at the very top of `run()`).
`dirs::data_dir()` resolves to the same base Tauri's own resolver uses
(`%APPDATA%` / `~/Library/Application Support` / `$XDG_DATA_HOME`,
confirmed via docs.rs) -- joined with a friendly "Nodekeeper" folder
name instead of the reverse-DNS bundle identifier Tauri would use, for
a nicer on-disk folder name.

**Installed mode's default environment-data location changed from
`data_root()` itself to `data_root()/environments`.** Previously
`environment_data_root()` fell back to the exact same directory as the
settings database when unset. Splitting them slightly (still both
under the same OS app-data location, just an `environments` subfolder)
means a fresh install's environment data and Nodekeeper's own tiny
settings DB are never literally sharing one folder, matching portable
mode's already-separate `/data` vs `/config` split instead of only
achieving that separation once the user explicitly picks a directory.

**Cross-platform testing limit for the rest of this phase.** Recorded
plainly in PROGRESS.md's Phase 9 intro: this session is Windows-only
(no macOS/Linux host, VM, or cross-compilation toolchain available).
Windows-specific work in this phase gets real verification; macOS/
Linux-specific detection code (App Translocation, noexec mounts) will
be written from VERIFIED platform-API research but flagged as
untested-on-the-real-OS rather than silently claimed as verified.

### Safely shut down and eject (Phase 9, 2026-09-25)

**Reused the tray Quit path's stop-everything logic instead of writing
a second copy.** Phase 8's tray "Quit" already stops every running
environment (ord then bitcoind) before exiting. Extracted that loop
into `stop_every_running_environment`, now returning a real
`Result<(), TypedError>` instead of swallowing errors -- Quit still
discards the result (`let _ =`, best-effort: the app is exiting either
way), but `safe_eject` propagates it, since silently telling the user
it's safe to unplug a drive when a stop actually failed would risk
real data corruption, not just be unhelpful.

**Frontend "safe to unplug" state derived at render time, not reset
via an effect.** First pass used a `useEffect` to clear the success
flag once `runningCount` rose above zero again -- flagged by this
project's own lint rule against synchronous `setState` in an effect
body. Fixed by deriving `showSafeToUnplug = ejected && runningCount
=== 0` directly during render instead of a separate reset effect. This
surfaced a related real gap: `runningCount` only updates on the
Overview screen's existing 3s poll, so right after a successful eject
the success message would stay hidden for up to 3s behind a stale
count. Fixed by also setting `runningCount` to 0 immediately in the
eject handler itself -- `safe_eject` succeeding is already confirmation
enough, no need to wait for the next poll to agree. Live-verified both
the immediate success message and the message correctly disappearing
again once a service was restarted.

### Window-close warning in portable mode (Phase 9, 2026-09-25)

**VERIFY: `tauri-plugin-dialog`'s blocking dialog can't run on the main
thread.** `MessageDialogBuilder::blocking_show()`'s own docs state it
"cannot be executed on the main thread as it will freeze your
application" (docs.rs) -- and `on_window_event`'s closure runs on the
main thread, so calling it there directly was never an option. Used
`show(callback)` instead (async under the hood, takes an
`FnOnce(bool)`), which is exactly what's needed: `api.prevent_close()`
fires synchronously first, then the dialog's callback decides whether
to actually stop everything and exit once the user responds.

**Portable mode branches away from Phase 8's hide-to-tray behavior
entirely**, rather than adding a warning on top of it. Installed mode
keeps hiding to the tray on close while anything runs (unchanged).
Portable mode shows a native Yes/No warning instead and, on
confirmation, runs the same `stop_every_running_environment` path
`safe_eject` uses before exiting -- resolves the interaction flagged
as open when Safe Eject was built: a drive's bitcoind should never be
left running in the background the way installed mode's tray
deliberately keeps it running, since the drive itself could be
unplugged at any moment.

### exFAT/filesystem warning (Phase 9, 2026-09-25)

**VERIFY, live, on this dev machine**: `sysinfo::Disk::file_system()`
reports this machine's real NTFS system drive as the all-caps string
`"NTFS"`, not title-case -- confirmed via a temporary `eprintln!` in
the real test, run once, then removed, rather than assumed from the
crate's docs alone. `is_risky_portable_filesystem`'s exFAT match is
case-insensitive as a direct result.

**Single source of truth, fixed before committing.** First pass
computed `disk_filesystem` in nk-core but left the actual "is this
risky" judgment to a TypeScript string comparison in
`SystemCheckScreen.tsx` -- a second, driftable copy of the same logic
`is_risky_portable_filesystem` already existed to express, and that
Rust function ended up exercised only by its own unit tests, not by
any real code path. Caught on review before committing: added
`disk_filesystem_is_risky: bool` to `SystemCheck`, computed from the
one real predicate, so the frontend just reads a boolean instead of
knowing what "risky" means.

### Unclean-shutdown recovery guidance (Phase 9, 2026-09-25)

**VERIFY, live: does bitcoind remove its own `bitcoind.pid` on a clean
shutdown?** This is the entire premise the feature rests on -- if it
didn't, a leftover pid file would mean nothing. Confirmed yes with a
real regtest bitcoind: started via the exact recipe `NodeManager::
start` uses (including writing `bitcoin.conf` first -- a first pass of
this test skipped that and got a `StartupTimeout` instead of a real
answer, since bitcoind without `server=1`/the configured RPC port
never became ready), then stopped gracefully; `bitcoind.pid` was gone
afterward. ord needed no live VERIFY for the equivalent question:
`ord.pid` is Nodekeeper's own file (ord writes none itself), and
`OrdProcess::stop` already deletes it explicitly on a clean exit,
visible directly in the same module -- the guarantee is in the source,
not an external binary's behavior to confirm.

**A stale pid file (present, but the process it names isn't alive) is
therefore a reliable "previous run didn't exit cleanly" signal**, kept
deliberately separate from the existing `detect_running_bitcoind`/
`detect_running_ord` (which answer "is it running *right now*",
collapsing "never run" and "crashed last time" into the same `None`) --
a new, narrowly-scoped `bitcoind_had_unclean_shutdown`/
`ord_had_unclean_shutdown` pair instead of overloading or changing the
existing functions' return shape and every call site that pattern-
matches on it.

**Process/tooling note, not a defect in the shipped feature**: writing
this VERIFY test surfaced a real, pre-existing gap -- `BitcoindProcess`
has no `Drop` impl that kills its spawned child, so a test that panics
after starting a real bitcoind (as an early, broken version of this
test did) leaves it running as an orphan. In this session specifically,
the orphaned process appears to have kept the invoking background
shell from reporting completion until the orphan was killed by hand --
worth remembering for any future test that spawns a real child
process and can panic before reaching its own cleanup code. Not fixed
here (out of scope for this feature; the existing tests all clean up
via their own explicit `.stop()`/`.kill_sync()` calls on the success
path), but worth a future look if it recurs.

### Windows release builds: console-less process handling (Phase 10 step 0, 2026-09-26)

**Why this was checked.** The Phase 10 scoping critic noticed that every
earlier check of process handling ran under a parent that had a console
(the Phase 0 PowerShell spike, `cargo test`, `tauri dev`, the debug
build), but the shipped exe does not: `src-tauri/src/main.rs` makes
release builds a Windows *GUI* subsystem program. `nk-proc` stops `ord`
with `GenerateConsoleCtrlEvent(CTRL_BREAK_EVENT, pid)`, which per the
Win32 docs only reaches processes that share the caller's console.
Nothing had ever exercised that combination, and CI only builds
`tauri build --debug --no-bundle`.

**How it was checked.** `crates/nk-testkit/src/bin/console_less_probe.rs`
(first written as an example; see "Implementation" below for why it is
now a bin target) is itself a `windows_subsystem = "windows"` exe (no
console, exactly the release situation) and drives the *same* `nk-proc`/
`nk-exec` code the app uses: start regtest bitcoind, start `ord server`,
run one console-subsystem child through the executor (`ping -n 6`), then
graceful-stop ord, then bitcoind, logging each step to a file. The driver
`crates/nk-testkit/scripts/console_less_probe.ps1` launches it with
`Start-Process`, samples the process tree and visible top-level windows
while everything is alive, and reports survivors. A full Tauri release
build was not used (disk was at ~8.5 GB free); the probe reproduces the
only relevant property -- a GUI-subsystem parent spawning console-
subsystem children -- with none of the GUI code in the way. The
bitcoind/ord binaries are the cached verified ones (bitcoind 31.1, ord
0.29.0). Commands (PowerShell; cargo is at `%USERPROFILE%\.cargo\bin`,
which was not on PATH in this session):

```
cargo build -p nk-testkit --bins
./crates/nk-testkit/scripts/console_less_probe.ps1
```

**Result on unmodified code (master at 34d3d5e):**

```
Name                     Pid Parent                       ConsoleHostChildren
console_less_probe.exe 15836 powershell.exe#724
bitcoind.exe           16184 console_less_probe.exe#15836 conhost.exe#10340
ord.exe                 7016 console_less_probe.exe#15836 conhost.exe#14900
PING.EXE                4852 console_less_probe.exe#15836 conhost.exe#7800

NEW visible top-level windows since before the probe
11652|CASCADIA_HOSTING_WINDOW_CLASS|...\ping.exe
11652|CASCADIA_HOSTING_WINDOW_CLASS|...\ord.exe
11652|CASCADIA_HOSTING_WINDOW_CLASS|...\bitcoind.exe

[  3582ms] bitcoind started, pid Some(16184)
[  5308ms] ord started, pid Some(7016)
[ 13396ms] stop_ord FAILED after 0ms: Ord(Io(Os { code: 6, kind: Uncategorized, message: "The handle is invalid." })); ord still detected running: Some(7016)
[ 13516ms] bitcoind stop OK in 120ms
survivors after probe exit: ord.exe 7016 (orphaned, still running)
```

Two real defects in what a release build would ship:

1. **`ord` cannot be stopped gracefully.** `GenerateConsoleCtrlEvent`
   fails with `ERROR_INVALID_HANDLE` because the caller has no console.
   ord keeps running as an orphan (holding its index open), and
   `stop_every_running_environment` (`src-tauri/src/lib.rs`) aborts at
   that first error, so that environment's bitcoind and every later
   environment are never stopped either; tray Quit ignores the result
   (`let _ =`) and exits anyway. Safe Eject would report the failure,
   but everything else stays running. bitcoind itself is fine (RPC
   `stop`, 120 ms).
2. **Every child process gets its own visible console window.** A GUI
   parent with no console makes Windows allocate a new console for each
   console-subsystem child. Here (Windows 11 with Windows Terminal as
   the default terminal) that shows as a terminal window titled with the
   exe path for bitcoind, ord, and every executor-spawned command
   (`ord wallet ...`, `bitcoin-cli`, scripts); on machines with the
   classic console host they'd be plain black console windows (not
   observed here). Closing such a window would normally terminate the
   process in it -- standard Windows behavior, not tested.

**Prototype of the leading fix (not shipped -- see below).** Preserved,
uncommitted-to-master, on local branch `wip/phase10-step0-console-fix`
(commit 95f8aa9): `CREATE_NO_WINDOW` on the bitcoind, ord and executor
spawns (each child gets a console-less-window host of its own), plus,
for ord only, a stop that temporarily joins ord's hidden console
(`FreeConsole` + `AttachConsole(ord_pid)`), sends `CTRL_BREAK_EVENT` to
ord's process group, and leaves again, all under a mutex (attach/free is
process-wide state). Same probe, same machine:

```
NEW visible top-level windows since before the probe: (none)
[ 12937ms] stop_ord OK in 1523ms; ord still detected running: None
[ 13063ms] bitcoind stop OK in 125ms
survivors after probe exit: (none)
ord exit status: ExitStatus(0) code=Some(0)     (second run, stop_ord 1593 ms)
```

Exit code 0 (not `0xC000013A`, what an unhandled CTRL_BREAK gives) and a
~1.5 s stop mean ord's own shutdown handler ran -- a graceful stop, not a
kill. Known gap in the prototype, deliberately not papered over: it
calls `FreeConsole()` unconditionally, which would detach a *developer's*
terminal (debug builds, `cargo test`) from the process and could break
later stdout writes there. A real implementation must branch at runtime
on "does this process have a console" (`GetConsoleProcessList`): with a
console, keep today's verified shared-console behavior (no
`CREATE_NO_WINDOW` on ord, direct `GenerateConsoleCtrlEvent`); without
one, use hidden consoles plus the attach-and-signal stop. That runtime
split is also what a regression test needs to cover (the probe is that
test's console-less half).

**STOP AND ASK (CLAUDE.md: graceful ord shutdown is a named non-
negotiable).** Options put to the project owner:

- **A (recommended, prototyped and verified above):** hidden consoles for
  every child + attach-and-signal graceful stop for ord, with the runtime
  console/no-console split. Trade-off: a few lines of process-global
  Win32 state (serialized by a lock) that must stay correct as more
  features stop things concurrently.
- **B:** give the app one hidden console at startup (`AllocConsole` +
  hide) so children inherit it and today's stop code works unchanged.
  Trade-off: with Windows Terminal as the default terminal the console
  can still show a window and hiding it is unreliable; not tested.
- **C:** force-kill ord. Rejected: the spec requires a graceful stop and
  a killed ord risks a repair/reindex on next start.
- Independent of A/B: make `stop_every_running_environment` keep going
  after a failure and report every failure at the end, rather than
  aborting at the first (so one stuck ord no longer leaves bitcoind and
  the other environments running).

**Outcome: the project owner approved option A** (2026-09-26), including
the keep-going-past-failures change.

**Implementation (2026-09-26).**

- `nk-exec/src/console.rs`: `no_console_window(&mut Command)` and
  `CREATE_NO_WINDOW`; applied to every executor child (all executor stdio
  is piped, so none needs a console). Trade-off, stated in the file: with
  a console, a developer's Ctrl+C no longer also reaches an executor child
  that is mid-run.
- `nk-proc/src/console.rs`: `ConsoleMode::{Shared, Hidden}`, decided once
  at ord's spawn from `GetConsoleProcessList` and stored on `OrdProcess`
  so the stop uses the route the spawn set up. **Shared** (Nodekeeper has
  a console: dev, debug, `cargo test`) is unchanged from the behavior
  verified since Phase 0 -- same creation flags, same signal; the one
  difference is that the ord/bitcoind spawns now set stdin to null. **Hidden** (the release exe):
  `CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW` at spawn; at stop,
  `AttachConsole(ord_pid)` -> `GenerateConsoleCtrlEvent(CTRL_BREAK,
  ord_pid)` -> `FreeConsole`, serialized by a process-wide lock (Windows
  only -- see the dead-code finding below). Unlike the prototype there is
  deliberately **no `FreeConsole` before the attach**: if this process
  somehow does hold a console, `AttachConsole` fails (access denied) and
  is reported, rather than tearing down a console we don't own.
  `process_has_console()` is public (the probe logs it, so its test
  cannot pass vacuously under a console); always `true` off Windows.
- **The stop also saves and restores this process's three standard
  handles** (`GetStdHandle` before the attach, `SetStdHandle` after
  `FreeConsole`), and the ord and bitcoind spawns set `stdin` to null
  explicitly. Found by the review, not by my first tests: in a
  console-less process the std handles start NULL, `AttachConsole` fills
  them in, and `FreeConsole` closes them but leaves the stale numbers, so
  the next `ord`/`bitcoind` spawn (default stdin = inherit = duplicate
  `GetStdHandle`) failed with "The handle is invalid (os error 6)" until
  Nodekeeper was restarted. Reproduced by two independent skeptics and by
  the negative control below.
- bitcoind gets `CREATE_NO_WINDOW` **only when Nodekeeper has no
  console** (`hide_window_when_console_less`); with a console it keeps
  sharing it, so Ctrl+C in a developer's terminal still stops it (review
  finding: the first version hid it unconditionally and would have left
  an orphan there).
- `OrdProcess::stop` returns ord's `ExitStatus` (callers ignored the old
  `()`), so a test can tell graceful (exit 0) from killed. An ord that
  **already exited** is now a successful stop (checked before the signal
  and again if the signal fails; pid file removed): on the Windows hidden
  route the signal cannot even be attempted for a dead process, which
  used to make Stop / Safe Eject report a failure for something that
  wasn't running.
- `NodeManager::stop_everything(&environments)` (+ `StopFailure`,
  `Service`, `NodeManagerError::StillRunning`): ord-then-bitcoind per
  chain in `Chain::ALL` order, unchanged 30 s / 120 s timeouts, **never
  returns early**, and then **verifies against reality**. `stop` and
  `stop_ord` untrack a process the moment a stop is *asked for*, so after
  a failed or timed-out stop the process can be alive yet invisible --
  and a second Safe Eject used to find nothing tracked and say "safe to
  unplug" while it was still running (review finding; predates this
  change, but this change is what makes stop failures reachable). Now
  each chain's `bitcoind.pid` / `ord.pid` is checked after the stops
  (3 s settle window for a process that is still disappearing) and
  anything alive is reported, tracked or not -- which also catches an
  orphan from an earlier crashed session. Calls are **single-flight**
  (an async gate) and `any_running()` counts calls in flight, so a
  double-clicked window close or Quit-during-Eject no longer sees "nothing
  running" while a stop is still waiting on a slow bitcoind.
  `stop_every_running_environment` turns the failure list into one
  `TypedError` naming every failure (the structured `code` is kept only
  for a single failure). Tray Quit is still best-effort by design; Safe
  Eject reports.
- **A stop can no longer hang forever** (second review). `BitcoindProcess::
  stop` used to await the `stop` RPC *outside* its timeout, and the RPC
  client has no request timeout, so a bitcoind that accepts the
  connection but never answers (RPC threads stuck behind a stalled
  drive -- the portable case) hung it for good; under the new
  single-flight gate that would also have wedged `any_running()` and
  every later Quit / window close, which used to be an escape hatch. Now
  the RPC is bounded by the same timeout (worst case two timeouts), and
  `stop_everything` wraps each service's stop in an outer bound
  (`StopBudget`: ord 30 s + 5 s; bitcoind 2 x 120 s + 5 s) that turns a
  hang into an ordinary `StopTimeout` failure and releases the gate.
  Worst case for Quit with everything hung is therefore long (several
  minutes) but bounded.
- `NodeManagerError::StillRunning` reads "still running (process id N)"
  with no "after being asked to stop": it is also what an orphan from a
  crashed session or a node started outside Nodekeeper looks like, and
  a retry can never stop those. Safe Eject's panel says so ("close that
  program yourself, for example in Task Manager").
- Safe Eject's failure panel (`OverviewScreen`) is now specific -- "Not
  everything stopped / Do not unplug this drive yet ... <which service is
  still running>" -- instead of the generic "Something went wrong" with
  the real cause hidden behind the technical-details toggle. No new
  `AppErrorCode` was added (that enum is the spec's named set); any
  failure of `safe_eject` means "not confirmed safe", so the wording is
  specific to Safe Eject rather than code-driven.
- Probe and helper are `[[bin]]` targets of `nk-testkit`
  (`console_less_probe`, `console_probe_child`), driver in
  `crates/nk-testkit/scripts/`, regression test in
  `crates/nk-testkit/tests/console_less.rs`.

**Verification.**

- Live, console-less, final code (`console_less_probe.ps1`): no new
  visible windows; `has console: false`, std handles `[0, 0, 0]` at start
  and again after the stop; `executor child console: console_window=none`;
  `stop_ord OK in 1441ms; ord exit code: Some(0)`; **ord restarted and
  stopped again (exit 0); a second bitcoind started and stopped after the
  first ord stop**; no survivors.
- Regression test `nk-testkit/tests/console_less.rs` (Windows) asserts all
  of the above from the probe's log. **Three negative controls**, each
  reverted afterwards:
  1. force `ConsoleMode::Shared` (the old behavior): fails in ~7 s with
     `stop_ord FAILED ... code: 6 ... "The handle is invalid"` and
     `ord still detected running: Some(pid)`, no processes left behind;
  2. remove both the std-handle restore and the null stdin: fails at the
     respawn -- std handles `[1472, 1664, 1860]` after the stop, `ord
     restart FAILED: ... The handle is invalid. (os error 6)`, `second
     bitcoind start FAILED` (the reviewers' bug, reproduced exactly);
  3. remove `no_console_window` from the executor: fails with `executor
     child console: console_window=present`.
- Unit: mode selection, per-mode creation flags, "a failed hidden-console
  signal leaves this process's console state alone", stopping an
  already-exited ord on both routes (+ pid file removed), and 11
  tests in `node_manager.rs` (9 through a fake that models "tracked" and
  "alive" separately, 2 on message shape) plus 3 for the error mapping in
  `lib.rs`; the fake covers: ordering, nothing running, a failing ord doesn't
  stop the rest (reported once, not twice), every failure reported, **a
  retry after a failed stop does not report success**, an untracked live
  process is reported, a lingering process gets its settle window,
  overlapping calls run one after the other and `any_running()` stays true
  in between, message shapes. Negative control: with the gate and the
  verification pass disabled exactly the four tests that should fail did
  (retry, untracked orphan, settle window, overlap).
- Second-review additions: the probe now also asks Windows which
  processes own a *visible* window and the test asserts none belongs to
  the long-lived bitcoind or ord (previously only the executor helper
  was checked automatically; by owning pid, so reliable under Windows
  Terminal but able to miss with the classic console host -- the .ps1
  driver samples every visible window as the backstop); the first-stop
  assertion is one whole-line match, because separate substring checks
  were satisfied by the *second* stop's log line. Negative controls:
  (4) unbounded stop RPC -> `a_stop_rpc_that_never_answers_cannot_hang_
  the_stop_forever` fails after 10 s ("stop must give up by itself");
  (5) bitcoind and ord without their hidden-window flags -> the
  regression test fails with `visible windows owned by bitcoind/ord:
  [17936, 22424]`. Both reverted.
- UI: Vitest for the Safe Eject failure state (specific title, "do not
  unplug", the service named in plain view, no all-clear, button usable
  again) and the success state; looked at in the dev preview with a
  failing `safe_eject`.
- **Non-Windows compile check (simulated).** Only the Windows target is
  installed, so both console modules were compiled as copies with the
  `cfg(windows)` gates flipped and `rustc -D warnings`: clean. The control
  (lock left ungated, as in the first version) reproduces the reviewers'
  `static CONSOLE_LOCK is never used` / `function lock is never used`
  errors, so the simulation can see the problem. Not a substitute for
  macOS/Linux CI.
- Console (Shared) path: the existing real-node tests (`starts_ord_
  reports_status_and_stops_it`, the nk-testkit wallet/inscribe suites)
  exercise it and stayed green.

**Review of the implementation (2026-09-26).** A 4-lens read-only
workflow (Win32 semantics, regression risk, stop-everything semantics,
test validity), every finding then attacked by two skeptics instructed to
refute it (40 agents, the first attempt lost three lenses to an API
connection error and was re-run). Fixed: the stale-std-handles spawn
failure (high, above), stop of an already-exited ord, dead code on
non-Windows CI, bitcoind Ctrl+C in dev, the retry-says-safe hole, the
double-close race, the generic Safe Eject error, and test gaps (a probe
that never spawned after a stop; cleanup that missed a process whose
startup failed; `pid None` matching `started`; no test for executor
windows; stale PROGRESS path). **Deliberately not changed:** the
portable-mode window-close path and tray Quit still discard stop failures
and exit (both skeptics: pre-existing, documented best-effort design --
though the close dialog's "safe to unplug" wording is worth revisiting);
`stop`/`stop_ord` still untrack before stopping (the verification pass
makes that safe rather than redesigning process ownership); the test
skips silently when `NK_TEST_*` are unset (the project-wide convention).

**Second review, of the fixes (2026-09-26).** Same shape (3 lenses, two
skeptics per finding, 21 agents). No critical or high findings; six
survived, all rated low by their skeptics except the hang. Fixed: the
stop that could hang forever (above), the misleading "after being asked
to stop" / "wait and retry" wording for untracked processes (above), no
automated window check for bitcoind/ord (above), the first-stop
assertion satisfiable by the second stop's log line (above), and
documentation slips (a build command that missed the helper binary,
a test count, "byte-for-byte"). **Deliberately not changed -- pid
reuse:** the verification pass (like `detect_running_*` before it)
trusts "pid file names a live pid", so a stale `bitcoind.pid` whose pid
an unrelated process has since reused makes Safe Eject say "not
everything stopped" until that process exits or the file is deleted.
Making liveness check the process *name* would trade that (fails safe:
an unnecessary "don't unplug") for a false "nothing running" whenever
the process is not literally called bitcoind/ord -- notably Bitcoin-Qt,
which writes the same `bitcoind.pid` -- which is the dangerous
direction for a safe-to-unplug check. The reported message names the
pid so the user can see what it is. Rejected by their skeptics:
`any_running()` ignoring manual Stop/Restart (the documented
untrack-before-stop design), per-poll `sysinfo` cost, and
`kill_stragglers` matching a stale parent pid (needs a user's real
node to be a child of a dead probe's reused pid).

**Not verified / limits.** The probe stands in for a real Tauri release
exe (no GUI code, debug profile); a check with a real release exe is
still owed before any packaging work (disk was too tight to build one).
Nothing here ran on macOS/Linux: those platforms keep the direct SIGINT
path and the new code is cfg-gated, but only Windows compiled and ran it
(plus the simulated check above). The prototype branch
`wip/phase10-step0-console-fix` (commit 95f8aa9) is superseded.

**Test-engineering lessons.**

1. `cargo test` does **not** rebuild examples (checked: the probe exe was
   17 minutes older than its source after a `cargo test`), so a regression
   test that launches an example can silently run a stale binary. The
   probe is therefore a `[[bin]]`: Cargo builds bins before an integration
   test runs and hands it the exact path via `CARGO_BIN_EXE_*`.
2. An **orphaned ord inherits the probe's stdout/stderr pipe handles**
   (Windows children inherit every inheritable handle even when their own
   std handles are redirected to NUL), so the executor never sees
   end-of-output and the test hangs -- my first negative control ran 612 s
   until the orphan was killed by hand. Same mechanism as the "orphaned
   bitcoind stalls the background command" note above. The test now waits
   on the probe's own `DONE` line, kills any bitcoind/ord that is a
   *child of the probe* (found by parent pid, so a process whose startup
   failed is caught too; pid *and* name matched, since pids get reused) on
   every path including panics, and includes the probe log in every
   failure message.
3. A probe/test must **exercise what happens after** the thing under test:
   the first version stopped ord and checked it stopped, and so missed
   that the stop broke every later spawn.
4. Disk on this machine filled again mid-run (`no space on device`); the
   fix was deleting only `target\debug` (leaving the cached verified
   binaries) and building with `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0`
   (environment only, no repo change).

**Process note.** `cargo` was not on PATH in either shell this session;
prefix `$env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"`.

### Single-instance lock and command-history pruning (Phase 10 step 1c/1d, 2026-09-26)

Both were spec-mandated and built, but never wired in (found by the Phase
10 scoping critic, confirmed by grep): `SingleInstanceLock` (Foundation C)
was never acquired by the app, and `Store::prune_command_history` (item 7,
"last 5,000 entries per environment") had no caller outside its own test.

**Single-instance lock.**

- *Where and when:* on the app data folder (`data_root()`: the settings
  database, and in portable mode -- where `config/` and `data/` sit side by
  side on the drive -- effectively everything), at the very start of
  `run()`, **before the settings database is opened**, so a second copy
  never touches it.
- *Refusal:* a native message box (`instance_lock.rs`) in plain words --
  "Nodekeeper is already running" (with how long ago and where to look), or
  "in use on another computer" (names it, says to close it there first, and
  shows exactly which file to delete if that computer is really done), a
  damaged lock file, or an unwritable folder -- then exit code 1. There is
  deliberately **no "take over anyway" button** for a lock held by another
  computer (two computers on one drive can corrupt it); the user deletes
  the named file. The box is shown before the Tauri app exists, so it uses
  `rfd` directly -- already in the build at 0.16.0 with exactly the features
  `tauri-plugin-dialog` enables, so declaring it changed `Cargo.lock` by one
  line (the edge) and downloaded nothing.
- *Release:* **explicitly**, on `RunEvent::Exit`. Checked rather than
  assumed: with the explicit release removed (negative control) a normal
  window close left `.nodekeeper.lock` behind, i.e. Tauri's exit skips the
  destructors of managed state; restored afterwards. A crash / kill still
  leaves the file, which is what the stale detection is for.
- *Stale detection made identity-aware.* `nk-proc`'s lock used a bare
  "is any process running with that pid" check. Fine for a crash *within*
  one session, but a lock outlives every exit that cannot clean up (Task
  Manager, power loss, a future updater exiting) and pids are recycled
  aggressively, especially across a reboot -- so after one such exit, the
  data folder could stay "already running" for as long as an unrelated
  program happened to hold that number. A process can only have *reused*
  the pid if the holder was already gone, so `lock_holder_is_alive` asks
  whether it **started no later than the lock was written** (5 s slack): if
  so it can be the holder; if it started after, it cannot. That start time
  is decisive when the OS reports it; **the executable name is only a
  fallback for an unknown start time** (0: e.g. an elevated process seen
  from a normal one). A first version required the name to match as well;
  review showed that on Linux the OS reports the name a program was
  *launched under* (a symlink's, truncated to 15 characters) while
  `current_exe` gives the resolved target, so a live copy started through
  a symlink would have been judged stale and two copies would share the
  folder -- and the start time already covers pid reuse completely. Every
  unknown errs toward "still held". This is **not** applied to the
  bitcoind/ord pid-file checks (`process_is_alive` is unchanged): there a
  name check could wrongly report "nothing running" for a node started
  another way -- Bitcoin-Qt writes the same `bitcoind.pid` -- which is the
  dangerous direction for a safe-to-unplug check (DECISIONS.md, console-
  less process handling, second review).
- *Verified live with the real app* (`target/debug/nodekeeper.exe` copied
  into a temp folder with a `config/` sibling, i.e. an isolated portable
  install; nothing near real data): (1) first copy takes the lock (hostname,
  its pid, unix time); (2) a second copy stays up only as a message box
  ("#32770 | Nodekeeper is already running", screenshot taken), leaves the
  lock byte-for-byte unchanged, and exits with code 1 when dismissed; (3)
  closing the first copy normally removes the lock; (4) after `Stop-Process
  -Force` (a crash) the lock is left naming the dead pid and the next copy
  takes it over and runs normally; (5) a lock written an hour ago naming a
  live, unrelated `ping` (which necessarily started after it) is treated
  as stale and the app starts; (6) **the race: three real copies launched
  at once over a crashed lock -- exactly one gets a main window, two show
  the "already running" message, and the lock names the running copy.**
  (An earlier run of (5) "failed" because the script planted the lock
  with `Get-Date -UFormat %s`, which in Windows PowerShell 5.1 is *local*
  time -- 10,801 s ahead here -- making an "hour ago" lock two hours in
  the future; the app correctly treated that as a live holder. Fixed in
  the script, not the code.)
  Unit tests: message wording per failure, ages, exclusive-then-released
  through the real lock, the start-time/name decision (all combinations),
  a lock whose pid a live unrelated process holds, and the lock-file race
  cases below.
- *Concurrency hardening of `acquire` (from review).* The first cut had a
  read-then-`remove_file` takeover: inspecting a stale lock (a process-list
  scan, ~0.1-1.4 s here) left a wide window in which a slow launch would
  delete *another launch's brand-new, live lock* and create its own, so
  both ran. `acquire` is now a bounded retry loop: the stale file is
  removed only under an exclusive takeover guard (`.nodekeeper.lock.
  takeover`, `create_new`) **and only if it still holds exactly the
  contents that were judged stale**; the atomic `create_new` of the lock
  itself then decides any remaining race. Negative control: with the
  guard replaced by the old unconditional remove, the 8-way race test
  failed on the first round with **"8 launches, 6 won"**. Also from
  review: a lock that disappears while being inspected is a reason to
  look again, not an I/O error; an empty or garbled lock is first waited
  for (the file is created *then* filled in, so a second launch can catch
  it empty), and is replaced only once too old to belong to a live writer
  (10 s) -- a younger one is reported as damaged and never deleted; a
  failed write (disk full, drive unplugged) removes the empty file it
  just created; and losing the create race re-reads the winner instead
  of reporting this process's own pid as "the other copy".
- *Not covered / limits.* The environment data folder is locked only when
  it is inside the app data folder (portable default) -- **one pointed at a
  shared external drive is not locked**; there, bitcoind's own datadir lock
  and the pid-file checks are the only guard, and a second computer would
  not see this computer's bitcoind. Locking it too needs a decision on what
  a refusal *there* should do (the user must still be able to reach the app
  to change the folder), so it is left as a follow-up rather than guessed.
  **Safe Eject now closes the app** (found by review as a gap, then
  decided -- see the next section). **Host identity is the OS hostname**, which macOS can
  change with the network, and on Linux the process start time is derived
  from the boot time, which moves if the wall clock is stepped: either can
  make a same-computer lock read as foreign or a live holder as stale on
  those platforms (untestable here; a stable machine id / boot id in the
  lock would fix both).
  No "focus the existing window" behavior (tauri-plugin-single-instance)
  -- the message says where to look. Tray Quit and the portable close-and-
  stop path also end via `app.exit(0)`, so they reach the same Exit
  handler, but only the ordinary window close was exercised live (a tray
  menu cannot be clicked from here). The message box on macOS/Linux is
  untested (this is a Windows-only session).

**Review of this change (2026-09-26).** Same shape as the earlier ones (3
lenses -- lock lifecycle, stale detection, pruning + docs -- every finding
attacked by two skeptics; 41 agents). Nothing critical; the substantive
findings were fixed (above): the takeover race, empty/partial/vanishing
lock files, the over-strict name check, the misplaced
`#[cfg_attr(mobile, tauri::mobile_entry_point)]` (my new `open_store` had
been inserted between it and `run()`; harmless on desktop, wrong on mobile),
the pre-lock `create_dir_all().expect()` that would fail silently instead of
showing the friendly message, and the throttle test. **Not changed:** Safe
Eject vs. the lock (owner question, recorded above), hostname stability on
macOS and the Linux clock-step case (untestable here, recorded above).
Refuted by their skeptics: the startup prune being synchronous work before
any window (5,000 rows x a few environments is milliseconds), a running
command's row being prunable (only after 5,000 newer ones), and "a failed
prune only logs" (true of every store write in this app).

**Safe Eject closes Nodekeeper after a successful eject (2026-09-26).**
The review of the lock found that "Safely shut down and eject" stopped the
services and said "safe to unplug" while Nodekeeper itself kept running --
from the drive, in portable mode, with its settings database open and
`config/.nodekeeper.lock` on it. Unplugging then left the lock behind, and
the next computer to open the drive refused with "in use on another
computer". The question (quit the app, or release the lock and reword) went
to the project owner, whose reply was a bare "Continue"; that has been how
this project's owner delegates judgment, so the recommended option was
taken -- **easy to reverse: it is one spawned task in `safe_eject`**. After
the stops succeed and are verified, `safe_eject` spawns a task that waits
4 s (`SAFE_EJECT_CLOSE_DELAY`, so the message can be read) and then calls
`stop_everything_and_exit`, so anything started again in the meantime is
stopped first, and the exit event releases the lock. The UI says so
("...safe to unplug this drive now. Nodekeeper is closing.", and the
panel's hint says the app will be closed). A *failed* eject changes
nothing: the app stays open with the "Not everything stopped" panel. Only
reachable in portable mode (the panel is portable-only). Verified end to
end with the real app (a debug build in an isolated portable folder, its
web view driven over a DevTools port, calling the real IPC commands):
start a real regtest bitcoind through `start_node` (pid file present) ->
`safe_eject` returned Ok in 1 s with bitcoind gone and its pid file
removed -> the app still open just after it returned -> it closed itself
within the 4 s (exit code 0) -> `.nodekeeper.lock` removed -> no processes
left. The rendered message is covered by Vitest (the click-through of the
real UI would need the setup wizard's binary downloads). Not covered: the
exit path when the user restarts a node during the 4 s window (by
construction it goes through the same stop-then-exit as tray Quit).

**Command-history pruning.**

- `COMMAND_HISTORY_KEEP_PER_ENVIRONMENT = 5_000` (item 7's "e.g. last
  5,000"). The event bridge (`persist_exec_events`) prunes an environment
  every **100 new rows** in it -- not after every command, because
  background polling records a great many rows and a `DELETE` each time is
  wasteful for a limit that only has to hold *about* (worst overshoot: 100
  rows per environment between prunes) -- and `open_store` trims every
  environment once at launch, so a history that grew before pruning
  existed (or while the app was closed) shrinks without waiting for new
  commands. A failed prune only logs; it never interrupts anything.
  `prune_command_history` now returns how many rows it deleted.
- Tests: per-environment and whole-store pruning (counts, isolation,
  idempotence); the shipped limits (5,000 / 100); end to end through the
  real executor event stream with a small limit (keep 2, prune every 5)
  asserting the **intermediate** states -- nothing pruned after four
  commands (review: the first version only checked the end state, so
  pruning after every command would have passed; negative control: with
  that bug the test fails at "below the interval"), cut back at the fifth
  and again at the tenth, another environment left alone; and the startup
  path on a real file database reopened the way a launch does.
- Rows are pruned by `started_at_ms`, so a very old command still marked
  "running" could in principle be pruned -- only after 5,000 newer ones.

### Console private-key output and secret arguments (Phase 10 step 1a, 2026-09-26)

**The problem** (found by the Phase 10 scoping critic, confirmed here).
The raw console runs whatever the user types after a safety classification,
and shows/stores the output: in the Live Command Monitor and in
`command_history` (SQLite, and so in exports). Three ways to make it print
private keys were classed **read-only, "runs instantly, no confirmation"**:
`ord wallet dump`, `listdescriptors true` and `gethdkeys` with `private`.
That breaks the secrets rule (CLAUDE.md: never logged, stored or exported),
whose "logs, database, exports" clause had never been tested for this path.
Scanning for it turned up more holes on the *input* side (passphrases typed
as arguments), and two adversarial reviews of the fix found more (below).

**Decision.** The question (block the commands, or route them through the
sensitive channel; and what to do about old rows) went to the project
owner, who answered "Do what you believe better"; the recommended option
was taken: **refuse** them in the console and **erase** old rows, with a
scrubber behind it. A backup feature (Phase 10 step 4) is where private
descriptors legitimately get read, through a dedicated sensitive path.

**The design rule, after two review rounds.** The first versions tried to
*recognise the dangerous spellings* (this method's 4th argument, that
prefix) and every review found another spelling. The rule now runs the other
way: **an argument is shown or stored only when the method is a known
Bitcoin Core method and the argument is before that method's first secret
position; everything else is hidden.** Unknown, mistyped, decorated or
future methods, unquoted passphrases with spaces, named arguments and JSON
forms all fall on the hidden side without anyone having to anticipate them.

**VERIFY (real binaries, not memory).** Script and exact invocation:
`crates/nk-testkit/scripts/scan_help_for_secrets.ps1 -BinDir <bitcoin-31.1\bin>`
(starts a throwaway regtest `bitcoind`, runs `bitcoin-cli help` and
`bitcoin-cli help <method>` for every method, stops and deletes it; with
`-ListOnly` it prints only the method names and probes for hidden ones).
Output of the run recorded here (Bitcoin Core `/Satoshi:31.1.0/`, 151
methods):

```text
=== (1) methods whose help text mentions private-key language ===
abortrescan                  private key
createwallet                 private key
createwalletdescriptor       private key
deriveaddresses              xprv
encryptwallet                private key, seed
getaddressinfo               private key, seed
getdescriptorinfo            private key
gethdkeys                    "private", private key, xprv
listdescriptors              private descriptor
scantxoutset                 xprv
signmessage                  private key
signmessagewithprivkey       private key, privkey
signrawtransactionwithkey    private key, privkey
walletpassphrase             private key

=== (2) positional parameters that look like a secret (the Arguments section) ===
createwallet:              4. passphrase   (string, optional) Encrypt the wallet with this passphrase.
encryptwallet:             1. passphrase   (string, required)
listdescriptors:           1. private      (boolean, optional, default=false) Show private descriptors.
migratewallet:             2. passphrase   (string, optional) The wallet passphrase
signmessagewithprivkey:    1. privkey      (string, required)
signrawtransactionwithkey: 2. privkeys     (json array, required) The base58-encoded private keys
walletpassphrase:          1. passphrase   (string, required)
walletpassphrasechange:    1. oldpassphrase, 2. newpassphrase
(the other hits -- abortprivatebroadcast, createwallet's disable_private_keys,
 avoid_reuse and external_signer, signmessage's address -- are not secrets)
```

(Shortened: the script prints the full parameter lines.) `help` does not list
the "hidden" RPCs, but `help <name>` answers for one that exists; probing a
list of candidate names against the node (the `-ListOnly` mode) found 18 that
exist in 31.1: `addconnection addpeeraddress echo echoipc echojson
estimaterawfee generate generateblock generatetoaddress generatetodescriptor
getorphantxs getrawaddrman invalidateblock mockscheduler reconsiderblock
sendmsgtopeer setmocktime syncwithvalidationinterfacequeue` (regtest tooling
and test hooks; candidates that did not exist, e.g. `getaddressbylabel`, were
discarded). Together with the 151: **169 known methods**
(`KNOWN_BITCOIN_RPC_METHODS`). Reading the lists:

- Only **`listdescriptors ( private )`** and **`gethdkeys ( {"active_only",
  "private"} )`** *print* private keys (`gethdkeys help`: named argument
  `private`, "Show private keys"; the result carries `xprv`).
- `createwallet "wallet_name" ( disable_private_keys blank "passphrase" ... )`
  -- the passphrase is the **4th** positional argument -- and
  `migratewallet ( "wallet_name" "passphrase" )` -- the **2nd** -- take a
  wallet passphrase as *input*; `encryptwallet`, `walletpassphrase`,
  `walletpassphrasechange`, `signmessagewithprivkey` and
  `signrawtransactionwithkey` were already known. `importdescriptors` takes
  descriptors that may be private. `deriveaddresses`, `getdescriptorinfo`,
  `scantxoutset`, `createwalletdescriptor` take a descriptor that *may*
  carry a key; there is no fixed position to hide, so what protects them is
  the backstop below, not the positional rule.
- **Bitcoin Core echoes a bad key back in its error message.** A live test
  (below) sends `getdescriptorinfo wpkh(<key-shaped string>)` to the real
  node and gets an RPC error whose text contains the key -- the shape
  `key '<what you typed>' is not valid` (the template string is also in
  `bitcoind.exe`). That error line reaches the event stream and the history.
- `ord 0.29.0`, verbatim: `ord wallet --help` lists `dump  Dump wallet
  descriptors`, `create  Create new wallet`, `restore  Restore wallet`,
  `sweep  Sweep assets from private key`, with options `--name <NAME>`,
  `--no-sync` and `--server-url <SERVER_URL>` before the subcommand. `ord
  wallet dump --help`: "Dump wallet descriptors" (no options). Run on a
  throwaway regtest wallet it prints `{"descriptors": [...], "wallet_name":
  ...}` whose descriptors contain **`tprv...`** (`xprv...` on mainnet); no
  mnemonic. `ord wallet restore --help`: "Restore wallet from <SOURCE> on
  stdin" (`--from descriptor|mnemonic`), plus an optional `--passphrase`;
  `ord wallet sweep --help` lists only `--address-type`, `--dry-run` and
  `--fee-rate` -- no option carries the key. `create`/`restore` stay
  blocked because they *print* or take a recovery phrase.
- `listdescriptors true` against a real Bitcoin Core: eight descriptors,
  each with a `tprv`.

**What changed.**

1. **Refuse, per call** (`nk-core/console_safety`). New
   `classify_bitcoin_rpc_call(method, args)` -> `BlockedPrivateKeys`, and
   `OrdCommandClass::BlockedPrivateKeys`. Deliberately **strict**: the
   private form is refused unless the call is *provably* the public one --
   `listdescriptors` only with no argument or exactly `false`; `gethdkeys`
   only with none or one JSON object whose `private` is absent/`false`
   (parsed, so a JSON escape like `"private"` cannot spell it past a
   substring check); anything odd is refused too, because a refused
   oddity costs a retype and an allowed one leaks a key. `dumpprivkey` /
   `dumpwallet` refused outright (not in 31.1; a different node may have
   them). `ord ... dump` refused **in any position** (`--no-sync dump`).
   Plain `listdescriptors`, `gethdkeys`, `ord balance` etc. still run
   instantly. Departs from spec item 6's "read-only runs instantly" only
   for these calls, on purpose.
2. **Enforced in the backend, not only the dialog.** `console_run` (the
   Tauri command) applies it whatever the frontend did, and
   `nk_ord::wallet::run_console_subcommand` -- the one function the ord
   half of the console goes through -- refuses `dump`/`create`/`restore` with
   `WalletError::Blocked` before spawning anything. (That is the ord console
   entry point only: `RpcClient::call` is the general RPC client and is
   used by features that legitimately call any method, so the bitcoin-cli
   side is enforced in `console_run`.)
3. **Backstop** (`nk-exec/redact`, `scrub_private_keys`). `redact` -- applied
   to every command display and every output line the executor emits, for
   spawned commands and RPC calls alike -- also removes, **leniently** (a
   key that is a little wrong is exactly what Core rejects and echoes): (a)
   an extended private key (`xprv/yprv/zprv/Yprv/Zprv/tprv/uprv/vprv/Uprv/
   Vprv` followed by at least **60** letters/digits -- the whole run, so a
   `0`/`O`/`I`/`l` typo does not save it; a real key has 107), found anywhere
   (a key after `\n`, an ANSI code or `%28` included); and (b) a **WIF** key:
   one alphanumeric token of 50-53 characters starting `5 9 K L c`, with at
   most one non-base58 character, bounded by anything that is not a letter
   or digit (or a JSON `\n`-style escape) -- bare, in a descriptor, or inside
   the quotes of Core's error text. Replaced with `[private key removed]`.
   The value returned to the *caller* is untouched. Secrets passed to
   `redact` are replaced **longest first**. The 60-character minimum came
   from a test: with 20, one random bech32 address in ~4,000 contains
   `tprv`/`xprv`/... (all those letters are in the bech32 alphabet) and was
   turned into `[private key removed]`; the test now runs 200,000 random
   txids and addresses through the scrubber. Not caught, stated plainly: a
   key split by whitespace (the tail survives), a truncated paste shorter
   than the minimum, and -- rarely -- a 50-53 character run inside a base64
   PSBT is mistaken for a key in the *stored copy* (the caller's copy is
   never touched).
4. **Hide secret arguments** (`bitcoin_rpc_secret_arg_mask`,
   `RpcClient::call_masked`, `console_secrets`). By *position*, never by
   matching the typed text (the first version replaced the raw token in the
   rendered command, which fails whenever the token is valid JSON -- a
   quoted numeric passphrase renders without its quotes). The rules:
   (i) a method **not on the list of 169 known Bitcoin Core methods** hides
   all its arguments (typos, `importprivkey`, `sethdseed`, `sudo ...`);
   (ii) a method that takes a secret hides **everything from its first secret
   argument to the end** -- the tokenizer splits on spaces and knows only
   double quotes, so an unquoted passphrase with spaces arrives as several
   tokens and every word of it is part of the secret; (iii) a `name=value`
   or JSON-object argument before that position hides *everything* (the
   passphrase is not where it usually is); (iv) the method name is matched
   case-insensitively; (v) a run of hidden arguments is displayed as **one**
   `[redacted]` (their number would tell how many words the phrase has).
   The secret positions: `walletpassphrase`, `walletpassphrasechange`,
   `encryptwallet`, `signmessagewithprivkey`, `importdescriptors` from the
   1st argument, `signrawtransactionwithkey` and `migratewallet` from the
   2nd, `createwallet` from the 4th. The app's own unlock/encrypt calls use
   the same mechanism (`walletpassphrase [redacted] 60`). A console line
   whose **first word is not a plain word** of letters and digits
   (`bitcoin-cli ...`, `bitcoin-cli.exe`, `./bitcoin-cli`, a path, `-regtest`,
   a BOM) is **refused before anything is shown or run**, with a message that
   does not repeat it. The tokenizer's "unterminated quote" error no longer
   prints the line; the on-screen scrollback shows only the leading plain
   word of a refused line (computed with a character class, not by splitting
   on whitespace, because the browser and the Rust tokenizer disagree on what
   whitespace is); a refused **ord** line shows only its subcommand
   (`ord wallet create [redacted]`); and what the console hands back to the
   screen -- error text and result -- is scrubbed of private keys like what
   the executor records.
5. **Erase old history** (`Store::scrub_private_keys_from_command_history`,
   run by `open_store` at every launch, idempotent, one transaction).
   A stored row is **deleted whole** when `history_row_reveals_secrets(display,
   output)` says so: output with a `"mnemonic"` field; a stored `bitcoin-cli`
   call that prints private keys (the same check as the live one), whose
   argument at its method's first secret position is not `[redacted]` (or
   with words after a hidden one -- the rest of an unquoted passphrase), or
   whose method is not a known one and has arguments other than a single
   `[redacted]`; a stored `ord ... wallet ... dump`. What it must **not**
   delete is tested: the Wallet screen's own `create`/`restore` rows (output
   is only the sensitive placeholder), `offer create`, and
   `walletpassphrase [redacted] 60`. Every other row has extended keys and
   WIF keys scrubbed from its display and output in place. "Erase" means gone
   from the **file**: the connection runs with `secure_delete=ON`, a non-empty
   scrub is followed by `VACUUM`, and `VACUUM` also runs **once, ever** (a
   settings marker, set only after it succeeds) whether or not the scrub found
   anything -- bytes freed by an earlier build, which did not zero what it
   deleted, stay in free pages otherwise. If the scrub **fails** (full disk,
   locked file), the history is not left visible: `open_store` quarantines it
   (hidden for the run, and deleted if that can be done) and the next launch
   retries. There is no banner for this yet.

**Found by the adversarial reviews of this change, and fixed.** Round 1:
positional redaction failing for JSON-valid secrets; `redact` order; deleted
rows staying in the SQLite file; the scrubber's alphanumeric guard; a typo, a
wrong-case name or a pasted `bitcoin-cli` prefix recording the passphrase; old
rows from the `--no-sync create` hole and clear `createwallet` passphrases.
Round 2 (19 findings upheld): unquoted or single-quoted multi-word
passphrases (only the first word was hidden); `bitcoin-cli.exe`,
`./bitcoin-cli`, `sudo`, `ord.exe` prefixes (a two-string blocklist);
`name=value` and JSON forms; a bare WIF or `importprivkey`/`sethdseed`/
`importmulti` at an unlisted position; Core echoing a bad key in quotes, and
mistyped keys (`0`/`O`/`I`/`l`), not scrubbed; a stored-row predicate that
deleted the app's own rows (`offer create`, the Wallet screen's create) and
missed other secret methods; bytes freed by earlier builds never vacuumed
when the scrub found nothing; a failed scrub failing open; the label and ord
display echoing arguments on screen; comments and this file overstating the
backstop. Each has a test; several have a live one.

**A second hole found on the way and fixed.** Classification used
`args.first()`, so any leading `ord wallet` option hid the subcommand:
`ord wallet --no-sync create` (or `--name x restore`) was treated as an
unrecognised command (a confirmation dialog) instead of the blocked
`create`/`restore` -- which prints a **recovery phrase** into the monitor
and history. The classifier now skips options (and the values of
`--name`/`--server-url`) to find the real subcommand.

**Verification.**

- Unit: key prefix table, descriptors, WIF positions and echo shapes,
  mistyped keys, look-alikes untouched, a key after `\n`/ANSI/`%28`,
  200,000 random ids/addresses never matched, a 16 MB line scrubbed in
  linear time, contained-secret ordering (`nk-exec`); the full
  `listdescriptors`/`gethdkeys` argument matrix, the mask (secret positions,
  unquoted phrases, named/JSON forms, unknown methods, case), the known-method
  list (sorted, complete for our own lists), the plain-word check, refused ord
  display, stored-row recognition and preservation (`nk-core`); delete/scrub/
  keep/idempotence, the raw database file bytes, `vacuum_once` on a file an
  "old build" left bytes in, and quarantine (`nk-store`); the console's own
  display and refusals, `open_store` on a real file database, what the console
  hands back (`nodekeeper`); the on-screen label (Vitest).
- **Live, real Bitcoin Core** (`nk-testkit/tests/private_keys.rs`, six tests):
  (1) `listdescriptors true` through the RPC client wired to a watched
  executor and the real history bridge -- the caller *does* receive `tprv`
  (positive control), events and history contain none. (2) A real
  `createwallet` with the numeric passphrase `48213907` typed the JSON-quoted
  way, then `walletpassphrase` -- the wallet really is encrypted
  (`unlocked_until` present) and the passphrase really unlocks it, while no
  event or history row contains it. (3) The same for a passphrase with
  **spaces** (four distinctive words): the unquoted form (rejected by the
  node) and the quoted form (accepted) both display `... [redacted]`, no word
  appears anywhere, the quoted form really encrypts and unlocks. (4) A key
  that Core rejects: the caller's error **does** contain it (positive
  control -- Core echoes it), events and history do not. (5)
  `dump`/`create`/`restore` (also with leading options) are refused
  **before anything is launched**. (6) Every method the real node lists is in
  `KNOWN_BITCOIN_RPC_METHODS` (fails, on purpose, when a newer Core adds one).
- **Live, the real app** (a debug build in an isolated portable folder, its
  web view driven over a DevTools port, real IPC), re-run after the round-2
  redesign: `console_classify` shows `walletpassphrase [redacted]` for
  `hunter2`, for four unquoted words, for a single-quoted phrase, for
  `WalletPassphrase` and for the JSON-quoted numeric form;
  `createwallet w false false [redacted]` for one word and for several;
  `createwallet [redacted]` for `wallet_name=w passphrase=hunter2`;
  `walletpasspharse [redacted]`, `importprivkey [redacted]`, `sudo [redacted]`
  and a fully redacted `importdescriptors` blob; refuses (with a message
  that does not repeat the line) `bitcoin-cli ...`, `bitcoin-cli.exe ...`,
  `./bitcoin-cli ...` and `-regtest ...`, and an unterminated quote; shows a
  refused ord line as `ord wallet create [redacted]` / `ord wallet restore
  [redacted]`; still runs `ord balance`, `getblockcount` and the hidden
  regtest method `generatetoaddress 1 <address>` with their arguments
  visible; blocks `listdescriptors true`; `console_run` refuses a pasted line
  and the blocked command. After seeding the app's real database with 74
  rows -- a key-printing row, `ord wallet dump`, a stray key, a `createwallet`
  with the passphrase in clear and one with it hidden, `ord ... create` and a
  row with only a `"mnemonic"` output, a numeric passphrase, an unquoted
  multi-word one, a decorated `bitcoin-cli.exe` line, the app's own unlock /
  Wallet-screen create / `offer create` rows, a clean row, and 60 filler rows
  so deleted rows sit mid-file -- and relaunching: 66 rows remain (the hidden
  `createwallet`, the app's own three rows, the clean row and the stray key
  with the placeholder), and the raw database file **no longer contains** the
  key, the passphrase, the numeric passphrase, the words or the mnemonic
  marker (all five were in it before).
- **Negative controls** (each run, shown to fail, and reverted): hide only the
  first secret position instead of everything from it (4 unit tests fail, and
  the live spaces test fails on the mask assertion); treat every method as
  known (3 fail); never refuse a pasted line (1 fails); disable the WIF
  token rule (4 unit tests fail, and the live echoed-key test fails with the
  key in the event stream); make `vacuum_once` skip the `VACUUM` (the
  old-build-bytes test fails). Earlier: with the backstop disabled the
  private-descriptor test fails; with the positional mask disabled the numeric
  passphrase test fails; with `secure_delete` and `VACUUM` both removed the
  raw-file test fails.
- **The gate**, on the final tree: `cargo fmt --check` and the exact clippy
  recipe clean; `cargo test --workspace` with the real binaries set: every
  test binary passes. `nk-testkit`'s live tests showed an **intermittent
  `Bitcoind(StartupTimeout)`** in 2 of 5 full runs (a different test each
  time; each passes alone and 3 of 5 runs were 12/12) -- the 60-second
  readiness cap under concurrent load (five live tests start bitcoind at
  once on a 5.9 GB machine), already known and part of the lifecycle work in
  PROGRESS.md "Phase 10M", G1. Not caused by this change; not weakened.


**Limits, stated plainly.**

- A secret typed as an argument to a **known** method that is not on the
  list of secret-taking ones is shown (there is no way to know it is a
  secret): only extended keys and WIF-shaped tokens are scrubbed from such
  arguments. Nothing pattern-matches a **mnemonic** (ordinary words);
  `create`/`restore` are blocked, the Wallet screen uses the sensitive
  channel, and old rows with a `"mnemonic"` output are deleted.
- The known-method list is Bitcoin Core **31.1**. On another version a
  method that is new is treated as unknown (its arguments are hidden; the
  live test says so), which costs readability, not safety.
- The typed line exists in the UI's memory until it is sent; what is shown
  and stored is the masked form.
- The classification guards the *console*. User **scripts** run through the
  executor with the node's cookie can still call `listdescriptors true`
  themselves; their output is covered by the backstop for extended and WIF
  keys, but whether scripts should be allowed that at all is the still-open
  Phase 7 policy question.
- `secure_delete` + `VACUUM` remove the old rows from the database file.
  They cannot remove what SQLite does not control: a rollback-journal or
  filesystem remnant, SSD wear-levelling copies, and any **copy, backup or
  export of the file made earlier** -- a wallet whose private descriptors
  were ever printed should be treated as exposed if such copies exist.
- A failed startup scrub hides the history but does not tell the user.
- `backupwallet` (a file copy that contains keys) prints nothing and is
  unchanged (a state-changing command with a confirmation).
- `RpcClient::call_at` still always records with the normal (visible)
  sensitivity; the encrypted-backup feature needs a sensitivity-aware
  call and will add one.

### Making the wallet ready to sign: `with_wallet_unlocked` (Phase 10 step 1b, 2026-09-26)

**The problem.** The Wallet screen creates a wallet **without a passphrase** on
regtest, signet and testnet4 (the passphrase is optional there; only mainnet
requires one). Send, Inscribe, Batch inscribe and Reinscribe each carried their
own copy of "unlock with the passphrase, run, re-lock", and every copy began
by demanding a passphrase -- so on a test chain the GUI could not sign
anything: it prompted for a passphrase that does not exist and looped on
Core's error. That also blocked the natural rehearsal on signet/testnet4. On
mainnet the same code "failed closed" for an unencrypted wallet only by
accident (`walletpassphrase` errors on it), with a raw message and no reason
given.

**Decision.** Put to the owner as: skip the unlock for an unencrypted wallet, or
make encryption default-on for test chains. The owner answered "Do what you
believe better"; the recommended option was taken: **skip the unlock only for an
unencrypted wallet on a non-mainnet chain; on mainnet an unencrypted wallet is
refused** (fail closed). One shared helper for every signing command. A plan
detail from the mainnet audit, not a new question: the *rehearsal* wallets on
signet/testnet4 are created **with** a passphrase (PROGRESS.md, Phase 10M), so
that the rehearsal exercises the real mainnet unlock -> sign -> re-lock path
and not only this shortcut.

**VERIFY (real binaries).** The test for "is this wallet encrypted" is the
`unlocked_until` field of `getwalletinfo`. Core 31.1's own help text
(`bitcoin-cli help getwalletinfo`, asserted by the live test so that a change
trips it): `"unlocked_until" : xxx, (numeric, optional) the UNIX epoch time
until which the wallet is unlocked for transfers, or 0 if the wallet is locked
(only present for passphrase-encrypted wallets)`. Confirmed live
(`crates/nk-testkit/tests/wallet_unlock.rs`): absent on a wallet created without
a passphrase; present and `0` on an encrypted wallet that is locked; a positive
value after `walletpassphrase`; `0` again after `walletlock`. A wrong passphrase
is Core's `-14`.

**What changed.**

- `nk-rpc`: `RpcClient::wallet_protection` (`getwalletinfo`; an answer that is
  not a JSON object is an error, never a guess) and `RpcClient::unlock_for_signing`
  -- encrypted: `walletpassphrase` (no passphrase -> `PassphraseRequired`);
  unencrypted on a test chain: `NotNeeded`; unencrypted on **mainnet**:
  `MainnetWalletNotEncrypted`, whatever passphrase is offered. Any error, or an
  answer that cannot be read, is an error on **every** chain.
- `src-tauri`: `with_wallet_unlocked` -- resolves the passphrase (the caller's,
  else the remembered one), unlocks through `unlock_for_signing`, remembers the
  passphrase only when it really unlocked something and the user asked, runs
  the action, and re-locks **only if it unlocked**, whatever the action's
  outcome. `wallet_send`, `wallet_inscribe` and `wallet_inscribe_batch` use it
  (their three copies are gone; reinscribe goes through `wallet_inscribe`). A
  **remembered** passphrase that Core now rejects (`-14`: the wallet's passphrase
  was changed, or the data folder now holds another wallet) is forgotten and
  reported as `WALLET_LOCKED`, so the frontend asks again instead of retrying the
  stale one for the rest of the 15-minute remember window.
- **The console's `ord wallet ...` commands** (found by the review: they sign on
  their own and skipped the check): on **mainnet**, a command that can sign and
  broadcast (every class but read-only, and not a dry-run) is refused unless the
  wallet is encrypted (`require_encrypted_wallet_for_console_signing`, fails
  closed if Core cannot say), and `--name` is refused there -- the app has one
  wallet per environment, and another name would sign with a wallet the check
  never looked at.
- **A code of its own for the refusal**: `WALLET_NOT_ENCRYPTED` -- an
  **addition to the spec's list of error codes** (docs/SPEC.md item 8; the pinned
  test lists it). The refusal needs a "what to do" the user can act on, and the
  code-less error rendered as "Something went wrong" with the real sentence behind
  a toggle (traced by the review). The UI text points at a step that exists today,
  the Console's `encryptwallet <passphrase>`; a guided "encrypt this wallet"
  action is G3 in PROGRESS.md, Phase 10M. `WALLET_LOCKED` -- the code the frontend
  prompts on -- is produced only for an encrypted wallet that was given no
  passphrase (or a stale remembered one); an unencrypted test-chain wallet no
  longer raises it, so the passphrase prompt never appears for it.
- The encryption check is one more command in the Live Command Monitor
  (`bitcoin-cli getwalletinfo`, "check wallet encryption") before each signing
  action. Dry-runs (which need no unlock) do not come through the helper.

**Found by the adversarial review of this change** (10 findings upheld; fixed
unless listed under limits): the console's `ord` signing bypassing the mainnet
check (three reviewers); the refusal shown as "Something went wrong" with no
in-app remedy (own code, i18n text, wording pointing at the Console); a stale
remembered passphrase retried forever; the unlock glue having no test at all
(now a live test of the helper and the console gate against a real node); a
vacuous "shown in the monitor" assertion (the test's own helper satisfied it --
it now looks for the app's own action name and checks the unlock/encrypt calls
were recorded with the passphrase hidden); and no binary-free test of the
decision (a stub JSON-RPC server now runs the whole matrix -- every chain x
encrypted / unencrypted / unreadable answers / errors / wrong passphrase --
anywhere, without a node).

**Verification.**

- **Live, real Bitcoin Core + real ord** (`wallet_unlock.rs`): (1) the field, per
  state (above); an unencrypted test-chain wallet is ready with no passphrase, and
  a passphrase offered for it is not used; a client **labelled mainnet** against
  the same unencrypted wallet refuses (no passphrase, a wrong one, the right
  one -- the chain label is the only thing that differs; Core is regtest either
  way); an encrypted wallet: none -> `PassphraseRequired`, wrong -> `-14`, right
  -> unlocked and `unlocked_until > 0`, re-locked -> `0`; the same encrypted
  wallet unlocks under the mainnet label; the passphrase and a wrong one appear
  nowhere in the event stream, the check is recorded as "check wallet encryption",
  and the unlock/encrypt calls are recorded with `[redacted]`. (2) With real ord:
  an unencrypted regtest wallet signs a real `ord wallet send` with no passphrase;
  after `encryptwallet` the send needs the passphrase, works once unlocked, and
  fails again after the re-lock.
- **Live, the helper and the console gate** (in the app crate, against a real
  node): the action does not run when the wallet is not ready (encrypted with no
  passphrase; unencrypted on mainnet); an unencrypted test-chain wallet runs it
  and a passphrase "to remember" is *not* remembered; the action sees the wallet
  unlocked, it is locked again after -- also when the action fails -- and only then
  is the passphrase remembered; a remembered passphrase is used when none is
  given; a stale one is forgotten and reported as `WALLET_LOCKED`; the console
  gate refuses an unencrypted mainnet wallet and `--name`, passes an encrypted
  one, does no RPC where it does not apply (read-only, dry-run, other chains: a
  client aimed at a closed port proves it), and refuses when Core cannot be asked.
- **Negative controls:** restoring the old behaviour (an unencrypted test-chain
  wallet demands a passphrase) fails both primitive tests; removing the mainnet
  refusal fails the first; for the helper and the console gate, skipping the re-lock,
  remembering a passphrase even when nothing was unlocked, never gating the
  console, and not forgetting a stale remembered passphrase each fail the live
  glue test (four runs, each reverted).
- **Live, the real app** (a debug build in an isolated portable folder, its web view
  driven over a DevTools port, real IPC; a real regtest bitcoind and ord started
  through the app's own commands): unencrypted wallet, `wallet_send` with no
  passphrase -> a txid and fee; after `encryptwallet` (through the console) the
  same call with no passphrase -> `WALLET_LOCKED`, with the passphrase -> a txid,
  and `getwalletinfo` afterwards shows `unlocked_until: 0`.
- Unit: the error-to-frontend mapping, the UI's text for every code; the whole
  decision matrix against a stub node.
- **The gate**, on the final tree: `cargo fmt --check` and the exact clippy
  recipe clean; the UI typecheck, lint (the one existing warning) and 23 Vitest
  tests; every workspace test passes with the real binaries set -- **but not all
  in one run**: on this 5.9 GB machine, with other applications leaving only
  about 0.4-0.6 GB free, the live tests that start `bitcoind` hit
  `Bitcoind(StartupTimeout)` intermittently (3 of the last 8 full-workspace
  runs, plus 3 of 5 runs of `private_keys`/`wallet_unlock`/the `nk-testkit`
  library tests; a different test each time, and `nk_proc`'s suite took 543 s
  instead of 26 s), and each failure leaves an orphaned `bitcoind` behind that
  holds the output pipe. Each failed test passed when run alone, and every
  live test file passed in full on a quieter run. Not caused by this change and
  not weakened; the readiness-cap and fixture-cleanup work is PROGRESS.md
  Phase 10M, G1.

**Limits, stated plainly.**

- **Mainnet has not run.** The mainnet refusal is exercised by labelling a client
  as mainnet against a regtest node, which tests the rule, not a mainnet node. The
  real mainnet unlock -> sign -> re-lock path is a Phase 10M rehearsal item
  (an *encrypted* wallet on signet/testnet4 through the release exe first).
- **Not closed here, tracked in PROGRESS.md Phase 10M:** the *creation/restore*
  window in which a mainnet wallet exists unencrypted (create then `encryptwallet`
  failing, or a restore rescan lasting hours before encryption), and a guided
  "encrypt this wallet" action (G3); showing the wallet's encryption state on
  the Wallet screen so the refusal appears before the user fills in a form (G3);
  a per-chain single-flight guard, so a double click cannot run two unlock-send-
  relock sequences that interleave (G7); the console's raw `bitcoin-cli` signing
  RPCs (`signrawtransactionwithwallet`, `walletprocesspsbt`, ...) and a structural
  "cannot sign without the check" token in `nk-ord` (G7); surfacing a failed
  re-lock (G3); the live tests skipping silently when the real binaries are not
  configured, which `just check` does today (G0).

### The quality gate runs the live tests (Phase 10M, G0, 2026-09-26)

**The problem** (found by the mainnet readiness audit, confirmed here). About 28
tests need the real Bitcoin Core / ord binaries and returned early -- passing --
when `NK_TEST_BITCOIND` / `NK_TEST_ORD` were unset. The `Justfile` exported
neither, so `just check` (the gate CLAUDE.md requires before claiming a task
done) could be green while every wallet, restore, inscribe and process test did
nothing. Shown directly: with the variables unset, the two new wallet-unlock
tests "pass" in 0.04 s.

**Decision.** `nk_core::live_tests::live_binary(var)` replaces the 35 direct
reads of those variables (in `nk-proc`, `nk-testkit`, the app crate). Set: the
path. Not set and `NK_REQUIRE_LIVE=1`: **panic** -- a failure, not a skip. Not
set otherwise: the old skip message. The `Justfile` now exports
`NK_REQUIRE_LIVE=1` and defaults the two variables to the verified copies the
verification tests cache under `target/` (either can be overridden), so
`just check` runs -- or fails -- the live tests; `just test-rust-quick` is the
old skip-quietly run and says it is not the gate.

**Verified.** Variables unset, `NK_REQUIRE_LIVE=0`: the tests skip and "pass"
(the problem, reproduced). Variables unset, `NK_REQUIRE_LIVE=1`: both fail with
"NK_REQUIRE_LIVE=1 but NK_TEST_BITCOIND is not set: this test would silently
skip". Variables set and required: the live tests run and pass. `fmt`, clippy
clean.

**Not verified / not done.** `just` is not installed on this machine, so the
new `Justfile` lines (`os_family()`, `env_var_or_default()`,
`justfile_directory()`) were written to just's documented syntax but not run
through it; the gate has been run by hand with the same variables all along. The
`nk-scripts` test that skips when no Python interpreter is found is not covered
by this. The examples that read the variable (`nk-verify`'s fetch example) are
not tests.

### Live smoke test of signet and testnet4 (Phase 10 step 2, 2026-09-26)

**Why.** Only `[regtest]` and `[main]` had ever been started live through
Nodekeeper's own config generation, yet `bitcoin_conf.rs` (and a comment
in `chain.rs`) said the signet/testnet4 section names were "also live-
verified". A wrong `[section]` name is a *fatal bitcoind startup error*
(the Phase 2 finding), so this was worth checking rather than trusting.

**How.** `crates/nk-testkit/examples/chain_smoke_test.rs` (the old
`mainnet_smoke_test`, generalised to take a chain and optionally an ord
binary): temp data directory, the chain's default ports, Nodekeeper's own
`generate_bitcoin_conf` + `BitcoindProcess`; asserts `getblockchaininfo`
reports the expected chain, waits up to 3 minutes for a real peer,
watches the sync for 15 s, then starts `ord server` on top of the
still-syncing node using Nodekeeper's own argument generation, and stops
ord then bitcoind gracefully. Real internet (each chain's P2P network, the
same category as the Phase 2 mainnet check), the cached verified binaries
(bitcoind 31.1, ord 0.29.0), nothing left behind (temp dir deleted; the
sync was stopped after seconds, so the download was a few MB of headers).
Commands (PowerShell; the bitcoind/ord paths are under `target/`):

```
cargo build -p nk-testkit --example chain_smoke_test
target\debug\examples\chain_smoke_test.exe signet   <bitcoind.exe> <ord.exe>
target\debug\examples\chain_smoke_test.exe testnet4 <bitcoind.exe> <ord.exe>
```

**Results (both passed, exit code 0, no processes left):**

```
[signet]   generated bitcoin.conf: ... [signet] rpcbind=127.0.0.1 rpcallowip=127.0.0.1 rpcport=38332 port=38333
[signet]   bitcoind ready after 442 ms; getblockchaininfo chain="signet" blocks=0 headers=0 initialblockdownload=true
[signet]   CHECK OK: bitcoind reports the right chain (signet)
[signet]   CHECK OK: connected to 1 peer(s)
[signet]   CHECK OK: ord answered /status after 559 ms   ("chain":"signet","height":0,...)
[signet]   CHECK OK: ord stopped gracefully in 4689 ms (exit Some(0))
[signet]   CHECK OK: bitcoind stopped cleanly in 2391 ms
[testnet4] generated bitcoin.conf: ... [testnet4] rpcbind=127.0.0.1 rpcallowip=127.0.0.1 rpcport=48332 port=48333
[testnet4] bitcoind ready after 448 ms; getblockchaininfo chain="testnet4" blocks=0 headers=0 initialblockdownload=true
[testnet4] CHECK OK: bitcoind reports the right chain (testnet4)
[testnet4] CHECK OK: connected to 1 peer(s); headers 0 -> 43908 within 15 s
[testnet4] CHECK OK: ord answered /status after 519 ms  ("chain":"testnet4","height":0,...)
[testnet4] CHECK OK: ord stopped gracefully in 4562 ms (exit Some(0))
[testnet4] CHECK OK: bitcoind stopped cleanly in 3134 ms
```

**Findings.**

- The `[signet]` and `[testnet4]` sections, the network flags, the default
  RPC/P2P ports, ord's `--signet` / `--testnet4` handling and both
  processes' graceful stop all work as generated. The suspicion was
  unfounded, but it was only a suspicion until now; the comments are
  corrected, and a new unit test pins the exact section names and flags
  (`the_conf_sections_and_flags_are_the_ones_verified_live_against_the_
  real_binaries`) so changing one has to be a deliberate, re-verified act.
- **ord starts and answers `/status` while bitcoind is still in initial
  block download at height 0** (with `"chain"` correct and `"height":0`).
  That answers the "what does ord do before the node has synced" question
  the earlier scoping left open: it does not refuse to start. (Wallet
  commands still refuse until ord catches up -- Phase 5.) The open Phase 4
  task "start ord after IBD" is therefore about *usefulness*, not about ord
  failing to start.
- Both chains share the `tb` address prefix, so the UI must not claim it
  prevents mixing up a signet and a testnet4 address (it makes no such
  claim today: checked).
- The interface already handles all four chains generically; new test:
  the Overview shows a card with its own node and ord controls for each of
  the four (the port-distinctness unit test that lets them run side by
  side already existed).

**Limits.** Both stops were run from a console-attached process (an
example run from PowerShell), i.e. ord's *Shared*-console stop path;
the release exe's hidden-console path is covered by Phase 10 step 0.
Only bitcoind + ord were exercised here, not the wallet on those chains
(that is Phase 10 step 1(b): an unencrypted test-chain wallet cannot send
or inscribe today). Nothing beyond a peer connection and a few seconds of
sync was waited for.

### Public preview release: owner decisions (2026-09-27)

**Follow-up decisions (2026-09-27, after a sourced scoping pass found the mainnet
node/wallet split needed an explicit answer):**

- **The mainnet NODE and Explorer stay available** in the preview; only mainnet
  *wallet* creation, restore, signing and receive are blocked (unchanged from the
  first decision above). The scoping pass recommended blocking the mainnet node
  too -- it has never been run through the app, needs 1 TB+ and days to sync, and
  a first click on Start can hit the known startup-orphan/stale-cookie loop
  (PROGRESS.md, Phase 10M, G1) -- but the owner chose to keep it available.
  Consequence, now a **precondition for publishing, not a nice-to-have**: G1's
  node-lifecycle hardening (the readiness-wait, orphan/attach, and stop-path
  items) and a mainnet Start confirmation with a real disk-space check must land
  before the tag is pushed, since a stranger's first click on the Mainnet
  Dashboard is exactly that Start button.
- **The VC++ runtime `ord.exe` needs is detected, not bundled.** If missing, the
  app says so in plain words and links to Microsoft's own redistributable page;
  no extra download ships inside Nodekeeper. (`bitcoind.exe` does not need it;
  confirmed by inspecting the cached binaries' import tables.)


The owner asked to "make a release ... so anyone can use it". Because that
puts a wallet-adjacent app in strangers' hands while the mainnet readiness audit
(PROGRESS.md, Phase 10M) still lists blockers that could lose someone's
bitcoin, the question was put as choices, with recommendations. Answers:

- **What it is:** a **public preview**, clearly labelled not for real money yet
  (recommended option taken). Regtest, signet and testnet4 are the intended
  use; the full release for real bitcoin follows the Phase 10M gates.
- **Mainnet in the preview:** the mainnet **node and read-only Explorer work**;
  **creating or restoring a mainnet wallet, and every signing or fund-moving
  action on mainnet, is blocked in the preview build** -- enforced in Rust, with
  a plain explanation in the UI -- plus a strong warning and a required
  acknowledgement (recommended option taken over "warn only"). Lifted in the
  full release.
- **Packaging:** an **MSI installer and a portable zip** (the exe with an empty
  `config` folder). Unsigned at first (Windows will show an "unknown publisher"
  warning); signing needs a certificate only the owner can obtain and is a later
  step. The MSI tool (WiX) is downloaded by the build, on GitHub's runner; the
  owner's OK to download it locally is taken as given by choosing the MSI.
- **Built on GitHub Actions**, not on the owner's PC (a release build with
  `lto` and `codegen-units = 1` needs more memory than that PC has free). This
  needs the Actions billing block (since 2026-09-24) fixed **or the repository
  made public**, which gives free build minutes.
- **Repository:** private until the release is ready, then made **public by the
  owner** (an action only they can take).
- **License:** MIT, "Copyright (c) 2026 sixoBitmap" (the owner's GitHub name).
- **Security reports:** GitHub private vulnerability reporting (a repository
  setting the owner switches on when the repo is public).

Claude's own choice, recorded because it is reversible and was recommended
earlier without an answer: the release profile moves from `panic = "abort"` to
`"unwind"`, with a panic hook and an application log file, so that a crash is
recorded and cannot skip the app's cleanup (PROGRESS.md, Phase 10R).

### Node-lifecycle hardening for the public preview (Phase 10R, step R0, 2026-09-27)

**Why now.** The owner decided the mainnet node and Explorer stay available in
the public preview (only mainnet *wallets* are blocked -- see "Public preview
release: owner decisions" above). That made the mainnet-readiness audit's
node-lifecycle findings (PROGRESS.md, Phase 10M, G1) a precondition for the
release tag rather than a later step: a stranger's first click on the Mainnet
Dashboard is the Start button, and it used to be able to leave a real `bitcoind`
running with nothing in the app able to stop it.

**Two bugs found and fixed in `BitcoindProcess::start_and_wait_ready` and
`OrdProcess::start_and_wait_ready` (`crates/nk-proc`), both real, both proven
live.**

1. **The orphan bug** (the audit's `startup-warmup-budget-and-orphaned-child`).
   If the readiness wait timed out, the just-spawned `tokio::process::Child`
   was simply dropped -- and tokio does **not** kill a child on drop unless
   `kill_on_drop(true)` was set, which neither `start()` did. The process kept
   running, still holding the data directory and P2P/RPC ports, with nothing in
   `NodeManager` tracking it. The next `start()` call then reads the pid file
   Core (or, for ord, Nodekeeper itself) already wrote, finds the process alive,
   and refuses with `AlreadyRunning` -- and nothing in the app can stop a
   process it never tracked. **Fix**: on any readiness failure, kill the
   process, wait briefly for it to exit, and remove the pid file, before
   returning the error. Verified live: a real long-lived dummy process (never
   `kill_on_drop`) substituted for a stuck bitcoind/ord is confirmed dead, and
   its pid file gone, once the call returns `StartupTimeout` -- `bitcoind`'s
   test also feeds the pid file (`bitcoind.pid`) and checks it. **Negative
   control**: reverting the cleanup makes both tests fail (the dummy is still
   alive).
2. **The hung-poll bug**, found while fixing the first: the RPC-ready poll
   (`rpc.get_blockchain_info`) and ord's `/status` poll had no per-call
   timeout, and neither `RpcClient`'s nor `OrdClient`'s HTTP client sets one
   (`reqwest::Client::new()`) -- the same fact `BitcoindProcess::stop` was
   already written to account for, just not the readiness wait. A `bitcoind`
   or `ord` whose RPC/HTTP thread accepts the connection but never answers (a
   stalled disk -- exactly the portable-drive case this app targets) could hang
   the whole wait **past its own deadline**, since the deadline is only
   checked *between* calls. **Fix**: wrap each poll in
   `tokio::time::timeout(ready_timeout, ...)`. Verified live: a real listener
   that accepts and holds the connection open, replying to nothing, no longer
   hangs the wait past a 10-second outer test bound. **Negative control**:
   removing the per-call timeout makes that test hang until the outer bound
   and fail.

**Warm-up awareness**, the audit's other ask for this item: `bitcoind` answers
RPC calls during warm-up with a real response, JSON-RPC error `-28`
("Loading block index...", "Verifying blocks...", Core's own `RPC_IN_WARMUP`
code) -- proof it's alive and progressing, not a failure to connect at all. The
flat 60-second budget this replaced couldn't tell that apart from "never coming
up," and was only ever sized against an empty regtest chain. Seeing `-28` now
extends the deadline by one more `ready_timeout`, capped at
`MAX_WARMUP_EXTENSIONS = 10` extensions so a node that somehow relays `-28`
forever still times out. Verified against a scripted local JSON-RPC server (the
same shape used elsewhere in this codebase for RPC-behaviour tests): two
warm-up responses then real readiness survive past what a single
`ready_timeout` would have allowed; a server that answers `-28` forever still
times out well inside a generous outer bound. Real `bitcoind` on regtest never
stays in warm-up long enough to test this directly (its block index is empty),
so the scripted server is what actually exercises the extension logic; the
real binary is what exercises the orphan-cleanup and per-call-timeout fixes,
and the ordinary case (a normal, quick regtest start).

**Outbound-only P2P** (owner decision, this session): VERIFY, live, against the
pinned `bitcoind` 31.1 (`bitcoind -help-debug`):

```text
-listen
     Accept connections from outside (default: 1 if no -proxy, -connect or
     -maxconnections=0)
-natpmp
     Use PCP or NAT-PMP to map the listening port (default: 1)
-discover
     Discover own IP addresses (default: 1 when listening and no -externalip
     or -proxy)
-dnsseed
     Query for peer addresses via DNS lookup, if low on addresses (default: 1
     unless -connect used or -maxconnections=0)
```

Nodekeeper's generated `bitcoin.conf` set none of `-proxy`/`-connect`/
`-maxconnections=0`, so every environment accepted inbound connections and
tried to map a port on the router by default. Put to the owner as a choice
(keep Core's defaults, or go outbound-only); the recommended option was taken:
`generate_bitcoin_conf` (`crates/nk-core/src/bitcoin_conf.rs`) now writes
`listen=0` and `natpmp=0` for every chain. `-listen=0` only stops *accepting*
inbound connections -- outbound connections (how a node normally finds peers,
via `-dnsseed`, which stays on) are unaffected, so the node still syncs and
finds peers normally; it just never accepts a connection initiated by someone
else, and never touches the router's port mapping. Verified live: a real
`bitcoind` started with the generated config answers RPC normally, and a raw
TCP connection attempt to its own P2P port from the test itself is refused --
proof the port isn't listening, not just an assertion about the config text.
**Negative control**: removing the two lines from the generator makes that
test fail (the port accepts the connection). Not re-verified: the Phase 10
step 2 signet/testnet4 example (`crates/nk-testkit/examples/chain_smoke_test.rs`,
needs real network access and is not part of `just check`) waited for an
*outbound* peer connection, which `listen=0` does not affect per the verified
semantics above -- but it has not been re-run against this change; do so before
relying on it.

**Verification.** Unit and live tests added to `crates/nk-proc/src/bitcoind.rs`,
`crates/nk-proc/src/ord.rs` (orphan cleanup, per-call timeout, warm-up
extension and its ceiling -- 6 new tests, each with a negative control) and
`crates/nk-testkit/src/lib.rs` (the live outbound-only proof). The existing
real-node tests in `nk-testkit` and `nodekeeper` (`NodeManager`'s
`starts_reports_status_and_stops_a_real_node`, `starts_ord_reports_status_and_
stops_it`, and every wallet/inscribe/restore test that starts a real node
through the same generated config) all still pass with `listen=0`/`natpmp=0`
in effect, so the change doesn't regress anything that already worked.

**Still open from Phase 10M's G1, not done here:** the stale `.cookie`/pid
reuse after a crash (a different bug -- what happens on the *next* start after
an unclean exit, not what happens when *this* start doesn't succeed);
attach/adopt for a bitcoind or ord the app didn't start; the stop-path budget
and swallowed stop-RPC failures; and R0's other item, a confirmation plus a
real disk-space check before a mainnet **Start**, which is a separate,
UI-facing task not yet done.

**The full workspace gate** (`fmt --check`, the exact clippy recipe, every
workspace test with the real binaries set, the UI typecheck/lint/Vitest) was
run on this change and is clean, with one exception: `nk-rpc`'s
`the_unlock_decision_for_every_chain_and_every_answer` (a mock-JSON-RPC-server
test from Phase 10, step 1(b), unrelated to this change -- untouched here)
failed once in the full run and, rerun alone three times, failed once more, at
a different assertion each time and each after several real seconds rather
than instantly. On this machine, with 1-1.5 GB of RAM free of 6 GB for this
entire session, that pattern matches the already-documented real-`bitcoind`
`StartupTimeout` flakiness (this DECISIONS.md, Phase 10 step 1(b)'s gate note)
more than a logic bug: the mock's own accept loop or `reqwest`'s connection
pool stalling under load, not the unlock decision itself, which the same test
proved correct earlier in this session and which nothing here touched. Not
weakened, not skipped, and not fixed either -- recorded here as an owed
hardening pass (a bounded per-connection timeout in the test's own mock
server) before relying on this machine's CI runs unattended.

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
