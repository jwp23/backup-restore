# backup-restore

Restores XDG user directories (Documents, Downloads, Music, Pictures, etc.) from a backup drive into your home directory. Handles nested backup layouts, parallel copying, conflict detection, and optional source cleanup.

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
