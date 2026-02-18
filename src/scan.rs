use std::path::Path;

use walkdir::WalkDir;

use crate::types::{DetectedMapping, XdgDir};

/// Scan a backup directory and detect XDG directory mappings.
///
/// Walks the backup root looking for directories matching XDG names.
/// When found, records the mapping and skips descending into them.
/// Returns all detected mappings (may include duplicates for the same XdgDir
/// if found at multiple paths).
pub fn scan_backup(backup_root: &Path, home_dir: &Path) -> Vec<DetectedMapping> {
    let mut mappings = Vec::new();

    let mut walker = WalkDir::new(backup_root).into_iter();
    while let Some(entry) = walker.next() {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };

        if !entry.file_type().is_dir() {
            continue;
        }

        // Don't match the root itself
        if entry.path() == backup_root {
            continue;
        }

        let dir_name = match entry.file_name().to_str() {
            Some(name) => name,
            None => continue,
        };

        if let Some(xdg_dir) = XdgDir::from_dir_name(dir_name) {
            mappings.push(DetectedMapping {
                xdg_dir,
                source_path: entry.path().to_path_buf(),
                dest_path: home_dir.join(xdg_dir.dir_name()),
            });
            walker.skip_current_dir();
        }
    }

    mappings
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn finds_xdg_dirs_nested_in_backup() {
        // Simulates ~/backups/oryx-pro/oryx-pro/Documents
        let backup = tempdir().unwrap();
        let home = tempdir().unwrap();
        let nested = backup.path().join("oryx-pro").join("oryx-pro");
        fs::create_dir_all(nested.join("Documents")).unwrap();
        fs::create_dir_all(nested.join("Downloads")).unwrap();

        let mappings = scan_backup(backup.path(), home.path());

        assert_eq!(mappings.len(), 2);
        let xdg_names: Vec<XdgDir> = mappings.iter().map(|m| m.xdg_dir).collect();
        assert!(xdg_names.contains(&XdgDir::Documents));
        assert!(xdg_names.contains(&XdgDir::Downloads));
    }

    #[test]
    fn skips_non_xdg_dirs() {
        let backup = tempdir().unwrap();
        let home = tempdir().unwrap();
        fs::create_dir_all(backup.path().join(".config")).unwrap();
        fs::create_dir_all(backup.path().join(".local")).unwrap();
        fs::create_dir_all(backup.path().join("random_stuff")).unwrap();

        let mappings = scan_backup(backup.path(), home.path());

        assert_eq!(mappings.len(), 0);
    }

    #[test]
    fn does_not_descend_into_matched_xdg_dirs() {
        // If Documents contains a sub-directory called "Pictures",
        // that should NOT be detected as a separate mapping.
        let backup = tempdir().unwrap();
        let home = tempdir().unwrap();
        fs::create_dir_all(backup.path().join("Documents").join("Pictures")).unwrap();

        let mappings = scan_backup(backup.path(), home.path());

        assert_eq!(mappings.len(), 1);
        assert_eq!(mappings[0].xdg_dir, XdgDir::Documents);
    }

    #[test]
    fn detects_duplicate_xdg_dirs_at_different_paths() {
        let backup = tempdir().unwrap();
        let home = tempdir().unwrap();
        // Documents at two different nesting levels
        fs::create_dir_all(backup.path().join("Documents")).unwrap();
        fs::create_dir_all(backup.path().join("old-backup").join("Documents")).unwrap();

        let mappings = scan_backup(backup.path(), home.path());

        // Should find both — caller (main.rs) handles disambiguation
        let doc_mappings: Vec<&DetectedMapping> = mappings
            .iter()
            .filter(|m| m.xdg_dir == XdgDir::Documents)
            .collect();
        assert_eq!(doc_mappings.len(), 2);
    }

    #[test]
    fn returns_empty_for_empty_backup() {
        let backup = tempdir().unwrap();
        let home = tempdir().unwrap();

        let mappings = scan_backup(backup.path(), home.path());

        assert_eq!(mappings.len(), 0);
    }

    #[test]
    fn finds_all_eight_xdg_dirs() {
        let backup = tempdir().unwrap();
        let home = tempdir().unwrap();
        for xdg in &XdgDir::ALL {
            fs::create_dir(backup.path().join(xdg.dir_name())).unwrap();
        }

        let mappings = scan_backup(backup.path(), home.path());

        assert_eq!(mappings.len(), 8);
    }

    #[test]
    fn finds_xdg_dir_at_top_level() {
        let backup = tempdir().unwrap();
        let home = tempdir().unwrap();
        fs::create_dir(backup.path().join("Documents")).unwrap();

        let mappings = scan_backup(backup.path(), home.path());

        assert_eq!(mappings.len(), 1);
        assert_eq!(mappings[0].xdg_dir, XdgDir::Documents);
        assert_eq!(mappings[0].source_path, backup.path().join("Documents"));
        assert_eq!(mappings[0].dest_path, home.path().join("Documents"));
    }
}
