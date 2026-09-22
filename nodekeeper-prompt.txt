You are building "Nodekeeper", a cross-platform desktop app (Tauri 2 + Rust
backend, React + TypeScript + Tailwind + shadcn/ui frontend, Recharts for
charts) that makes it easy for non-technical users to install and run a
Bitcoin Core full node and the ord ordinals indexer/wallet, and to use ord's
features through a beautiful GUI. It must run on Windows, macOS, and Linux,
both as a normal installed app and in a portable mode from an external SSD.
It must support running several networks at the same time (e.g. mainnet
and regtest side by side), and it must let users see, live, every command
the app runs behind the scenes.

TARGET USER
Someone who wants to run their own node and self-custody ordinals but finds
the command line intimidating. Power users must also be able to run any
bitcoin-cli or ord command and their own scripts.

======================================================================
RULES FOR HOW YOU WORK
======================================================================
- Source of truth: save this entire spec verbatim as docs/SPEC.md. If
  code and spec disagree, the spec wins unless DECISIONS.md records a
  change I approved.
- Persistent project memory: create and maintain ARCHITECTURE.md,
  DECISIONS.md, and PROGRESS.md (a checklist of every phase, its tasks,
  and its acceptance criteria). Also create CLAUDE.md at the repo root
  (under ~150 lines): a one-paragraph project summary, the non-negotiable
  rules (STOP AND ASK, VERIFY, all commands through the executor, the
  security rules in short form), the quality-gate command, and pointers
  to docs/SPEC.md, ARCHITECTURE.md, DECISIONS.md, and PROGRESS.md.
- Resume protocol: at the start of every session, read CLAUDE.md,
  PROGRESS.md, and DECISIONS.md, then only the SPEC sections relevant to
  the current phase. Update PROGRESS.md after every completed task, so
  work can resume after a context reset or a new session.
- Verify, don't assume: ord's flags, defaults, and behavior change between
  versions. Before writing any ord integration, check the current ord
  documentation and CLI help (`ord --help`, `ord server --help`,
  `ord wallet --help`, `ord wallet inscribe --help`,
  `ord wallet send --help`) for the version you install. Every item below
  marked (VERIFY) must be checked this way and the result recorded in
  DECISIONS.md with the exact commands run and the relevant output.
- STOP AND ASK: if a VERIFY result contradicts this spec, or a required
  feature turns out to be impossible as written, stop. Explain what you
  found, propose options with their trade-offs, and wait for my decision.
  Never quietly improvise a workaround. This applies especially to
  mainnet wallet encryption, graceful ord shutdown, dry-run support, and
  anything touching seeds, keys, or fund movement. Stop immediately only
  for blockers, security questions, and VERIFY conflicts; collect
  non-blocking questions and ask them at the end of the phase.
- Never invent or recall pinned values from memory. Builder-key
  fingerprints and ord SHA-256 hashes must be taken from the official
  sources (bitcoin-core/guix.sigs, official ord GitHub releases). List
  every pinned value with its source URL in DECISIONS.md so I can check
  them.
- Versions: target the latest stable Bitcoin Core and ord releases at
  build time. Record the minimum supported versions in DECISIONS.md and
  refuse to run older binaries with a clear message.
- Tests:
  - Unit tests for secret redaction, path resolution (including Windows
    long paths), address validation, the fee guard, and index-option
    feature gating.
  - Integration tests that run against real regtest bitcoind and ord
    (via nk-testkit, see the Implementation Guide). CI downloads the real
    binaries and verifies them with the same code the app uses.
  - CI matrix (GitHub Actions) for Windows, macOS, and Linux from Phase 1.
  - Acceptance criteria are marked [CI] or [MANUAL]. Anything needing
    mainnet sync, physical drives, multiple machines, or my own checks is
    [MANUAL], and for those give me a step-by-step manual test checklist.
- Honesty: report exactly which tests ran, on which OS, locally or in CI.
  Never claim something works on a platform where it wasn't tested.
- Never weaken, skip, or delete a test or a security rule to make
  something pass. If you think a test or rule is wrong, explain why and
  ask.
- Storage: settings, command history, templates, and script metadata live
  in SQLite with versioned schema migrations.
- Internationalization: English first, but every user-facing string goes
  through i18n files from day one.
- Code signing: ask me in Phase 0 whether the app will be code-signed and
  notarized (Apple Developer ID, Windows certificate). Design the macOS
  first-launch flow for both answers.
- Scope discipline: do not add features that aren't in this spec (for
  example rune sending, minting, or etching). Suggest them to me instead.

======================================================================
FOUNDATION
======================================================================

A) The environment model
Everything is built on "environments". Each network (mainnet, regtest, and
optionally signet and testnet4) is an independent environment with its own
bitcoind process, ord server process, data directories, cookie file, RPC
port, P2P port, ord HTTP port, index options, wallets, history, logs,
command history, console history, and templates. Multiple environments can
run at once.
- Default ports: mainnet 8332/8333, regtest 18443/18444, signet
  38332/38333, testnet4 48332/48333 (RPC/P2P). Assign a distinct ord server
  port per environment (e.g. 8080, 8081, 8082, 8083). Check each port is
  free before starting; let the user change ports in settings.
- Data layout (installed and portable mode), always relative paths:
    data/mainnet/bitcoin, data/mainnet/ord
    data/regtest/bitcoin, data/regtest/ord
    (same pattern for signet and testnet4)
- Per-chain subfolders: Bitcoin Core puts non-mainnet data in chain
  subfolders (e.g. the regtest cookie is at .../bitcoin/regtest/.cookie),
  and ord does something similar (VERIFY). Resolve cookie, wallet, and
  index paths per chain in one path-resolution module; never hard-code
  them.
- Windows long paths: enable long-path support (application manifest
  longPathAware plus \\?\-prefixed paths where needed) and test with a
  deeply nested data folder on an external drive.
- testnet4 is offered only if the installed ord version supports it
  (VERIFY); otherwise hide it.
- A minimal environment banner (name + color + text label) exists from
  Phase 1 on every screen.

B) The central command executor
Every ord CLI command, bitcoin-cli command, and RPC call goes through one
central executor in the Rust backend. It:
- runs commands as argument arrays, never shell-interpolated strings;
- tags each command with its environment, source, and triggering app
  action;
- redacts secrets before anything is logged, displayed, stored, or
  exported;
- passes secrets (passphrases, mnemonics) through stdin or RPC parameters,
  never as command-line arguments, which other processes can read (VERIFY
  which ord commands accept stdin);
- has a SENSITIVE OUTPUT channel: output from commands that print or
  receive a mnemonic (wallet create, wallet restore, and any similar
  command) goes only to the full-screen seed view. It never reaches the
  command monitor, logs, history, or exports, not even in redacted form;
  the monitor shows "[sensitive output hidden]" instead;
- zeroizes secrets held in memory as soon as they are no longer needed;
- streams status and output to the Live Command Monitor (item 7).
No feature may run a command outside this executor. This is enforced
mechanically (see the Implementation Guide).

C) The process manager
- Tracks every child process (bitcoind, ord server) per environment with
  PID files. Child processes must never become silent orphans.
- On startup, detects already-running bitcoind or ord instances (from a
  service, a previous crashed session, or the user's own install). If the
  data directory matches and the cookie authenticates, offer to attach;
  otherwise refuse with a clear explanation. Never start a second bitcoind
  on the same data directory.
- After a crash or force-quit, the next launch finds leftover processes
  and offers "attach" or "shut down safely".
- On quit: stop services gracefully, or (installed mode only) hand them
  to an OS background service if the user enabled that. Never leave
  untracked processes running.
- Graceful stop:
  - bitcoind: RPC `stop`, then wait for the process to exit.
  - ord (no RPC stop): send SIGINT on macOS/Linux; on Windows, launch ord
    in its own process group (CREATE_NEW_PROCESS_GROUP) and send a
    CTRL_BREAK_EVENT (VERIFY that ord shuts down cleanly on it). Wait with
    a timeout (default 120 s, configurable).
  - If the timeout expires, warn the user that forcing a kill may corrupt
    the ord index (which can take days to rebuild) and ask before killing.
- Single-instance lock: a lock file in the data folder (hostname, PID,
  timestamp) prevents two copies of Nodekeeper from using the same data
  folder, including the same portable drive opened from two machines.
  Detect and clean stale locks safely.

D) Webview security model
Inscriptions can contain HTML, SVG, and JavaScript. They must never be
able to reach the app's code, Tauri IPC, or the wallet.
- Inscription content is served from the environment's ord server origin
  (http://127.0.0.1:<port>), never from the app's own origin.
- Render inscriptions only inside sandboxed iframes: never
  allow-same-origin, and allow-scripts only when the user clicks "Render
  interactive content" for that inscription. Static previews (images,
  text) are the default.
- Strict Content Security Policy for the app itself. frame-src and
  img-src list exactly the configured ord server origins
  (http://127.0.0.1:<port> for each environment), generated from the
  environment config and updated when ports change. Never use wildcards,
  "*", or broad http: sources to make the gallery work.
- Locked-down Tauri capabilities file: IPC available only to the main
  app window's own origin; no IPC for iframes, remote URLs, or ord server
  pages. Same rules for the embedded explorer.

E) Secrets storage
- Installed mode: secrets in the OS keychain (keyring crate).
- Linux without a Secret Service provider (e.g. no gnome-keyring or
  KWallet): fall back to the encrypted secrets file below, and explain
  why to the user.
- Portable mode (and the Linux fallback): an encrypted secrets file
  protected by a user master password entered at launch. Derive the key
  with Argon2id (strong parameters, per-file random salt) and encrypt
  with an authenticated cipher (e.g. XChaCha20-Poly1305). Keep the
  decrypted key only in memory, and clear it on lock or exit. Warn that a
  forgotten master password cannot be recovered; the wallet seed is still
  the ultimate backup.
- Wallet encryption passphrases and mnemonics are NEVER stored here, in
  the keychain, or anywhere else (see item 3).

F) Index options and feature gating
- Each environment records its ord index options (index-sats,
  index-runes, index-addresses).
- Features that depend on an index option are hidden or shown with an
  explanation ("Requires the runes index. Enabling it means a full
  reindex.") instead of producing errors:
  - rune balances -> index-runes
  - address lookups in the explorer -> index-addresses
  - sat-level views, possibly including "all inscriptions on this sat"
    in reinscribe mode -> index-sats (VERIFY exactly which features need
    which index for the installed ord version, and record the mapping in
    DECISIONS.md)
- Regtest enables all index options by default, since they cost almost
  nothing there.

======================================================================
CORE FEATURES
======================================================================

0. First-run disclaimer:
   - A short screen before setup, in plain language: this is self-custody;
     nobody (including Nodekeeper) can recover funds without the seed
     phrase; practice on regtest first; mainnet uses real bitcoin. The
     user must acknowledge it once.

1. Setup wizard:
   - Check OS, CPU, RAM, and disk space; warn that a mainnet node plus ord
     index needs 1 TB+ on an SSD (more with --index-sats).
   - Let the user choose the data directory (including external drives)
     and which environments to set up.
   - Offer a "try it safely" option that opens the Regtest Test Lab
     (item 11) before committing to a multi-day mainnet sync. Until the
     Test Lab exists (Phase 8), this opens the minimal regtest start from
     Phase 2.
   - Bitcoin Core verification: download from bitcoincore.org, verify
     SHA256SUMS, and verify SHA256SUMS.asc against builder keys PINNED
     inside Nodekeeper (from the bitcoin-core/guix.sigs builder keys).
     Require at least 3 valid signatures from pinned keys. Use Sequoia-PGP
     with a pure-Rust crypto backend (or another library that works on all
     three OSes without an external gpg). The pinned key list is updated
     only through Nodekeeper updates.
   - ord verification: verify the release's published checksums if they
     exist (VERIFY); in all cases also check against SHA-256 hashes pinned
     inside Nodekeeper for each supported ord version. Refuse any version
     that can't be verified.
   - Fail closed on every verification error, and show the result.
   - Generate bitcoin.conf per environment: txindex=1, prune=0 (pruning
     must be DISABLED when ord is enabled; ord and txindex both require an
     unpruned node), server=1, RPC bound to 127.0.0.1 only, cookie auth,
     dbcache sized to available RAM (accounting for other running
     environments).
   - ord index options (index-sats, index-runes, index-addresses): explain
     disk and time cost, list which app features each one unlocks
     (Foundation F), and state clearly that these choices are effectively
     permanent; changing them later means a full reindex.
   - Recommend starting ord indexing only after Bitcoin Core finishes its
     initial sync (much faster). Default: start ord automatically when the
     node is synced; allow "start now anyway" with a warning.
   - Windows: suggest excluding the data folder from Microsoft Defender
     scanning, with instructions (can make initial sync much faster).
   - Optional OS background services (systemd, launchd, Windows service)
     are a later-phase feature and never available in portable mode.

2. Dashboard (per environment):
   - Bitcoin Core: block height vs header height, verification progress %,
     estimated time remaining, peers, mempool, disk used, uptime.
   - ord: index height vs node height, indexing / caught-up status, and
     which index options are enabled.
   - Status badges with text labels: "Syncing", "Indexing", "Ready".
   - Start / stop / restart per service through the process manager.
   - Log viewer for debug.log and ord output: tail and page large files,
     never load a whole file; search and filter.
   - Disk monitor: projected usage including the ord index; warn well
     before free space gets low (Bitcoin Core shuts down when the disk is
     nearly full).

3. Wallet (visual ord wallet, scoped to the current environment):
   - ord wallet commands depend on a running, synced ord server (VERIFY
     for the installed version). Until ord is caught up, show a clear
     "Waiting for ord to catch up (block X of Y)" state instead of errors.
   - Create or restore: the mnemonic flows only through the sensitive
     output channel (Foundation B). Show it once in a full-screen view with
     a screenshot warning, then require the user to confirm several words.
     Never store, log, or transmit it. The mnemonic may exist in memory
     only during the create/confirm or restore flow and is wiped
     (zeroized) immediately afterward.
   - Wallet encryption: every MAINNET wallet (installed and portable
     mode) must be encrypted with a Bitcoin Core wallet passphrase. Before
     a signing action, the app unlocks it with walletpassphrase for a short
     timeout and locks it again afterwards. VERIFY how the installed ord
     version works with encrypted Core wallets. If it doesn't work cleanly,
     STOP AND ASK; do not pick a workaround yourself. The UI must clearly
     distinguish the encryption password from an optional BIP39
     passphrase, which is a different thing.
   - The wallet encryption passphrase is NEVER stored in the keychain,
     secrets file, or database. Default: the user enters it for each
     signing action. Optional setting: "Remember for this session", held
     in memory only, cleared on lock, app exit, or after a configurable
     idle timeout (default 15 minutes).
   - Multiple named wallets.
   - Balance: cardinal vs inscribed sats; rune balances only when the
     runes index is enabled (Foundation F).
   - Runes are VIEW-ONLY: no rune send, mint, or etch. Say so in the UI
     (e.g. "Rune transfers are not supported in Nodekeeper yet") so the
     wallet never appears to support rune transfers.
   - Receive: address with a QR code.
   - Inscriptions gallery: static previews by default, loaded from the
     ord server and rendered per Foundation D.
   - Send inscriptions or sats:
     - Address checks: reject addresses from the wrong network (e.g. bc1
       on regtest, bcrt1/tb1 on mainnet); warn when sending an inscription
       to a non-Taproot address; general reminder that exchanges often
       don't support inscriptions and they may be lost.
     - Fee rate: estimates only from the local node (estimatesmartfee). If
       no estimate is available (regtest, freshly synced node), use a
       configurable fallback on regtest and require manual entry with
       guidance on mainnet.
     - Absurd-fee guard: warn when the fee is unusually high in sat/vB,
       in absolute terms, or as a share of the amount sent.
     - Preview with `ord wallet send --dry-run` (VERIFY that it exists
       and what it reports; if it doesn't, STOP AND ASK), then a review
       and confirmation screen.
     - MAINNET extra confirmation: every fund-moving action on mainnet
       gets an extra step ("You are on MAINNET - this uses real bitcoin")
       showing the environment name in large text.
   - Transaction history.

4. Inscribe studio:
   - Drag-and-drop file, preview (sandboxed per Foundation D), content-type
     check, size warning.
   - Fee-rate picker (same rules and fee guard as the wallet) and estimated
     total cost; always dry run first and show the cost breakdown.
   - Mainnet extra confirmation, same as the wallet.
   - Visual batch-YAML builder with export and edit.
   - Advanced options (hidden by default): parent/child, metadata, postage.
   - REINSCRIBE MODE:
     - The user picks an owned inscription from the gallery; the app fills
       in its satpoint and uses ord's reinscribe flag (VERIFY syntax).
     - Show all existing inscriptions on that sat, in order, with previews
       and inscription numbers. If this requires an index option that is
       off (Foundation F), say so and show what can be shown.
     - Before the first reinscription, explain: reinscriptions are
       permanent, don't replace the original, and are displayed
       differently across explorers and marketplaces.
     - Only allow reinscribing sats in the user's own ord wallet, and block
       the action if the ord index isn't fully synced.
     - Dry run, then a review screen showing target sat, existing
       inscriptions, new content, fee, and resulting inscription count,
       with a mandatory "I understand this sat already has inscriptions"
       checkbox before broadcasting.
     - Reinscribe entries in batch mode only if the installed ord version
       supports them (VERIFY; hide otherwise).
     - Label reinscriptions as "Reinscription #N on sat X".

5. Explorer:
   - Search inscriptions, sats, transactions, addresses, blocks, and runes
     via the environment's local ord server, with index-dependent searches
     gated per Foundation F. The embedded explorer follows Foundation D.

6. Console and script runner:
   - Command console with autocomplete for bitcoin-cli and ord subcommands,
     command history, and pretty-printed JSON output.
   - Each console tab is locked to one environment, shown in the prompt
     (e.g. "[regtest] $", "[mainnet] $").
   - Safety layer:
     - Read-only commands run instantly.
     - State-changing commands (stop, wallet restore, etc.) show a
       confirmation dialog explaining what they do.
     - Inscription protection: Bitcoin Core spend commands (sendtoaddress,
       sendmany, send, bumpfee, and similar) against a wallet used by ord
       are BLOCKED by default, with an explanation that they can spend
       inscribed sats as fees or change, and a button to use the ord wallet
       send flow instead.
     - Spend commands on other Core wallets: raw commands have no dry run,
       so show a preview built with walletcreatefundedpsbt /
       testmempoolaccept and require an extra confirmation.
     - ord commands that support --dry-run are previewed with it first.
     - Mainnet fund-moving commands get the mainnet extra confirmation.
   - Saved command templates with fill-in fields.
   - Scripts (bash / python / js):
     - Scripts are trusted code with full node control (they receive the
       RPC URL, cookie path, ord server URL, and NKP_NETWORK). Show a clear
       warning when a script is imported or created.
     - The runner requires choosing the environment before running. The
       "regtest only" restriction is enforced by the runner, not the
       script.
     - Detect whether Python and Node are installed on the machine and
       explain what's missing; bash is unavailable on stock Windows, so
       say so.
     - Live output; scheduling is a later-phase feature.
   - Include 3 example scripts: export inscriptions to CSV, alert when the
     node falls behind, daily disk-usage report.

7. LIVE COMMAND MONITOR (show/hide panel):
   - Shows, live, every command the app runs against ord and Bitcoin Core:
     ord CLI commands, bitcoin-cli commands, and RPC calls (RPC calls shown
     as their equivalent bitcoin-cli command so users can learn them). Fed
     by the central command executor (Foundation B).
   - Show/hide: toggle in the top bar and a keyboard shortcut (e.g.
     Ctrl/Cmd + `). A resizable bottom drawer, like browser developer
     tools, that can also pop out into its own window. Remember show/hide
     state and size. An activity indicator on the toggle pulses when
     commands run while the panel is hidden.
   - Each entry: timestamp, environment tag with color AND text label,
     source (ord / bitcoin-cli / RPC / script), full command with
     arguments, status, duration, exit code, and expandable output
     (stdout/stderr streamed live).
   - Show which app action triggered each command (e.g.
     "Inscribe studio -> dry run").
   - Filters: environment, source, status (running / success / error),
     text search.
   - Background polling is hidden by default with a "Show background
     polling" toggle.
   - Per entry: copy command, "Open in console" (pre-fills a console tab
     of the same environment, never runs it automatically), copy output.
   - Panel controls: pause/resume auto-scroll, clear view, export to a
     text file.
   - Redaction happens in the backend before anything reaches the UI,
     storage, or exports; sensitive-channel output never appears at all.
   - Rolling history (e.g. last 5,000 entries per environment) in SQLite,
     relative to the data folder so it works in portable mode.
   - "Learn mode": confirmation dialogs also show the exact command that
     will run.

8. User experience and reliability:
   - Plain-language errors: map common failures (port in use, disk full,
     index behind, index option disabled, wallet locked, RPC warming up,
     ord not synced, binary not verified) to friendly messages with a
     "What to do" button. Technical details are available behind a toggle.
   - Notifications: "Your node is fully synced", "ord is ready", disk
     space warnings.
   - Tray: minimize to tray while services run (installed mode).
   - Optional "prevent sleep during sync" setting.
   - Diagnostics export: a redacted support bundle (versions, configs,
     recent logs, system check, monitor history) that never contains
     secrets or sensitive-channel output.
   - Accessibility: environment colors always paired with text labels
     (colorblind users), full keyboard navigation, screen-reader labels.

9. Settings and maintenance:
   - Bitcoin Core updates: changelog and full verification (pinned
     builder keys, so new Core releases can be verified without a
     Nodekeeper update). Updates apply to all environments together.
   - ord updates: because ord binaries are verified against hashes pinned
     inside Nodekeeper, a new ord version becomes installable only after a
     Nodekeeper update adds its hash. The update checker says so plainly
     ("ord 0.X is available; it will be installable in the next
     Nodekeeper update"). Warn when an ord upgrade requires a full
     reindex. Updates apply to all environments together.
   - Nodekeeper self-updates: signed updates via the Tauri updater, hosted
     on GitHub Releases. The updater's private signing key is held by me,
     the project owner, stored only in CI secrets, and never committed to
     the repository. Update checks happen only when the user clicks "Check
     for updates" or has opted in to automatic checks.
   - Backups: config files and PUBLIC wallet descriptors only by default.
     Private descriptors (listdescriptors true) are as sensitive as the
     seed: include them only in an optional encrypted backup protected by
     a separate backup password (Argon2id + authenticated encryption, as
     in Foundation E), with a clear warning. Never include the seed.
   - Tor: Nodekeeper does not bundle Tor. The Tor toggle detects a running
     system Tor daemon (control port / SOCKS port), configures Bitcoin Core
     to use it, and otherwise guides the user through installing Tor for
     their OS. Record this decision in DECISIONS.md.
   - Resource limits (dbcache, max connections), per-environment ports.
   - Remote mode: works ONLY through an SSH tunnel that forwards the
     remote node's RPC port and ord server port to 127.0.0.1. The executor
     and ord CLI then talk to the tunneled local ports, so the
     "localhost only" rule still holds. Explain this model in the UI.
   - Dark / light theme, language selection.

10. MULTI-ENVIRONMENT UI:
   - Environment switcher in the top bar showing every environment with a
     status badge. Switching only changes which environment the UI shows;
     it never stops the others.
   - "All environments" overview: status side by side, start/stop per
     environment, "stop all".
   - Every environment has a distinct color, a text label, and a permanent
     banner on every screen (e.g. orange MAINNET, purple REGTEST). All
     confirmation dialogs repeat the environment name in large text.
   - Show combined RAM and disk use across running environments; warn when
     several heavy environments at once exceed available resources.
     Regtest can always run alongside mainnet.

11. REGTEST TEST LAB (safe practice and development mode):
   - Runs bitcoind with -regtest and ord with its regtest flag in the
     regtest environment's own data directories, alongside any other
     running environment, with all ord index options enabled.
   - Banner on every screen: "REGTEST - test coins, no real value".
   - One-click setup: start bitcoind and ord server, create a test wallet,
     and mine 101 blocks to it so its coins are spendable (coinbase
     outputs need 100 confirmations).
   - "Mine blocks" button with a number field (default 1), using
     generatetoaddress. After any send or inscribe, offer "Mine 1 block to
     confirm", plus an optional auto-mine toggle.
   - "Get test coins" button that mines blocks to the current wallet.
   - Guided walkthroughs with checkpoints, with the Live Command Monitor
     opened automatically:
     a) create wallet -> receive -> mine -> check balance
     b) inscribe a file -> mine -> see it in the gallery
     c) reinscribe that inscription -> mine -> see both on the sat
     d) send an inscription to a second test wallet -> mine -> confirm it
        arrived
     e) run a console command and an example script
   - "Reset Test Lab": stop regtest services gracefully, delete only the
     regtest data directories after confirmation, start fresh.
   - If the installed ord version has a built-in regtest environment
     command (e.g. `ord env`) (VERIFY), it may be used internally, but the
     app's own controls must still work.

12. PORTABLE MODE (external SSD, plug-and-play across PCs):
   - The app, binaries, data, and config all live on one external drive:
       /Start-Windows.exe, /Start-Mac.app (universal: Intel + Apple
       Silicon), /Start-Linux.AppImage
       /bin/windows, /bin/macos, /bin/linux
       /runtime/windows (WebView2 fixed-version runtime)
       /data/<environment>/bitcoin, /data/<environment>/ord
       /config (settings, scripts, templates, SQLite database, encrypted
       secrets file)
   - All paths are relative to the drive root; never store absolute paths
     or drive letters.
   - Master password at launch unlocks the encrypted secrets file
     (Foundation E).
   - Webview prerequisites (the app cannot explain a missing webview using
     the webview itself):
     - Windows: bundle the WebView2 fixed-version runtime on the drive so
       nothing needs installing. The installed-mode installer embeds the
       WebView2 bootstrapper.
     - Linux: VERIFY whether the AppImage bundles WebKitGTK. If it
       doesn't, and it's missing, the launcher shows a native OS dialog
       (not a webview) with install instructions.
     - Any prerequisite failure before the webview loads is reported via a
       native dialog.
   - macOS App Translocation: an unsigned .app launched from a quarantined
     location runs from a hidden, randomized read-only path, which breaks
     relative paths. Detect this at launch and guide the user to remove
     the quarantine attribute (xattr -dr com.apple.quarantine) or move/
     reopen the app correctly. Also check and handle the quarantine
     attribute on the bitcoind and ord binaries.
   - Linux noexec: detect when the drive is mounted noexec (the AppImage
     and binaries won't run) and explain how to remount it.
   - Filesystem checks: warn if exFAT (corruption risk on unplug, no
     permission bits, macOS writes ._ metadata files; ignore ._ files
     everywhere); recommend NTFS if the user only uses Windows and Linux.
   - Windows long paths are supported (Foundation A).
   - Detect USB speed if possible, and free space.
   - Unclean-shutdown recovery with clear guidance if an ord index needs
     rebuilding.
   - Prominent "Safely shut down and eject" button: stop ALL running
     environments via the process manager (ord first, then bitcoind), wait
     for clean exits, then say it's safe to unplug. Warn if the window is
     closed while services are running.
   - Services run only while the app is open.
   - Remind the user that the seed phrase is the only recovery if the
     drive is lost, and link to optional VeraCrypt full-drive encryption
     guidance.
   - Binaries for all OSes stay at the same version so ord indexes remain
     compatible.
   - "Prepare a new portable drive" wizard: formatting guidance, copy the
     launchers, binaries, and WebView2 runtime, initialize the folder
     structure.
   - The Test Lab, multi-environment support, and Live Command Monitor
     must work fully in portable mode.

======================================================================
SECURITY RULES (mandatory)
======================================================================
- RPC and ord server bind to 127.0.0.1 only. ord server may bind to all
  interfaces by default (VERIFY), so always pass its address flag
  explicitly.
- Cookie auth; secrets stored per Foundation E.
- Never log, store, or transmit seed phrases or private keys; mnemonics
  use the sensitive output channel only.
- Wallet encryption passphrases are never persisted; mnemonics and
  passphrases held in memory are zeroized after use.
- Backups contain only public descriptors unless the user explicitly
  creates an encrypted private backup.
- All commands go through the central command executor; secrets passed
  via stdin/RPC, never command-line arguments.
- Inscription content is sandboxed per Foundation D, with a CSP listing
  only the exact ord server origins.
- All mainnet wallets are encrypted.
- Verify all binaries (pinned keys and hashes, taken only from official
  sources and checked by me); fail closed.
- Every fund-moving action has a preview (dry run or PSBT) and explicit
  confirmation; mainnet requires an extra confirmation step from the
  first phase that moves funds; Core spend commands against ord wallets
  are blocked by default.
- Environments are fully isolated: no command, script, wallet action, or
  template may cross from one environment to another.
- No telemetry. Network calls only to: Bitcoin P2P, bitcoincore.org and
  the official ord GitHub releases (downloads), Nodekeeper's GitHub
  Releases update endpoint (user-initiated or opted-in), the local Tor
  daemon when enabled, and local services.
- Scripts are trusted code; the runner enforces environment restrictions.
- When a VERIFY result conflicts with any of these rules: STOP AND ASK.

======================================================================
DESIGN
======================================================================
- Modern, calm, dark-mode-first, with a Bitcoin-orange accent for mainnet
  and distinct colors for other environments, always with text labels.
- Clear status badges and progress indicators everywhere.
- Plain-language explanations with an optional "show technical details"
  toggle on every screen.
- The Live Command Monitor uses a monospace font and terminal-style
  styling that matches the app theme.

======================================================================
IMPLEMENTATION GUIDE (how to work as a coding agent)
======================================================================

Code structure
- Cargo workspace with focused crates; business logic lives in crates,
  not in Tauri command handlers, so it is testable without the GUI:
    crates/nk-core     environment model, path resolution, config generation
    crates/nk-exec     central command executor, redaction, sensitive channel
    crates/nk-proc     process manager, PID/lock files, graceful stop
    crates/nk-verify   downloads, SHA-256, PGP verification, pinned values
    crates/nk-rpc      Bitcoin Core JSON-RPC client (calls go through nk-exec)
    crates/nk-ord      ord CLI and ord server API wrappers
    crates/nk-secrets  keychain, encrypted secrets file, zeroization
    crates/nk-store    SQLite and migrations
    crates/nk-testkit  regtest fixture for integration tests
    src-tauri/         thin Tauri command layer only
    ui/                React frontend
- Enforce the executor rule mechanically: configure clippy
  disallowed-methods (or an equivalent CI check) so that
  std::process::Command and tokio::process::Command can only be used
  inside nk-exec and nk-proc. The build fails otherwise.
- Typed IPC: generate TypeScript types from the Rust types (e.g.
  tauri-specta or ts-rs). Never hand-write duplicate types.
- Errors: every user-facing error is a typed code (e.g. PORT_IN_USE,
  ORD_NOT_SYNCED) mapped to an i18n message and a "What to do" action.
  Raw subprocess output is only ever shown as technical details.
- Suggested libraries (confirm current versions and record them in
  DECISIONS.md): tokio, serde, thiserror, tracing, reqwest with rustls,
  sha2, sequoia-openpgp (pure-Rust backend), argon2, chacha20poly1305,
  zeroize and secrecy, keyring, rusqlite with a migration tool,
  tauri-plugin-updater. Frontend: Vite, TanStack Query, Zustand,
  react-i18next, Vitest, Testing Library.

UI building blocks (build before feature screens)
- Design tokens: per-environment colors, spacing, typography, dark and
  light themes.
- Shared components that every screen must use: EnvBanner, StatusBadge,
  ErrorPanel, SensitiveSeedView, and a single ConfirmDialog that always
  shows the environment name, applies the mainnet extra step, and
  supports Learn mode. No screen may implement its own confirmation
  flow, so the mainnet step cannot be skipped.

Regtest test kit
- nk-testkit starts bitcoind and ord in a temporary directory on random
  free ports, can mine blocks, and always tears down (even on panic or
  test failure). CI caches the verified binaries.
- Every feature that touches Bitcoin Core or ord gets at least one
  integration test using nk-testkit.

Quality gate
- One command (e.g. `just check`) runs: cargo fmt --check, cargo clippy
  with -D warnings, cargo test, TypeScript type check, ESLint, and
  Vitest. Run it before every commit and before claiming a task is done.
- Security regression tests that must always pass:
  - redaction of every secret type;
  - a known fake mnemonic and passphrase pushed through the create,
    restore, and unlock flows never appear in logs, the database, monitor
    records, exports, or support bundles (search for each word);
  - a malicious test inscription cannot reach Tauri IPC;
  - the disallowed-methods check.

Working rhythm
- Before starting a phase, split it into small tasks in PROGRESS.md.
- Work one task at a time: implement -> test -> quality gate -> commit
  (clear conventional commit message) -> tick it in PROGRESS.md.
- Security self-review at the end of Phases 2, 5, and 7: go through the
  SECURITY RULES line by line, point to the code and test that enforce
  each rule, and list any gaps.

======================================================================
RELEASE SCOPE
======================================================================
MVP (end of Phase 7): mainnet + regtest running side by side, with the
first-run disclaimer, setup wizard, node and ord management, dashboard,
wallet (with mainnet confirmations), inscribe studio including reinscribe,
explorer, console and scripts (no scheduling), and the Live Command
Monitor, installed mode only. Runes are view-only.
Later: full Test Lab walkthroughs, portable mode, signet/testnet4,
background services, remote mode, Tor, script scheduling, tray and
notifications, diagnostics bundle, self-updates.
Not planned unless I ask: rune send, mint, or etch.

======================================================================
PHASES AND ACCEPTANCE CRITERIA
======================================================================
Build one phase at a time. At the end of each phase: run the tests, check
off the acceptance criteria in PROGRESS.md, summarize what was built, list
any VERIFY results and decisions, give me the [MANUAL] test checklist,
and wait for my approval.

The setup wizard grows phase by phase (system check and disclaimer in
Phase 1, Bitcoin Core in Phase 2, ord and index options in Phase 4).
Until Phase 8, "try it safely" opens the minimal regtest start from
Phase 2.

Phase 0 - Feasibility spike (no app code)
  Build: save this spec as docs/SPEC.md; create CLAUDE.md, ARCHITECTURE.md
  (initial draft), DECISIONS.md, and PROGRESS.md. In a throwaway spikes/
  folder, download the current Bitcoin Core and ord, run them on regtest,
  and answer every VERIFY item that could change the architecture:
  - do ord wallet commands work with an encrypted Core wallet, and how
    is it unlocked?
  - does ord shut down cleanly on SIGINT, and on Windows CTRL_BREAK_EVENT
    (if no Windows machine is available, write a CI job that tests it)?
  - which ord commands accept secrets via stdin?
  - ord server's default bind address and the flag to change it
  - does `ord wallet send --dry-run` exist, and what does it output?
  - reinscribe syntax, batch reinscribe support
  - which features need which index option (Foundation F)
  - per-chain data paths for Bitcoin Core and ord
  - testnet4 support; whether ord releases publish checksums
  - whether a built-in regtest command (e.g. `ord env`) exists
  Record each answer in DECISIONS.md with the exact commands run and the
  relevant output. Present a summary, ask me the code-signing question,
  and STOP AND ASK about any conflict with the spec.
  Done when:
  - every VERIFY item above has an answer or is explicitly marked
    "needs CI/Windows"
  - [MANUAL] I have read DECISIONS.md, answered the code-signing
    question, and approved

Phase 1 - Foundation
  Build: Cargo workspace and crate skeletons, quality gate command,
  disallowed-methods check, CI matrix, UI shell and navigation, design
  tokens and shared components (EnvBanner, StatusBadge, ErrorPanel,
  ConfirmDialog skeleton), i18n setup, SQLite with migrations,
  environment model and per-chain path resolution (with Windows long
  paths), minimal environment switcher, single-instance lock, secrets
  storage (Foundation E), first-run disclaimer, system check.
  Done when:
  - [CI] app builds and launches on all 3 OSes; the quality gate passes
  - [CI] a deliberate process spawn outside nk-exec/nk-proc fails the
    build (verified once, then removed)
  - [CI] path resolution tests pass for all chains, including a deeply
    nested Windows path
  - [CI] a second app instance on the same data folder is refused
  - [CI] encrypted secrets file round-trips; wrong master password fails
  - [MANUAL] banner shows the current environment; disclaimer appears on
    first run only

Phase 2 - Process manager, executor, installer
  Build: process manager (Foundation C), central command executor with
  redaction, sensitive channel, and zeroization (Foundation B),
  nk-testkit, Bitcoin Core download and verification, bitcoin.conf
  generation (prune=0, txindex=1), start/stop, minimal regtest start and
  block mining.
  Done when:
  - [CI] real Bitcoin Core binaries download and verify with 3+ pinned
    signatures; a tampered file is rejected
  - [CI] regtest bitcoind starts, mines 101 blocks, and stops cleanly
  - [CI] redaction unit tests pass; a pre-existing bitcoind is detected
  - [MANUAL] mainnet bitcoind starts, connects to peers, and stops
    cleanly (no full sync required)
  - [MANUAL] I have checked every pinned builder-key fingerprint against
    bitcoin-core/guix.sigs from a separate machine or browser
  - Security self-review completed

Phase 3 - Dashboard and monitor
  Build: RPC client, status polling, log viewer with tailing, disk
  monitor, plain-language errors, Live Command Monitor.
  Done when:
  - [CI] every command from Phase 2 is recorded by the monitor backend;
    secrets are redacted in records and exports
  - [CI] a busy port produces the friendly error
  - [MANUAL] a large debug.log opens instantly; show/hide and pop-out
    work

Phase 4 - ord integration
  Build: ord download and verification (pinned hashes), ord server per
  environment bound to 127.0.0.1, graceful stop on all 3 OSes,
  wait-for-sync logic, index options and feature gating (Foundation F),
  index dashboard.
  Done when:
  - [CI] ord verifies; an unpinned version is refused
  - [CI] ord indexes regtest with all index options and stays caught up
  - [CI] ord stops gracefully on Windows, macOS, and Linux and restarts
    without reindexing
  - [MANUAL] ord server is not reachable from another machine on the LAN
  - [MANUAL] I have checked every pinned ord SHA-256 hash against the
    official ord release from a separate machine or browser
  - VERIFY results for Foundation F recorded in DECISIONS.md

Phase 5 - Wallet
  Build: Foundation D security model and CSP, SensitiveSeedView, wallet
  create/restore via the sensitive channel, mainnet wallet encryption
  with per-action passphrase entry and optional in-memory session
  remember, balance, view-only runes, receive, gallery, send with address
  checks, fee fallback, fee guard, dry run, and the MAINNET extra
  confirmation through the shared ConfirmDialog.
  Done when:
  - [CI] the fake-mnemonic and fake-passphrase search test passes: they
    never appear in monitor records, logs, the database, the keychain,
    the secrets file, or exports
  - [CI] a malicious test HTML/SVG inscription cannot call Tauri IPC or
    read app data
  - [CI] wrong-network addresses are rejected; an inscription send
    completes on regtest
  - [MANUAL] mainnet confirmation appears for a mainnet send (cancel it
    before broadcasting); encrypted wallet unlock/lock works; the session
    remember clears after the idle timeout
  - Encryption compatibility with ord VERIFIED, or I have decided how to
    proceed
  - Security self-review completed

Phase 6 - Inscribe studio
  Build: single and batch inscribe, reinscribe mode, mainnet confirmation.
  Done when:
  - [CI] inscribe and reinscribe both work on regtest; the sat shows both
    inscriptions in order
  - [MANUAL] the reinscribe checkbox is enforced; gated features explain
    missing index options

Phase 7 - Console, scripts, explorer (MVP complete)
  Build: console with autocomplete and safety layer, templates, script
  runner, example scripts, explorer.
  Done when:
  - [CI] sendtoaddress against an ord wallet is blocked
  - [CI] a "regtest only" script refuses to run on mainnet
  - [CI] the 3 example scripts run on regtest
  - [MANUAL] mainnet and regtest run side by side, and switching never
    stops either
  - Security self-review completed

Phase 8 - Multi-environment UI and Test Lab
  Build: full switcher, overview page, resource warnings, Test Lab with
  guided walkthroughs, notifications, tray, prevent-sleep. "Try it
  safely" in the setup wizard now opens the full Test Lab.
  Done when:
  - [CI] Reset Test Lab deletes only regtest data
  - [MANUAL] all 5 walkthroughs complete on a fresh regtest

Phase 9 - Portable mode
  Build: portable launchers and layout, bundled WebView2 runtime, native
  prerequisite dialogs, translocation/noexec/filesystem checks, master
  password unlock, safe eject, drive-preparation wizard.
  Done when:
  - [CI] portable build artifacts are produced for all 3 OSes
  - [MANUAL] one drive syncs regtest on one OS and continues on the other
    two without resyncing
  - [MANUAL] translocation and noexec are detected with guidance; a
    missing webview shows a native dialog
  - [MANUAL] safe eject stops every environment cleanly

Phase 10 - Extras and release
  Build: signet/testnet4 (if supported), background services, remote mode
  via SSH tunnel, Tor (system daemon), script scheduling, diagnostics
  bundle, Core/ord update checker, signed Nodekeeper self-updates via
  GitHub Releases, encrypted private-descriptor backups, packaging (.msi,
  .dmg, .AppImage / .deb), user documentation.
  Done when:
  - [CI] installers build; the support bundle contains no secrets
  - [CI] a signed test update installs and verifies; an unsigned one is
    rejected
  - [CI] a default backup contains no private descriptors; an encrypted
    private backup restores only with the correct backup password
  - [MANUAL] remote mode works through an SSH tunnel; Tor toggle works
    with a system Tor daemon

======================================================================
START
======================================================================
Start with Phase 0. Save this spec as docs/SPEC.md, create CLAUDE.md,
ARCHITECTURE.md, DECISIONS.md, and PROGRESS.md, run the feasibility
spike, and then present your findings together with the code-signing
question.
