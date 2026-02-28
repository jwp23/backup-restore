# backup-restore

Restores XDG user directories (Documents, Downloads, Music, Pictures, etc.) from a backup drive into your home directory. Handles nested backup layouts, parallel copying, conflict detection, and optional source cleanup.

# Human Notes
This project was built with claude code as an experiment to see if opus 4.6 was good enough to one shot a solution to real  need that I had with restoring backup from a previous linux distribution to my current one.

The prompt was a paragraph long which left most of the ui to be created by model and to keep the code human readable.

It wasn't quite a clean experiment since I had [obra's superpower](https://github.com/obra/superpowers) plugin installed at the user level. I think the only difference is that claude used TDD. None of the plugin's workflow was used.

## Results
* start to finish coding time: 49 minutes
* cli moved 22GB and about 3800 files in 0.1s.
* functionality was complete with only one addition of the dry run flag that I forgot to specify at the beginning. README.md was not automatically created.

## Feedback
I'm not a rust developer. I did a cursory look through the code and it seemed to be human readable. I'd like feedback from an experienced rust developer on the quality of the code created.

## Install

Requires [Rust](https://rustup.rs/).

```
cargo install --path .
```

Or build without installing:

```
cargo build --release
./target/release/backup-restore /mnt/backup
```

## Usage

```
backup-restore [OPTIONS] <BACKUP_DIR>
```

### Preview first

```
backup-restore -n /mnt/backup
```

The `--dry-run` (`-n`) flag scans the backup and shows what would happen — files to copy, byte totals, per-directory breakdown, and any conflicts — without writing anything.

### Run the restore

```
backup-restore /mnt/backup
```

The tool will:

1. Scan the backup for XDG directories (at any nesting depth)
2. Show detected mappings and ask for confirmation
3. Copy files in parallel, creating `.restore` variants for conflicts
4. Print a summary report
5. Offer interactive conflict resolution (overwrite, keep original, or leave both)
6. Optionally delete source files from the backup

### Options

| Flag | Description |
|------|-------------|
| `-n`, `--dry-run` | Preview without copying |
| `-j`, `--jobs N` | Parallel copy threads (default: 4) |
| `--home PATH` | Restore into a different home directory |

### Conflicts

When a destination file already exists, the restored version is written alongside it with a `.restore` suffix (e.g. `notes.restore.txt`). After copying, you choose how to resolve: overwrite all, keep all originals, decide per folder, or decide per file.

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
