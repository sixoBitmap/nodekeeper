# Quality gate: run before every commit and before claiming a task done.
# See CLAUDE.md and docs/SPEC.md "Quality gate".

# The real-binary tests (wallet, restore, inscribe, process handling, the
# console's secret handling against a real node) need the verified Bitcoin Core
# and ord binaries. They used to skip quietly when NK_TEST_BITCOIND / NK_TEST_ORD
# were unset, so this gate could be green while they did nothing. Now the gate
# sets them, and NK_REQUIRE_LIVE=1 turns a missing binary into a FAILURE.
# Defaults: the verified copies the verification tests cache under target/
# (override either variable to test other binaries).
exe := if os_family() == "windows" { ".exe" } else { "" }
export NK_REQUIRE_LIVE := "1"
export NK_TEST_BITCOIND := env_var_or_default("NK_TEST_BITCOIND", justfile_directory() + "/target/nodekeeper-bitcoin-core-31.1/extracted/bitcoin-31.1/bin/bitcoind" + exe)
export NK_TEST_ORD := env_var_or_default("NK_TEST_ORD", justfile_directory() + "/target/nodekeeper-ord-0.29.0/extracted/ord-0.29.0/ord" + exe)

check: fmt-check clippy test-rust check-ui

fmt-check:
    cargo fmt --all -- --check

clippy:
    cargo clippy --workspace --all-targets -- -D warnings -W clippy::disallowed-methods

test-rust:
    cargo test --workspace

# Quick run WITHOUT the real-binary tests (they are skipped): not the gate.
test-rust-quick:
    NK_REQUIRE_LIVE=0 cargo test --workspace

check-ui:
    cd ui && npm run typecheck
    cd ui && npm run lint
    cd ui && npm run test -- --run

fmt:
    cargo fmt --all

# Launches the app in dev mode (hot reload). Must run from the repo root
# (not ui/): the Tauri CLI only looks for src-tauri/ in subfolders of the
# current directory, and src-tauri lives at the repo root, not inside ui/
# -- `npm run tauri` from ui/ can't find it. Invoking the binary directly
# with the repo root as cwd sidesteps that.
dev:
    ./ui/node_modules/.bin/tauri dev

# Builds a debug binary without bundling installers (target/debug/nodekeeper[.exe]).
build-app:
    ./ui/node_modules/.bin/tauri build --debug --no-bundle
