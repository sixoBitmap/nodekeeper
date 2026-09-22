# Decisions log

Records every VERIFY result (exact commands + relevant output), pinned-value
sources, and any approved deviation from docs/SPEC.md.

## Status

Phase 0 feasibility spike: **complete**. Two items need a decision from the
project owner before Phase 1 can proceed safely (see "STOP AND ASK" below).
Everything else either confirms the spec as written or needs no change.

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

ord (via `ord settings`, no `--data-dir` override):
```
regtest: data_dir = %APPDATA%\ord\regtest
mainnet: data_dir = %APPDATA%\ord
```
Same per-chain-subfolder pattern as Core. Nodekeeper always passes an
explicit `--data-dir` per environment anyway (per Foundation A's
`data/<environment>/ord` layout), so this default never actually applies in
the app — but confirms the path-resolution module's assumptions are correct.

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

## STOP AND ASK — decisions needed before Phase 1

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

## Approved deviations from SPEC.md

None yet — pending your answers to A and B above.
