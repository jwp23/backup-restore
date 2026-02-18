use std::fs;
use crate::types::Conflict;

/// What to do with a conflict.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resolution {
    /// Replace original with the .restore version.
    Overwrite,
    /// Delete the .restore file, keeping the original.
    KeepOriginal,
    /// Leave both files in place.
    LeaveAsIs,
}

/// Apply a resolution to a single conflict.
pub fn apply_resolution(conflict: &Conflict, resolution: Resolution) -> std::io::Result<()> {
    match resolution {
        Resolution::Overwrite => {
            fs::rename(&conflict.restore_path, &conflict.original_path)?;
        }
        Resolution::KeepOriginal => {
            fs::remove_file(&conflict.restore_path)?;
        }
        Resolution::LeaveAsIs => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::XdgDir;
    use std::fs;
    use std::path::Path;
    use tempfile::tempdir;

    fn make_conflict(dir: &Path) -> Conflict {
        fs::write(dir.join("photo.jpg"), "original").unwrap();
        fs::write(dir.join("photo.restore.jpg"), "restored").unwrap();
        Conflict {
            restore_path: dir.join("photo.restore.jpg"),
            original_path: dir.join("photo.jpg"),
            size: 8,
            xdg_dir: XdgDir::Pictures,
        }
    }

    #[test]
    fn overwrite_replaces_original_with_restore() {
        let dir = tempdir().unwrap();
        let conflict = make_conflict(dir.path());

        apply_resolution(&conflict, Resolution::Overwrite).unwrap();

        assert_eq!(fs::read_to_string(dir.path().join("photo.jpg")).unwrap(), "restored");
        assert!(!dir.path().join("photo.restore.jpg").exists());
    }

    #[test]
    fn keep_original_deletes_restore_file() {
        let dir = tempdir().unwrap();
        let conflict = make_conflict(dir.path());

        apply_resolution(&conflict, Resolution::KeepOriginal).unwrap();

        assert_eq!(fs::read_to_string(dir.path().join("photo.jpg")).unwrap(), "original");
        assert!(!dir.path().join("photo.restore.jpg").exists());
    }

    #[test]
    fn leave_as_is_keeps_both_files() {
        let dir = tempdir().unwrap();
        let conflict = make_conflict(dir.path());

        apply_resolution(&conflict, Resolution::LeaveAsIs).unwrap();

        assert!(dir.path().join("photo.jpg").exists());
        assert!(dir.path().join("photo.restore.jpg").exists());
        assert_eq!(fs::read_to_string(dir.path().join("photo.jpg")).unwrap(), "original");
        assert_eq!(
            fs::read_to_string(dir.path().join("photo.restore.jpg")).unwrap(),
            "restored"
        );
    }
}
