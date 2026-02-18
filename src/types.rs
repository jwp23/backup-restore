use std::fmt;
use std::path::PathBuf;

/// The 8 user-facing XDG directories we care about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum XdgDir {
    Desktop,
    Documents,
    Downloads,
    Music,
    Pictures,
    Public,
    Templates,
    Videos,
}

impl XdgDir {
    pub const ALL: [XdgDir; 8] = [
        XdgDir::Desktop,
        XdgDir::Documents,
        XdgDir::Downloads,
        XdgDir::Music,
        XdgDir::Pictures,
        XdgDir::Public,
        XdgDir::Templates,
        XdgDir::Videos,
    ];

    /// Returns the directory name as it appears on disk.
    pub fn dir_name(&self) -> &'static str {
        match self {
            XdgDir::Desktop => "Desktop",
            XdgDir::Documents => "Documents",
            XdgDir::Downloads => "Downloads",
            XdgDir::Music => "Music",
            XdgDir::Pictures => "Pictures",
            XdgDir::Public => "Public",
            XdgDir::Templates => "Templates",
            XdgDir::Videos => "Videos",
        }
    }

    /// Try to parse a directory name into an XdgDir.
    pub fn from_dir_name(name: &str) -> Option<XdgDir> {
        XdgDir::ALL.iter().find(|d| d.dir_name() == name).copied()
    }
}

impl fmt::Display for XdgDir {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.dir_name())
    }
}

/// A detected backup → home directory mapping.
#[derive(Debug, Clone)]
pub struct DetectedMapping {
    pub xdg_dir: XdgDir,
    pub source_path: PathBuf,
    pub dest_path: PathBuf,
}

/// A single file copy operation.
#[derive(Debug, Clone)]
pub struct CopyOp {
    pub source: PathBuf,
    pub dest: PathBuf,
    pub size: u64,
    pub xdg_dir: XdgDir,
}

/// A directory that needs to be created.
#[derive(Debug, Clone)]
pub struct DirOp {
    pub dest: PathBuf,
}

/// The full copy plan.
#[derive(Debug, Clone)]
pub struct CopyPlan {
    pub dirs: Vec<DirOp>,
    pub files: Vec<CopyOp>,
    pub total_bytes: u64,
}

/// A successfully copied file.
#[derive(Debug, Clone)]
pub struct CopiedFile {
    pub source: PathBuf,
    pub dest: PathBuf,
    pub size: u64,
    pub xdg_dir: XdgDir,
}

/// A conflict: dest existed, so we wrote to a .restore path instead.
#[derive(Debug, Clone)]
pub struct Conflict {
    pub restore_path: PathBuf,
    pub original_path: PathBuf,
    pub size: u64,
    pub xdg_dir: XdgDir,
}

/// An error during copy of a single file.
#[derive(Debug)]
pub struct CopyError {
    pub source: PathBuf,
    pub dest: PathBuf,
    pub error: std::io::Error,
    pub xdg_dir: XdgDir,
}

impl fmt::Display for CopyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}: {} → {} ({})",
            self.xdg_dir,
            self.source.display(),
            self.dest.display(),
            self.error
        )
    }
}

impl std::error::Error for CopyError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.error)
    }
}

/// Results of the copy operation.
#[derive(Debug)]
pub struct CopyResult {
    pub copied: Vec<CopiedFile>,
    pub conflicts: Vec<Conflict>,
    pub errors: Vec<CopyError>,
    pub bytes_copied: u64,
}
