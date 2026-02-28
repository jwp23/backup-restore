# Code Quality Tooling Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Audit the codebase for quality/complexity, then enforce standards via pre-commit hooks and GitHub Actions CI.

**Architecture:** Four phases — audit first (discover issues), fix issues and configure lints, add local enforcement (pre-commit hooks), add remote enforcement (CI). Each phase builds on the previous.

**Tech Stack:** cargo clippy, cargo fmt, rust-code-analysis-cli, cargo-audit, cargo-deny, GitHub Actions

**Working directory:** `/home/mordant23/workspace/jwp23/backup-restore/.worktrees/code-quality-tooling`

---

## Task 1: Install Audit Tools

**Files:** None (system-level installs)

**Step 1: Install cargo-audit, cargo-deny, and rust-code-analysis-cli**

Run:
```bash
cargo install cargo-audit cargo-deny rust-code-analysis-cli
```

**Step 2: Verify installations**

Run:
```bash
cargo audit --version && cargo deny --version && rust-code-analysis-cli --version
```

Expected: Version strings printed for all three tools.

---

## Task 2: Run Formatting Audit and Fix

**Files:**
- Modify: all `src/*.rs` files and `tests/integration.rs` (if formatting diffs exist)

**Step 1: Check formatting**

Run:
```bash
cargo fmt --check
```

Expected: Either clean (no output, exit 0) or diffs shown (exit 1).

**Step 2: Apply formatting fixes (if needed)**

Run:
```bash
cargo fmt
```

**Step 3: Verify tests still pass**

Run:
```bash
cargo test
```

Expected: All 34 tests pass.

**Step 4: Commit (if changes were made)**

```bash
git add -A && git diff --cached --stat
git commit -m "style: apply rustfmt default formatting"
```

Only commit if there were actual changes. Check `git diff --cached --stat` before committing.

---

## Task 3: Run Clippy Audit (Pedantic)

**Files:**
- Create: `docs/audit/2026-02-27-clippy.md` (temporary notes, folded into final audit)

**Step 1: Run clippy with pedantic warnings**

Run:
```bash
cargo clippy -- -W clippy::pedantic 2>&1 | tee /tmp/clippy-audit.txt
```

**Step 2: Catalog the warnings**

The current warnings are (for reference — re-run to confirm):

| Lint | Count | Fixable? |
|---|---|---|
| `cast_precision_loss` | 6 | Allow — intentional f64 display of byte counts |
| `doc_markdown` | 5 | Fix — add backticks to doc items |
| `uninlined_format_args` | 4 | Fix — use `cargo clippy --fix` |
| `must_use_candidate` | 6 | Allow — adding `#[must_use]` everywhere is noisy |
| `missing_errors_doc` | 3 | Allow — errors are self-evident from signatures |
| `missing_panics_doc` | 2 | Allow — panics are in test helpers only |
| `needless_continue` | 1 | Fix — remove redundant continue |
| `manual_let_else` | 1 | Fix — rewrite as let...else |

Save these findings — they inform Task 5.

**Step 3: No commit** (this is just information gathering)

---

## Task 4: Run Complexity and Dependency Audits

**Files:**
- Create: `docs/audit/2026-02-27-audit.md`

**Step 1: Run rust-code-analysis-cli**

Run:
```bash
rust-code-analysis-cli -m -O json -p src/ 2>/dev/null | python3 -c "
import json, sys
for line in sys.stdin:
    data = json.loads(line)
    name = data.get('name', 'unknown')
    spaces = data.get('spaces', [])
    for space in spaces:
        kind = space.get('kind', '')
        if kind == 'function':
            fn_name = space.get('name', '?')
            metrics = space.get('metrics', {})
            cc = metrics.get('cyclomatic', {}).get('sum', 0)
            cog = metrics.get('cognitive', {}).get('sum', 0)
            sloc = metrics.get('loc', {}).get('sloc', 0)
            if cc >= 5 or cog >= 5:
                print(f'{name}::{fn_name}  cyclomatic={cc}  cognitive={cog}  sloc={sloc}')
" 2>/dev/null || echo "Parse output manually if python script fails"
```

If the python parsing fails, just run `rust-code-analysis-cli -m -p src/` and review raw output. Look for functions with cyclomatic complexity >= 10 or cognitive complexity >= 10.

**Step 2: Run cargo audit**

Run:
```bash
cargo audit
```

Expected: Either "0 vulnerabilities found" or a list of advisories.

**Step 3: Run cargo deny (without config — just to see what it finds)**

Run:
```bash
cargo deny check 2>&1 || true
```

This will likely fail without a `deny.toml` — that's fine. We're gathering info about what licenses and duplicates exist.

**Step 4: Write the audit report**

Create `docs/audit/2026-02-27-audit.md` with four sections:
1. **Formatting** — Clean or fixed (reference Task 2)
2. **Clippy (pedantic)** — Table of warnings from Task 3
3. **Complexity** — Table of high-complexity functions from Step 1
4. **Dependencies** — Audit results from Steps 2-3, license summary

List the dependency licenses found:
- `MIT`
- `Apache-2.0 OR MIT`
- `MIT OR Apache-2.0`
- `Unlicense OR MIT`
- `Zlib`
- `(MIT OR Apache-2.0) AND Unicode-3.0`
- `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT`
- `MIT OR Apache-2.0 OR LGPL-2.1-or-later`

**Step 5: Commit**

```bash
git add docs/audit/2026-02-27-audit.md
git commit -m "docs: add initial code quality audit results"
```

---

## Task 5: Configure Clippy Lints in Cargo.toml

**Files:**
- Modify: `Cargo.toml`

**Step 1: Add lint configuration**

Add this section to the end of `Cargo.toml`:

```toml
[lints.clippy]
pedantic = { level = "warn", priority = -1 }
# Intentional: byte counts displayed as f64 for human-readable formatting
cast_precision_loss = "allow"
# Noisy without value for a CLI tool
must_use_candidate = "allow"
# Error types are self-evident from Result signatures
missing_errors_doc = "allow"
# Panics are only in test helpers, not public API
missing_panics_doc = "allow"
```

Note: adjust this list based on actual audit findings from Task 3. If additional noisy lints surfaced, allow them here with a comment explaining why.

**Step 2: Fix the remaining warnings**

The following lints should be *fixed*, not allowed:

- `uninlined_format_args` — run `cargo clippy --fix --allow-dirty -- -W clippy::uninlined_format_args`
- `doc_markdown` — manually add backticks around code items in doc comments
- `needless_continue` — remove the redundant `continue` statement
- `manual_let_else` — rewrite as `let ... else { ... }`

After auto-fix, review changes and fix anything the auto-fix missed.

**Step 3: Verify clean clippy**

Run:
```bash
cargo clippy -- -D warnings
```

Expected: No warnings (exit 0).

**Step 4: Verify tests still pass**

Run:
```bash
cargo test
```

Expected: All 34 tests pass.

**Step 5: Commit**

```bash
git add Cargo.toml src/
git commit -m "Configure clippy pedantic lints and fix warnings

Enable pedantic lint group with targeted allowances for
cast_precision_loss, must_use_candidate, missing_errors_doc,
and missing_panics_doc. Fix all other pedantic warnings."
```

---

## Task 6: Create deny.toml

**Files:**
- Create: `deny.toml`

**Step 1: Create deny.toml**

```toml
[advisories]
vulnerability = "deny"
unmaintained = "warn"
yanked = "warn"
notice = "warn"

[licenses]
unlicensed = "deny"
allow = [
    "MIT",
    "Apache-2.0",
    "Zlib",
    "Unicode-3.0",
    "Unlicense",
    "LGPL-2.1-or-later",
]
confidence-threshold = 0.8

[bans]
multiple-versions = "warn"
wildcards = "allow"

[sources]
unknown-registry = "warn"
unknown-git = "warn"
allow-registry = ["https://github.com/rust-lang/crates.io-index"]
allow-git = []
```

**Step 2: Verify cargo deny passes**

Run:
```bash
cargo deny check
```

Expected: Passes (possibly with warnings for duplicate versions, which is fine).

If it fails on a license not in the allow list, add the missing license to the allow list.

**Step 3: Commit**

```bash
git add deny.toml
git commit -m "Add cargo-deny configuration for dependency auditing

Deny known vulnerabilities and unlicensed crates. Warn on
unmaintained, yanked, and duplicate dependencies."
```

---

## Task 7: Create Pre-commit Hook

**Files:**
- Create: `.githooks/pre-commit`
- Create: `setup.sh`

**Step 1: Create the hook directory and script**

Create `.githooks/pre-commit` with this content (and make it executable):

```bash
#!/usr/bin/env bash
set -e

echo "Running pre-commit checks..."

echo "  Checking formatting..."
cargo fmt --check
echo "  Formatting OK."

echo "  Running clippy..."
cargo clippy -- -D warnings
echo "  Clippy OK."

echo "  Running tests..."
cargo test --quiet
echo "  Tests OK."

echo "All pre-commit checks passed."
```

Run:
```bash
chmod +x .githooks/pre-commit
```

**Step 2: Create setup.sh**

Create `setup.sh` at the project root:

```bash
#!/usr/bin/env bash
set -e

git config core.hooksPath .githooks
echo "Git hooks configured. Pre-commit checks will run on each commit."
```

Run:
```bash
chmod +x setup.sh
```

**Step 3: Activate hooks in this worktree**

Run:
```bash
./setup.sh
```

Expected: "Git hooks configured..." message.

**Step 4: Test the hook works**

Make a trivial change (add a blank line to a file), stage it, and attempt a commit. The hook should run all three checks. Then reset the change.

Run:
```bash
echo "" >> src/lib.rs
git add src/lib.rs
git commit -m "test: verify pre-commit hook" 2>&1 || true
git checkout src/lib.rs
```

Expected: The hook runs fmt, clippy, and tests. It may pass or fail on the blank line — either way confirms the hook is active.

**Step 5: Commit**

```bash
git add .githooks/pre-commit setup.sh
git commit -m "Add pre-commit hook for fmt, clippy, and test checks

Hook runs cargo fmt --check, cargo clippy, and cargo test.
Run setup.sh once per clone to activate."
```

---

## Task 8: Create GitHub Actions CI Workflow

**Files:**
- Create: `.github/workflows/ci.yml`

**Step 1: Create the workflow file**

Create `.github/workflows/ci.yml`:

```yaml
name: CI

on:
  push:
    branches: [main]
  pull_request:
    branches: [main]

env:
  CARGO_TERM_COLOR: always

jobs:
  check:
    name: Quality checks
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt, clippy

      - uses: actions/cache@v4
        with:
          path: |
            ~/.cargo/bin/
            ~/.cargo/registry/index/
            ~/.cargo/registry/cache/
            ~/.cargo/git/db/
            target/
          key: ${{ runner.os }}-cargo-${{ hashFiles('**/Cargo.lock') }}

      - name: Check formatting
        run: cargo fmt --check

      - name: Run clippy
        run: cargo clippy -- -D warnings

      - name: Run tests
        run: cargo test

      - name: Install cargo-audit
        run: cargo install cargo-audit --locked
      - name: Run security audit
        run: cargo audit

      - name: Install cargo-deny
        run: cargo install cargo-deny --locked
      - name: Run dependency checks
        run: cargo deny check

      - name: Install rust-code-analysis-cli
        run: cargo install rust-code-analysis-cli --locked
        continue-on-error: true
      - name: Run complexity analysis
        run: rust-code-analysis-cli -m -p src/ || true
        continue-on-error: true
```

**Step 2: Commit**

```bash
mkdir -p .github/workflows
git add .github/workflows/ci.yml
git commit -m "Add GitHub Actions CI workflow

Runs fmt, clippy, tests, cargo-audit, and cargo-deny on push
and PR to main. Complexity analysis runs as informational only."
```

---

## Task 9: Update README

**Files:**
- Modify: `README.md`

**Step 1: Add development setup section**

Add the following after the "Conflicts" section at the end of `README.md`:

```markdown

## Development

### Setup

After cloning, run the setup script to enable pre-commit hooks:

```
./setup.sh
```

This configures git to run formatting, lint, and test checks before each commit.

### Quality checks

The pre-commit hook runs these automatically, but you can run them manually:

```
cargo fmt --check    # formatting
cargo clippy         # lints
cargo test           # tests
cargo audit          # dependency vulnerabilities
cargo deny check     # license and dependency policy
```
```

**Step 2: Verify the README renders correctly**

Read it back and check markdown formatting.

**Step 3: Commit**

```bash
git add README.md
git commit -m "Add development setup instructions to README"
```

---

## Task 10: Final Verification

**Files:** None

**Step 1: Run the full quality suite**

Run each check and confirm all pass:

```bash
cargo fmt --check && echo "fmt: OK"
cargo clippy -- -D warnings && echo "clippy: OK"
cargo test && echo "tests: OK"
cargo audit && echo "audit: OK"
cargo deny check && echo "deny: OK"
```

Expected: All five pass.

**Step 2: Verify pre-commit hook catches problems**

Introduce a deliberate formatting violation, try to commit, and verify the hook rejects it:

```bash
echo "fn ugly(){}" >> src/lib.rs
git add src/lib.rs
git commit -m "test: should be rejected" 2>&1; echo "Exit: $?"
git checkout src/lib.rs
```

Expected: Hook rejects the commit (non-zero exit). Then the checkout restores the file.

**Step 3: Review the commit log**

```bash
git log --oneline
```

Verify commits are clean and well-ordered.
