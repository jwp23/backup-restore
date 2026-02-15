use std::collections::HashMap;
use std::fmt::Write;
use std::time::Duration;

use crate::error::{CopyResult, XdgDir};

/// Format a summary report of the copy operation.
pub fn format_report(result: &CopyResult, elapsed: Duration) -> String {
    let mut out = String::new();

    writeln!(out, "\n--- Restore Summary ---").unwrap();
    writeln!(
        out,
        "{} files copied, {} conflicts, {} errors",
        result.copied.len(),
        result.conflicts.len(),
        result.errors.len()
    )
    .unwrap();
    writeln!(out, "Total: {} in {:.1}s", format_bytes(result.bytes_copied), elapsed.as_secs_f64()).unwrap();

    // Per-XDG breakdown
    let mut by_dir: HashMap<XdgDir, (usize, usize, usize)> = HashMap::new();
    for f in &result.copied {
        by_dir.entry(f.xdg_dir).or_default().0 += 1;
    }
    for c in &result.conflicts {
        by_dir.entry(c.xdg_dir).or_default().1 += 1;
    }
    for e in &result.errors {
        by_dir.entry(e.xdg_dir).or_default().2 += 1;
    }

    if !by_dir.is_empty() {
        writeln!(out, "\nPer directory:").unwrap();
        let mut dirs: Vec<_> = by_dir.into_iter().collect();
        dirs.sort_by_key(|(d, _)| d.dir_name());
        for (dir, (copied, conflicts, errors)) in dirs {
            writeln!(
                out,
                "  {:<12} {} copied, {} conflicts, {} errors",
                dir, copied, conflicts, errors
            )
            .unwrap();
        }
    }

    // List errors
    if !result.errors.is_empty() {
        writeln!(out, "\nErrors:").unwrap();
        for err in &result.errors {
            writeln!(out, "  {}", err).unwrap();
        }
    }

    // List conflicts (abbreviated if many)
    if !result.conflicts.is_empty() {
        writeln!(out, "\nConflicts:").unwrap();
        if result.conflicts.len() <= 10 {
            for c in &result.conflicts {
                writeln!(out, "  {} → {}", c.original_path.display(), c.restore_path.display()).unwrap();
            }
        } else {
            for c in result.conflicts.iter().take(5) {
                writeln!(out, "  {} → {}", c.original_path.display(), c.restore_path.display()).unwrap();
            }
            writeln!(out, "  ... and {} more", result.conflicts.len() - 5).unwrap();
        }
    }

    out
}

fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;

    if bytes >= GB {
        format!("{:.1} GiB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MiB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KiB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::{Conflict, CopiedFile, CopyError, XdgDir};
    use std::path::PathBuf;
    use std::time::Duration;

    #[test]
    fn report_shows_basic_stats() {
        let result = CopyResult {
            copied: vec![
                CopiedFile {
                    source: PathBuf::from("/backup/Documents/a.txt"),
                    dest: PathBuf::from("/home/joe/Documents/a.txt"),
                    size: 100,
                    xdg_dir: XdgDir::Documents,
                },
                CopiedFile {
                    source: PathBuf::from("/backup/Documents/b.txt"),
                    dest: PathBuf::from("/home/joe/Documents/b.txt"),
                    size: 200,
                    xdg_dir: XdgDir::Documents,
                },
            ],
            conflicts: vec![],
            errors: vec![],
            bytes_copied: 300,
        };

        let report = format_report(&result, Duration::from_secs(2));

        assert!(report.contains("2 files copied"));
        assert!(report.contains("300 B"));
        assert!(report.contains("0 conflicts"));
        assert!(report.contains("0 errors"));
    }

    #[test]
    fn report_shows_conflicts_and_errors() {
        let result = CopyResult {
            copied: vec![],
            conflicts: vec![
                Conflict {
                    restore_path: PathBuf::from("/home/joe/Documents/a.restore.txt"),
                    original_path: PathBuf::from("/home/joe/Documents/a.txt"),
                    size: 50,
                    xdg_dir: XdgDir::Documents,
                },
            ],
            errors: vec![CopyError {
                source: PathBuf::from("/backup/Music/bad.mp3"),
                dest: PathBuf::from("/home/joe/Music/bad.mp3"),
                error: std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied"),
                xdg_dir: XdgDir::Music,
            }],
            bytes_copied: 50,
        };

        let report = format_report(&result, Duration::from_secs(1));

        assert!(report.contains("1 conflicts"));
        assert!(report.contains("1 errors"));
        assert!(report.contains("a.txt"));
        assert!(report.contains("a.restore.txt"));
        assert!(report.contains("bad.mp3"));
    }

    #[test]
    fn report_shows_per_directory_breakdown() {
        let result = CopyResult {
            copied: vec![
                CopiedFile {
                    source: PathBuf::from("/backup/Documents/a.txt"),
                    dest: PathBuf::from("/home/joe/Documents/a.txt"),
                    size: 100,
                    xdg_dir: XdgDir::Documents,
                },
                CopiedFile {
                    source: PathBuf::from("/backup/Music/b.mp3"),
                    dest: PathBuf::from("/home/joe/Music/b.mp3"),
                    size: 200,
                    xdg_dir: XdgDir::Music,
                },
            ],
            conflicts: vec![],
            errors: vec![],
            bytes_copied: 300,
        };

        let report = format_report(&result, Duration::from_secs(1));

        assert!(report.contains("Documents"));
        assert!(report.contains("Music"));
    }

    #[test]
    fn report_abbreviates_many_conflicts() {
        let conflicts: Vec<Conflict> = (0..15)
            .map(|i| Conflict {
                restore_path: PathBuf::from(format!("/home/joe/Documents/file{i}.restore.txt")),
                original_path: PathBuf::from(format!("/home/joe/Documents/file{i}.txt")),
                size: 10,
                xdg_dir: XdgDir::Documents,
            })
            .collect();

        let result = CopyResult {
            copied: vec![],
            conflicts,
            errors: vec![],
            bytes_copied: 150,
        };

        let report = format_report(&result, Duration::from_secs(1));

        assert!(report.contains("... and 10 more"));
    }

    #[test]
    fn format_bytes_uses_correct_units() {
        assert_eq!(format_bytes(500), "500 B");
        assert_eq!(format_bytes(1024), "1.0 KiB");
        assert_eq!(format_bytes(1024 * 1024), "1.0 MiB");
        assert_eq!(format_bytes(1024 * 1024 * 1024), "1.0 GiB");
    }
}
