use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::fs;

use indicatif::{ProgressBar, ProgressStyle};

use crate::types::{Conflict, CopiedFile, CopyError, CopyOp, CopyPlan, CopyResult};

/// Execute the copy plan, returning results with conflicts and errors.
///
/// Creates all directories first, then copies files in parallel across
/// `jobs` threads. If a destination file exists, writes to a `.restore`
/// suffixed path instead and records a conflict.
pub fn execute_plan(plan: &CopyPlan, jobs: usize) -> CopyResult {
    // Create all directories first
    for dir_op in &plan.dirs {
        let _ = fs::create_dir_all(&dir_op.dest);
    }

    let progress = ProgressBar::new(plan.total_bytes);
    progress.set_style(
        ProgressStyle::default_bar()
            .template("{bar:40} {bytes}/{total_bytes} ({eta})")
            .unwrap(),
    );

    let result = Mutex::new(CopyResult {
        copied: Vec::new(),
        conflicts: Vec::new(),
        errors: Vec::new(),
        bytes_copied: 0,
    });

    let jobs = jobs.max(1);
    let chunk_size = plan.files.len().div_ceil(jobs);
    let chunks: Vec<&[CopyOp]> = if plan.files.is_empty() {
        Vec::new()
    } else {
        plan.files.chunks(chunk_size).collect()
    };

    std::thread::scope(|s| {
        for chunk in &chunks {
            s.spawn(|| {
                for op in *chunk {
                    copy_file(op, &result, &progress);
                }
            });
        }
    });

    progress.finish_and_clear();
    result.into_inner().unwrap()
}

fn copy_file(op: &CopyOp, result: &Mutex<CopyResult>, progress: &ProgressBar) {
    let dest = if op.dest.exists() {
        find_restore_path(&op.dest)
    } else {
        op.dest.clone()
    };

    let is_conflict = dest != op.dest;

    match fs::copy(&op.source, &dest) {
        Ok(bytes) => {
            // Preserve permissions
            if let Ok(metadata) = fs::metadata(&op.source) {
                let _ = fs::set_permissions(&dest, metadata.permissions());
            }

            let mut r = result.lock().unwrap();
            r.bytes_copied += bytes;
            progress.inc(bytes);

            if is_conflict {
                r.conflicts.push(Conflict {
                    restore_path: dest,
                    original_path: op.dest.clone(),
                    size: bytes,
                    xdg_dir: op.xdg_dir,
                });
            } else {
                r.copied.push(CopiedFile {
                    source: op.source.clone(),
                    dest,
                    size: bytes,
                    xdg_dir: op.xdg_dir,
                });
            }
        }
        Err(error) => {
            progress.inc(op.size);
            let mut r = result.lock().unwrap();
            r.errors.push(CopyError {
                source: op.source.clone(),
                dest: op.dest.clone(),
                error,
                xdg_dir: op.xdg_dir,
            });
        }
    }
}

/// Find an available `.restore` path for a conflicting file.
///
/// Given `photo.jpg`, tries `photo.restore.jpg`, then `photo.restore.2.jpg`, etc.
/// For files without extensions, tries `file.restore`, `file.restore.2`, etc.
fn find_restore_path(original: &Path) -> PathBuf {
    let stem = original
        .file_stem()
        .unwrap_or_default();
    let ext = original.extension();
    let parent = original.parent().unwrap_or(Path::new(""));

    // First try: name.restore.ext
    let candidate = make_restore_name(stem, ext, None);
    let path = parent.join(&candidate);
    if !path.exists() {
        return path;
    }

    // Subsequent tries: name.restore.N.ext
    for n in 2u32.. {
        let candidate = make_restore_name(stem, ext, Some(n));
        let path = parent.join(&candidate);
        if !path.exists() {
            return path;
        }
    }

    unreachable!()
}

fn make_restore_name(stem: &std::ffi::OsStr, ext: Option<&std::ffi::OsStr>, n: Option<u32>) -> OsString {
    let mut name = OsString::from(stem);
    match n {
        None => name.push(".restore"),
        Some(n) => {
            name.push(".restore.");
            name.push(n.to_string());
        }
    }
    if let Some(ext) = ext {
        name.push(".");
        name.push(ext);
    }
    name
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{CopyOp, DirOp, XdgDir};
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn copies_single_file() {
        let src = tempdir().unwrap();
        let dest = tempdir().unwrap();
        fs::write(src.path().join("hello.txt"), "world").unwrap();

        let plan = CopyPlan {
            dirs: vec![DirOp {
                dest: dest.path().join("Documents"),
            }],
            files: vec![CopyOp {
                source: src.path().join("hello.txt"),
                dest: dest.path().join("Documents/hello.txt"),
                size: 5,
                xdg_dir: XdgDir::Documents,
            }],
            total_bytes: 5,
        };

        let result = execute_plan(&plan, 1);

        assert_eq!(result.copied.len(), 1);
        assert_eq!(result.conflicts.len(), 0);
        assert_eq!(result.errors.len(), 0);
        assert_eq!(result.bytes_copied, 5);
        assert_eq!(
            fs::read_to_string(dest.path().join("Documents/hello.txt")).unwrap(),
            "world"
        );
    }

    #[test]
    fn creates_restore_file_on_conflict() {
        let src = tempdir().unwrap();
        let dest = tempdir().unwrap();
        // Source file
        fs::write(src.path().join("notes.txt"), "new content").unwrap();
        // Pre-existing file at destination
        fs::create_dir(dest.path().join("Documents")).unwrap();
        fs::write(dest.path().join("Documents/notes.txt"), "old content").unwrap();

        let plan = CopyPlan {
            dirs: vec![],
            files: vec![CopyOp {
                source: src.path().join("notes.txt"),
                dest: dest.path().join("Documents/notes.txt"),
                size: 11,
                xdg_dir: XdgDir::Documents,
            }],
            total_bytes: 11,
        };

        let result = execute_plan(&plan, 1);

        assert_eq!(result.copied.len(), 0);
        assert_eq!(result.conflicts.len(), 1);
        // Original untouched
        assert_eq!(
            fs::read_to_string(dest.path().join("Documents/notes.txt")).unwrap(),
            "old content"
        );
        // Restore file created
        assert_eq!(
            fs::read_to_string(dest.path().join("Documents/notes.restore.txt")).unwrap(),
            "new content"
        );
        assert_eq!(
            result.conflicts[0].restore_path,
            dest.path().join("Documents/notes.restore.txt")
        );
        assert_eq!(
            result.conflicts[0].original_path,
            dest.path().join("Documents/notes.txt")
        );
    }

    #[test]
    fn increments_restore_suffix_on_collision() {
        let src = tempdir().unwrap();
        let dest = tempdir().unwrap();
        fs::write(src.path().join("photo.jpg"), "data3").unwrap();
        // Pre-existing original AND .restore
        fs::create_dir(dest.path().join("Pictures")).unwrap();
        fs::write(dest.path().join("Pictures/photo.jpg"), "data1").unwrap();
        fs::write(dest.path().join("Pictures/photo.restore.jpg"), "data2").unwrap();

        let plan = CopyPlan {
            dirs: vec![],
            files: vec![CopyOp {
                source: src.path().join("photo.jpg"),
                dest: dest.path().join("Pictures/photo.jpg"),
                size: 5,
                xdg_dir: XdgDir::Pictures,
            }],
            total_bytes: 5,
        };

        let result = execute_plan(&plan, 1);

        assert_eq!(result.conflicts.len(), 1);
        assert_eq!(
            result.conflicts[0].restore_path,
            dest.path().join("Pictures/photo.restore.2.jpg")
        );
        assert_eq!(
            fs::read_to_string(dest.path().join("Pictures/photo.restore.2.jpg")).unwrap(),
            "data3"
        );
    }

    #[test]
    fn handles_file_without_extension() {
        let src = tempdir().unwrap();
        let dest = tempdir().unwrap();
        fs::write(src.path().join("Makefile"), "new").unwrap();
        fs::create_dir(dest.path().join("Documents")).unwrap();
        fs::write(dest.path().join("Documents/Makefile"), "old").unwrap();

        let plan = CopyPlan {
            dirs: vec![],
            files: vec![CopyOp {
                source: src.path().join("Makefile"),
                dest: dest.path().join("Documents/Makefile"),
                size: 3,
                xdg_dir: XdgDir::Documents,
            }],
            total_bytes: 3,
        };

        let result = execute_plan(&plan, 1);

        assert_eq!(result.conflicts.len(), 1);
        assert_eq!(
            result.conflicts[0].restore_path,
            dest.path().join("Documents/Makefile.restore")
        );
    }

    #[test]
    fn preserves_file_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let src = tempdir().unwrap();
        let dest = tempdir().unwrap();
        let src_file = src.path().join("script.sh");
        fs::write(&src_file, "#!/bin/sh").unwrap();
        fs::set_permissions(&src_file, fs::Permissions::from_mode(0o755)).unwrap();

        let plan = CopyPlan {
            dirs: vec![DirOp {
                dest: dest.path().join("Documents"),
            }],
            files: vec![CopyOp {
                source: src_file,
                dest: dest.path().join("Documents/script.sh"),
                size: 9,
                xdg_dir: XdgDir::Documents,
            }],
            total_bytes: 9,
        };

        let result = execute_plan(&plan, 1);

        assert_eq!(result.errors.len(), 0);
        let perms = fs::metadata(dest.path().join("Documents/script.sh"))
            .unwrap()
            .permissions();
        assert_eq!(perms.mode() & 0o777, 0o755);
    }

    #[test]
    fn collects_errors_without_aborting() {
        let src = tempdir().unwrap();
        let dest = tempdir().unwrap();
        // One valid file
        fs::write(src.path().join("good.txt"), "good").unwrap();
        fs::create_dir(dest.path().join("Documents")).unwrap();

        let plan = CopyPlan {
            dirs: vec![],
            files: vec![
                CopyOp {
                    source: src.path().join("nonexistent.txt"),
                    dest: dest.path().join("Documents/nonexistent.txt"),
                    size: 10,
                    xdg_dir: XdgDir::Documents,
                },
                CopyOp {
                    source: src.path().join("good.txt"),
                    dest: dest.path().join("Documents/good.txt"),
                    size: 4,
                    xdg_dir: XdgDir::Documents,
                },
            ],
            total_bytes: 14,
        };

        let result = execute_plan(&plan, 1);

        assert_eq!(result.errors.len(), 1);
        assert_eq!(result.copied.len(), 1);
        // The good file was still copied
        assert!(dest.path().join("Documents/good.txt").exists());
    }

    #[test]
    fn copies_with_multiple_threads() {
        let src = tempdir().unwrap();
        let dest = tempdir().unwrap();
        fs::create_dir(dest.path().join("Downloads")).unwrap();

        let mut files = Vec::new();
        let mut total = 0u64;
        for i in 0..20 {
            let name = format!("file_{i}.txt");
            let content = format!("content_{i}");
            fs::write(src.path().join(&name), &content).unwrap();
            let size = content.len() as u64;
            total += size;
            files.push(CopyOp {
                source: src.path().join(&name),
                dest: dest.path().join(format!("Downloads/{name}")),
                size,
                xdg_dir: XdgDir::Downloads,
            });
        }

        let plan = CopyPlan {
            dirs: vec![],
            files,
            total_bytes: total,
        };

        let result = execute_plan(&plan, 4);

        assert_eq!(result.copied.len(), 20);
        assert_eq!(result.errors.len(), 0);
        assert_eq!(result.bytes_copied, total);
    }
}
