use std::path::Path;

use walkdir::WalkDir;

use crate::types::{DetectedMapping, XdgDir};

pub struct ScanResult {
    pub mappings: Vec<DetectedMapping>,
    pub warnings: Vec<walkdir::Error>,
}

/// Scan a backup directory and detect XDG directory mappings.
///
/// Walks the backup root looking for directories matching XDG names.
/// When found, records the mapping and skips descending into them.
/// Returns all detected mappings (may include duplicates for the same `XdgDir`
/// if found at multiple paths), plus any walkdir errors as warnings.
pub fn scan_backup(backup_root: &Path, home_dir: &Path) -> ScanResult {
    let mut mappings = Vec::new();
    let mut warnings = Vec::new();

    let mut walker = WalkDir::new(backup_root).into_iter();
    while let Some(entry) = walker.next() {
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                warnings.push(e);
                continue;
            }
        };

        if !entry.file_type().is_dir() {
            continue;
        }

        // Don't match the root itself
        if entry.path() == backup_root {
            continue;
        }

        let Some(dir_name) = entry.file_name().to_str() else {
            continue;
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

    ScanResult { mappings, warnings }
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

        let result = scan_backup(backup.path(), home.path());

        assert_eq!(result.mappings.len(), 2);
        let xdg_names: Vec<XdgDir> = result.mappings.iter().map(|m| m.xdg_dir).collect();
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

        let result = scan_backup(backup.path(), home.path());

        assert_eq!(result.mappings.len(), 0);
    }

    #[test]
    fn does_not_descend_into_matched_xdg_dirs() {
        // If Documents contains a sub-directory called "Pictures",
        // that should NOT be detected as a separate mapping.
        let backup = tempdir().unwrap();
        let home = tempdir().unwrap();
        fs::create_dir_all(backup.path().join("Documents").join("Pictures")).unwrap();

        let result = scan_backup(backup.path(), home.path());

        assert_eq!(result.mappings.len(), 1);
        assert_eq!(result.mappings[0].xdg_dir, XdgDir::Documents);
    }

    #[test]
    fn detects_duplicate_xdg_dirs_at_different_paths() {
        let backup = tempdir().unwrap();
        let home = tempdir().unwrap();
        // Documents at two different nesting levels
        fs::create_dir_all(backup.path().join("Documents")).unwrap();
        fs::create_dir_all(backup.path().join("old-backup").join("Documents")).unwrap();

        let result = scan_backup(backup.path(), home.path());

        // Should find both — caller (main.rs) handles disambiguation
        let doc_mappings: Vec<&DetectedMapping> = result
            .mappings
            .iter()
            .filter(|m| m.xdg_dir == XdgDir::Documents)
            .collect();
        assert_eq!(doc_mappings.len(), 2);
    }

    #[test]
    fn returns_empty_for_empty_backup() {
        let backup = tempdir().unwrap();
        let home = tempdir().unwrap();

        let result = scan_backup(backup.path(), home.path());

        assert_eq!(result.mappings.len(), 0);
    }

    #[test]
    fn finds_all_eight_xdg_dirs() {
        let backup = tempdir().unwrap();
        let home = tempdir().unwrap();
        for xdg in &XdgDir::ALL {
            fs::create_dir(backup.path().join(xdg.dir_name())).unwrap();
        }

        let result = scan_backup(backup.path(), home.path());

        assert_eq!(result.mappings.len(), 8);
    }

    #[test]
    fn finds_xdg_dir_at_top_level() {
        let backup = tempdir().unwrap();
        let home = tempdir().unwrap();
        fs::create_dir(backup.path().join("Documents")).unwrap();

        let result = scan_backup(backup.path(), home.path());

        assert_eq!(result.mappings.len(), 1);
        assert_eq!(result.mappings[0].xdg_dir, XdgDir::Documents);
        assert_eq!(
            result.mappings[0].source_path,
            backup.path().join("Documents")
        );
        assert_eq!(result.mappings[0].dest_path, home.path().join("Documents"));
    }

    #[test]
    fn collects_warnings_for_unreadable_dirs() {
        use std::os::unix::fs::PermissionsExt;

        let backup = tempdir().unwrap();
        let home = tempdir().unwrap();
        // Create a readable XDG dir and an unreadable subdirectory
        fs::create_dir(backup.path().join("Documents")).unwrap();
        let unreadable = backup.path().join("sealed");
        fs::create_dir(&unreadable).unwrap();
        fs::set_permissions(&unreadable, fs::Permissions::from_mode(0o000)).unwrap();

        let result = scan_backup(backup.path(), home.path());

        assert_eq!(result.mappings.len(), 1);
        assert!(
            !result.warnings.is_empty(),
            "should collect warnings for unreadable dirs"
        );

        // Restore permissions so tempdir cleanup succeeds
        fs::set_permissions(&unreadable, fs::Permissions::from_mode(0o755)).unwrap();
    }
}
