# Decisions log

Records every VERIFY result (exact commands + relevant output), pinned-value
sources, and any approved deviation from docs/SPEC.md. Newest entries at the
bottom of each section unless noted.

## Status

Phase 0 feasibility spike: in progress.

## Pinned values

None recorded yet. Builder-key fingerprints (bitcoin-core/guix.sigs) and ord
release SHA-256 hashes will be listed here with their source URL once
collected, for the user to independently check.

## VERIFY items (Phase 0)

Each item below will be filled in with: the exact command(s) run, the
relevant output, the conclusion, and the date.

1. Do ord wallet commands work with an encrypted Core wallet, and how is it
   unlocked? — _pending_
2. Does ord shut down cleanly on SIGINT (macOS/Linux) and CTRL_BREAK_EVENT
   (Windows)? — _pending_
3. Which ord commands accept secrets via stdin? — _pending_
4. ord server's default bind address and the flag to change it — _pending_
5. Does `ord wallet send --dry-run` exist, and what does it output? —
   _pending_
6. Reinscribe syntax; batch reinscribe support — _pending_
7. Which features need which index option (index-sats/index-runes/
   index-addresses)? — _pending_
8. Per-chain data paths for Bitcoin Core and ord — _pending_
9. testnet4 support in ord; whether ord releases publish checksums —
   _pending_
10. Whether a built-in regtest command (e.g. `ord env`) exists — _pending_

## Approved deviations from SPEC.md

None yet.
