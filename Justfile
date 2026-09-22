# Quality gate: run before every commit and before claiming a task done.
# See CLAUDE.md and docs/SPEC.md "Quality gate".

check: fmt-check clippy test-rust check-ui

fmt-check:
    cargo fmt --all -- --check

clippy:
    cargo clippy --workspace --all-targets -- -D warnings -W clippy::disallowed-methods

test-rust:
    cargo test --workspace

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
