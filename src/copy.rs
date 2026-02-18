use std::ffi::OsString;
use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;

use crate::types::{Conflict, CopiedFile, CopyError, CopyOp, CopyPlan, CopyResult};

/// Execute the copy plan, returning results with conflicts and errors.
///
/// Creates all directories first, then copies files in parallel across
/// `jobs` threads. If a destination file exists, writes to a `.restore`
/// suffixed path instead and records a conflict.
pub fn execute_plan(plan: &CopyPlan, jobs: usize) -> io::Result<CopyResult> {
    // Create all directories first
    for dir_op in &plan.dirs {
        fs::create_dir_all(&dir_op.dest)?;
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

    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(jobs.max(1))
        .build()
        .unwrap();

    pool.install(|| {
        plan.files.par_iter().for_each(|op| {
            copy_file(op, &result, &progress);
        });
    });

    progress.finish_and_clear();
    Ok(result.into_inner().unwrap())
}

fn copy_file(op: &CopyOp, result: &Mutex<CopyResult>, progress: &ProgressBar) {
    match try_copy_atomic(&op.source, &op.dest) {
        Ok(bytes) => {
            preserve_permissions(&op.source, &op.dest);
            let mut r = result.lock().unwrap();
            r.bytes_copied += bytes;
            progress.inc(bytes);
            r.copied.push(CopiedFile {
                source: op.source.clone(),
                dest: op.dest.clone(),
                size: bytes,
                xdg_dir: op.xdg_dir,
            });
        }
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
            match write_to_restore_path(&op.source, &op.dest) {
                Ok((restore_path, bytes)) => {
                    let mut r = result.lock().unwrap();
                    r.bytes_copied += bytes;
                    progress.inc(bytes);
                    r.conflicts.push(Conflict {
                        restore_path,
                        original_path: op.dest.clone(),
                        size: bytes,
                        xdg_dir: op.xdg_dir,
                    });
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

/// Atomically create dest and copy source into it.
/// Returns AlreadyExists if dest already exists.
fn try_copy_atomic(source: &Path, dest: &Path) -> io::Result<u64> {
    let mut src_file = File::open(source)?;
    let mut dst_file = File::create_new(dest)?;
    let bytes = io::copy(&mut src_file, &mut dst_file)?;
    Ok(bytes)
}

/// Copy source to a .restore path, retrying with incrementing suffixes
/// if those also already exist.
fn write_to_restore_path(source: &Path, original_dest: &Path) -> io::Result<(PathBuf, u64)> {
    let stem = original_dest.file_stem().unwrap_or_default();
    let ext = original_dest.extension();
    let parent = original_dest.parent().unwrap_or(Path::new(""));

    // First try: name.restore.ext
    let candidate = parent.join(make_restore_name(stem, ext, None));
    match try_copy_atomic(source, &candidate) {
        Ok(bytes) => {
            preserve_permissions(source, &candidate);
            return Ok((candidate, bytes));
        }
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(e),
    }

    // Subsequent tries: name.restore.N.ext
    for n in 2u32.. {
        let candidate = parent.join(make_restore_name(stem, ext, Some(n)));
        match try_copy_atomic(source, &candidate) {
            Ok(bytes) => {
                preserve_permissions(source, &candidate);
                return Ok((candidate, bytes));
            }
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }

    unreachable!()
}

fn preserve_permissions(source: &Path, dest: &Path) {
    if let Ok(metadata) = fs::metadata(source) {
        let _ = fs::set_permissions(dest, metadata.permissions());
    }
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

        let result = execute_plan(&plan, 1).unwrap();

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

        let result = execute_plan(&plan, 1).unwrap();

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

        let result = execute_plan(&plan, 1).unwrap();

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

        let result = execute_plan(&plan, 1).unwrap();

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

        let result = execute_plan(&plan, 1).unwrap();

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

        let result = execute_plan(&plan, 1).unwrap();

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

        let result = execute_plan(&plan, 4).unwrap();

        assert_eq!(result.copied.len(), 20);
        assert_eq!(result.errors.len(), 0);
        assert_eq!(result.bytes_copied, total);
    }

    #[test]
    fn returns_error_when_dir_creation_fails() {
        let src = tempdir().unwrap();
        let dest = tempdir().unwrap();
        fs::write(src.path().join("hello.txt"), "world").unwrap();

        // Create a file where a directory needs to go
        fs::write(dest.path().join("Documents"), "blocker").unwrap();

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

        assert!(execute_plan(&plan, 1).is_err());
    }
}
