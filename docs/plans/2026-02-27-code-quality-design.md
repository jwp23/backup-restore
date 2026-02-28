# Code Quality and Complexity Tooling

## Goal

Audit the codebase for quality and complexity, then establish ongoing enforcement through pre-commit hooks and CI.

## Phase 1: Audit

Run four tools and capture results to `docs/audit/2026-02-27-audit.md`:

1. **`cargo clippy` (pedantic)** — Catalog all lint warnings with the `pedantic` group enabled.
2. **`cargo fmt --check`** — Identify formatting deviations from rustfmt defaults. Fix in a formatting-only commit.
3. **`rust-code-analysis-cli`** — Compute cyclomatic complexity, cognitive complexity, and SLOC per function. Flag hotspots.
4. **`cargo audit` + `cargo deny`** — Check dependencies for known vulnerabilities, license compliance, and duplicates.

## Phase 2: Pre-commit Hooks

A shell script at `.githooks/pre-commit`, activated via `git config core.hooksPath .githooks`.

Three checks, ordered by speed:

1. `cargo fmt --check` (~1s)
2. `cargo clippy -- -D warnings` (~5-10s incremental)
3. `cargo test` (~5-15s)

Commit rejected if any check fails. No network-dependent checks (audit/deny) in the hook.

A `setup.sh` script at the project root runs the `git config` command. Run once per clone.

## Phase 3: GitHub Actions CI

Single workflow at `.github/workflows/ci.yml`, triggered on push and PR to `main`.

Five gating steps:

1. `cargo fmt --check`
2. `cargo clippy -- -D warnings`
3. `cargo test`
4. `cargo audit`
5. `cargo deny check`

One informational (non-gating) step:

6. `rust-code-analysis-cli` — posts complexity metrics without blocking the build.

Uses `dtolnay/rust-toolchain@stable`, caches `~/.cargo` and `target/`.

## Phase 4: Project Configuration

### `Cargo.toml`

Add `[lints.clippy]` section: enable `pedantic` group, selectively allow noisy-but-unhelpful lints discovered during the audit.

### `deny.toml`

- License allowlist derived from actual dependency licenses
- Advisory database checks enabled
- Duplicate dependency detection as warnings

### No `rustfmt.toml`

Use rustfmt defaults. Most portable, least surprising.

## Files Added

| File | Purpose |
|---|---|
| `.githooks/pre-commit` | Pre-commit hook script |
| `.github/workflows/ci.yml` | CI pipeline |
| `deny.toml` | cargo-deny configuration |
| `setup.sh` | One-time hook setup |
| `docs/audit/2026-02-27-audit.md` | Audit results (one-time) |

## Files Modified

| File | Change |
|---|---|
| `Cargo.toml` | `[lints.clippy]` section |
| `README.md` | Setup instructions |

## What We're Not Adding

- **Coverage reporting** — Test ratio is already ~50% test code. Coverage tools in Rust CI are slow and flaky.
- **Nightly-only checks** — Stable toolchain only.
- **Release builds in CI** — Debug mode for speed.
- **`rustfmt.toml`** — Defaults are fine.
