use std::fs;

use tempfile::tempdir;

use backup_restore::conflict::{apply_resolution, Resolution};
use backup_restore::copy::execute_plan;
use backup_restore::error::XdgDir;
use backup_restore::plan::build_plan;
use backup_restore::report::{format_dry_run_report, format_report};
use backup_restore::scan::scan_backup;

/// Full pipeline: nested backup structure → scan → plan → copy → report
#[test]
fn full_pipeline_nested_backup() {
    // Build a mock backup: ~/backups/oryx-pro/oryx-pro/{Documents,Downloads}
    let backup_root = tempdir().unwrap();
    let home = tempdir().unwrap();

    let nested = backup_root.path().join("oryx-pro").join("oryx-pro");
    let docs = nested.join("Documents");
    let downloads = nested.join("Downloads");

    fs::create_dir_all(&docs).unwrap();
    fs::create_dir_all(&downloads).unwrap();
    fs::write(docs.join("notes.txt"), "my notes").unwrap();
    fs::write(docs.join("report.pdf"), "pdf content here").unwrap();
    fs::create_dir(docs.join("subdir")).unwrap();
    fs::write(docs.join("subdir").join("deep.txt"), "deep").unwrap();
    fs::write(downloads.join("setup.exe"), "binary data").unwrap();

    // Step 1: Scan
    let mappings = scan_backup(backup_root.path(), home.path());
    assert_eq!(mappings.len(), 2);

    let xdg_dirs: Vec<XdgDir> = mappings.iter().map(|m| m.xdg_dir).collect();
    assert!(xdg_dirs.contains(&XdgDir::Documents));
    assert!(xdg_dirs.contains(&XdgDir::Downloads));

    // Step 2: Plan
    let plan = build_plan(&mappings).unwrap();
    assert_eq!(plan.files.len(), 4); // notes.txt, report.pdf, deep.txt, setup.exe
    assert!(plan.total_bytes > 0);

    // Step 3: Copy
    let result = execute_plan(&plan, 2);
    assert_eq!(result.copied.len(), 4);
    assert_eq!(result.conflicts.len(), 0);
    assert_eq!(result.errors.len(), 0);

    // Verify files exist at destination
    assert_eq!(
        fs::read_to_string(home.path().join("Documents/notes.txt")).unwrap(),
        "my notes"
    );
    assert_eq!(
        fs::read_to_string(home.path().join("Documents/subdir/deep.txt")).unwrap(),
        "deep"
    );
    assert_eq!(
        fs::read_to_string(home.path().join("Downloads/setup.exe")).unwrap(),
        "binary data"
    );

    // Step 4: Report
    let report = format_report(&result, std::time::Duration::from_secs(1));
    assert!(report.contains("4 files copied"));
    assert!(report.contains("0 conflicts"));
    assert!(report.contains("0 errors"));
}

/// Pipeline with pre-existing files causing conflicts
#[test]
fn pipeline_with_conflicts_and_resolution() {
    let backup_root = tempdir().unwrap();
    let home = tempdir().unwrap();

    // Backup has Documents/readme.txt
    let docs = backup_root.path().join("Documents");
    fs::create_dir(&docs).unwrap();
    fs::write(docs.join("readme.txt"), "new readme").unwrap();
    fs::write(docs.join("fresh.txt"), "fresh file").unwrap();

    // Home already has Documents/readme.txt
    fs::create_dir(home.path().join("Documents")).unwrap();
    fs::write(home.path().join("Documents/readme.txt"), "old readme").unwrap();

    // Scan & plan
    let mappings = scan_backup(backup_root.path(), home.path());
    let plan = build_plan(&mappings).unwrap();

    // Copy
    let result = execute_plan(&plan, 1);
    assert_eq!(result.copied.len(), 1); // fresh.txt
    assert_eq!(result.conflicts.len(), 1); // readme.txt
    assert_eq!(result.errors.len(), 0);

    // Verify: original untouched, restore created
    assert_eq!(
        fs::read_to_string(home.path().join("Documents/readme.txt")).unwrap(),
        "old readme"
    );
    assert_eq!(
        fs::read_to_string(home.path().join("Documents/readme.restore.txt")).unwrap(),
        "new readme"
    );

    // Resolve: overwrite
    apply_resolution(&result.conflicts[0], Resolution::Overwrite).unwrap();
    assert_eq!(
        fs::read_to_string(home.path().join("Documents/readme.txt")).unwrap(),
        "new readme"
    );
    assert!(!home.path().join("Documents/readme.restore.txt").exists());
}

/// Dry-run pipeline: scan → plan → report, no files written to dest
#[test]
fn dry_run_does_not_write_files() {
    let backup_root = tempdir().unwrap();
    let home = tempdir().unwrap();

    // Set up backup with Documents and Music
    let docs = backup_root.path().join("Documents");
    let music = backup_root.path().join("Music");
    fs::create_dir(&docs).unwrap();
    fs::create_dir(&music).unwrap();
    fs::write(docs.join("notes.txt"), "my notes").unwrap();
    fs::write(music.join("song.mp3"), "audio data").unwrap();

    // Pre-create a file at dest to test conflict detection
    fs::create_dir(home.path().join("Documents")).unwrap();
    fs::write(home.path().join("Documents/notes.txt"), "old notes").unwrap();

    // Scan & plan (same as real pipeline)
    let mappings = scan_backup(backup_root.path(), home.path());
    let plan = build_plan(&mappings).unwrap();

    // Dry-run report instead of execute_plan
    let report = format_dry_run_report(&plan);

    // Report contains expected content
    assert!(report.contains("2 files"), "should show 2 files total");
    assert!(report.contains("Documents"), "should mention Documents");
    assert!(report.contains("Music"), "should mention Music");
    assert!(report.contains("1 conflict"), "should detect existing notes.txt");

    // Nothing was written to dest (Music dir should not exist, old file untouched)
    assert!(
        !home.path().join("Music").exists(),
        "Music dir should not be created in dry run"
    );
    assert_eq!(
        fs::read_to_string(home.path().join("Documents/notes.txt")).unwrap(),
        "old notes",
        "existing file should be untouched"
    );
}

/// Empty backup directories are created at destination
#[test]
fn empty_directories_created() {
    let backup_root = tempdir().unwrap();
    let home = tempdir().unwrap();

    let pics = backup_root.path().join("Pictures");
    fs::create_dir(&pics).unwrap();
    fs::create_dir(pics.join("Vacation")).unwrap();
    fs::create_dir(pics.join("Vacation").join("Empty Album")).unwrap();

    let mappings = scan_backup(backup_root.path(), home.path());
    let plan = build_plan(&mappings).unwrap();
    let result = execute_plan(&plan, 1);

    assert_eq!(result.copied.len(), 0);
    assert!(home.path().join("Pictures").is_dir());
    assert!(home.path().join("Pictures/Vacation").is_dir());
    assert!(home.path().join("Pictures/Vacation/Empty Album").is_dir());
}
