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
