use walkdir::WalkDir;

use crate::types::{CopyOp, CopyPlan, DetectedMapping, DirOp};

/// Build a copy plan from confirmed mappings.
///
/// Enumerates all files and directories within each mapping's source,
/// producing `CopyOps` for files and `DirOps` for directories.
pub fn build_plan(mappings: &[DetectedMapping]) -> std::io::Result<CopyPlan> {
    let mut files = Vec::new();
    let mut dirs = Vec::new();
    let mut total_bytes: u64 = 0;

    for mapping in mappings {
        for entry in WalkDir::new(&mapping.source_path).follow_links(true) {
            let entry = entry?;
            let relative = entry.path().strip_prefix(&mapping.source_path).unwrap();

            if relative.as_os_str().is_empty() {
                // The root of the mapping itself — ensure the dest dir exists
                dirs.push(DirOp {
                    dest: mapping.dest_path.clone(),
                });
                continue;
            }

            let dest = mapping.dest_path.join(relative);

            if entry.file_type().is_dir() {
                dirs.push(DirOp { dest });
            } else {
                let metadata = entry.metadata()?;
                let size = metadata.len();
                total_bytes += size;
                files.push(CopyOp {
                    source: entry.path().to_path_buf(),
                    dest,
                    size,
                    xdg_dir: mapping.xdg_dir,
                });
            }
        }
    }

    Ok(CopyPlan {
        dirs,
        files,
        total_bytes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::XdgDir;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::tempdir;

    fn mapping(xdg_dir: XdgDir, source: PathBuf, dest: PathBuf) -> DetectedMapping {
        DetectedMapping {
            xdg_dir,
            source_path: source,
            dest_path: dest,
        }
    }

    #[test]
    fn builds_plan_for_single_file() {
        let backup = tempdir().unwrap();
        let home = tempdir().unwrap();
        let src_dir = backup.path().join("Documents");
        fs::create_dir(&src_dir).unwrap();
        fs::write(src_dir.join("readme.txt"), "hello").unwrap();

        let m = mapping(XdgDir::Documents, src_dir, home.path().join("Documents"));
        let plan = build_plan(&[m]).unwrap();

        assert_eq!(plan.files.len(), 1);
        assert_eq!(
            plan.files[0].source,
            backup.path().join("Documents/readme.txt")
        );
        assert_eq!(plan.files[0].dest, home.path().join("Documents/readme.txt"));
        assert_eq!(plan.files[0].size, 5);
        assert_eq!(plan.files[0].xdg_dir, XdgDir::Documents);
        assert_eq!(plan.total_bytes, 5);
    }

    #[test]
    fn includes_deeply_nested_files() {
        let backup = tempdir().unwrap();
        let home = tempdir().unwrap();
        let deep = backup.path().join("Documents/a/b/c");
        fs::create_dir_all(&deep).unwrap();
        fs::write(deep.join("deep.txt"), "nested").unwrap();

        let m = mapping(
            XdgDir::Documents,
            backup.path().join("Documents"),
            home.path().join("Documents"),
        );
        let plan = build_plan(&[m]).unwrap();

        assert_eq!(plan.files.len(), 1);
        assert_eq!(
            plan.files[0].dest,
            home.path().join("Documents/a/b/c/deep.txt")
        );
        // Dirs: Documents, a, b, c
        assert_eq!(plan.dirs.len(), 4);
    }

    #[test]
    fn creates_dir_ops_for_empty_directories() {
        let backup = tempdir().unwrap();
        let home = tempdir().unwrap();
        let src_dir = backup.path().join("Pictures");
        fs::create_dir(&src_dir).unwrap();
        fs::create_dir(src_dir.join("empty_album")).unwrap();

        let m = mapping(XdgDir::Pictures, src_dir, home.path().join("Pictures"));
        let plan = build_plan(&[m]).unwrap();

        assert_eq!(plan.files.len(), 0);
        // Dirs: Pictures, empty_album
        assert_eq!(plan.dirs.len(), 2);
        let dir_dests: Vec<_> = plan.dirs.iter().map(|d| d.dest.clone()).collect();
        assert!(dir_dests.contains(&home.path().join("Pictures")));
        assert!(dir_dests.contains(&home.path().join("Pictures/empty_album")));
    }

    #[test]
    fn handles_multiple_mappings() {
        let backup = tempdir().unwrap();
        let home = tempdir().unwrap();

        let docs = backup.path().join("Documents");
        fs::create_dir(&docs).unwrap();
        fs::write(docs.join("a.txt"), "aa").unwrap();

        let music = backup.path().join("Music");
        fs::create_dir(&music).unwrap();
        fs::write(music.join("b.mp3"), "bbb").unwrap();

        let mappings = vec![
            mapping(XdgDir::Documents, docs, home.path().join("Documents")),
            mapping(XdgDir::Music, music, home.path().join("Music")),
        ];
        let plan = build_plan(&mappings).unwrap();

        assert_eq!(plan.files.len(), 2);
        assert_eq!(plan.total_bytes, 5); // 2 + 3
    }

    #[test]
    fn sums_total_bytes_correctly() {
        let backup = tempdir().unwrap();
        let home = tempdir().unwrap();
        let src_dir = backup.path().join("Downloads");
        fs::create_dir(&src_dir).unwrap();
        fs::write(src_dir.join("file1"), "aaaa").unwrap(); // 4 bytes
        fs::write(src_dir.join("file2"), "bbbbbb").unwrap(); // 6 bytes

        let m = mapping(XdgDir::Downloads, src_dir, home.path().join("Downloads"));
        let plan = build_plan(&[m]).unwrap();

        assert_eq!(plan.total_bytes, 10);
    }
}
