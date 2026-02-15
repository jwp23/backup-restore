use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

use clap::Parser;
use console::style;
use dialoguer::{Confirm, Select};

use backup_restore::conflict::{self, Resolution};
use backup_restore::error::{Conflict, DetectedMapping, XdgDir};
use backup_restore::copy;
use backup_restore::{plan, report, scan};

#[derive(Parser)]
#[command(name = "backup-restore", about = "Restore files from a backup into your home directory")]
struct Cli {
    /// Path to the backup directory to restore from
    backup_dir: PathBuf,

    /// Number of parallel copy threads
    #[arg(short, long, default_value_t = 4)]
    jobs: usize,

    /// Home directory to restore into (defaults to $HOME)
    #[arg(long)]
    home: Option<PathBuf>,
}

fn main() {
    let cli = Cli::parse();

    let home_dir = cli.home.unwrap_or_else(|| {
        PathBuf::from(std::env::var("HOME").expect("HOME environment variable not set"))
    });

    if !cli.backup_dir.is_dir() {
        eprintln!(
            "{} Backup directory does not exist: {}",
            style("Error:").red().bold(),
            cli.backup_dir.display()
        );
        std::process::exit(1);
    }

    // Step 1: Scan
    println!(
        "{} Scanning {}...",
        style("→").cyan().bold(),
        cli.backup_dir.display()
    );
    let all_mappings = scan::scan_backup(&cli.backup_dir, &home_dir);

    if all_mappings.is_empty() {
        println!(
            "{} No XDG directories found in backup.",
            style("!").yellow().bold()
        );
        return;
    }

    // Handle duplicates: group by XdgDir, let user choose if ambiguous
    let mappings = resolve_duplicate_mappings(all_mappings);

    // Show detected mappings and confirm
    println!(
        "\n{} Detected {} directories:",
        style("✓").green().bold(),
        mappings.len()
    );
    for m in &mappings {
        println!(
            "  {} → {}",
            style(m.source_path.display()).dim(),
            m.dest_path.display()
        );
    }
    println!();

    if !Confirm::new()
        .with_prompt("Proceed with restore?")
        .default(true)
        .interact()
        .unwrap_or(false)
    {
        println!("Aborted.");
        return;
    }

    // Step 2: Plan
    let copy_plan = match plan::build_plan(&mappings) {
        Ok(p) => p,
        Err(e) => {
            eprintln!(
                "{} Failed to build copy plan: {}",
                style("Error:").red().bold(),
                e
            );
            std::process::exit(1);
        }
    };

    println!(
        "\n{} {} files to copy ({} total)",
        style("→").cyan().bold(),
        copy_plan.files.len(),
        report::format_bytes(copy_plan.total_bytes)
    );

    // Step 3: Copy
    let start = Instant::now();
    let result = copy::execute_plan(&copy_plan, cli.jobs);
    let elapsed = start.elapsed();

    // Step 4: Report
    print!("{}", report::format_report(&result, elapsed));

    // Step 5: Conflict resolution
    if !result.conflicts.is_empty() {
        println!();
        resolve_conflicts(&result.conflicts);
    }

    // Step 6: Optional source cleanup
    if !result.copied.is_empty() || !result.conflicts.is_empty() {
        println!();
        if Confirm::new()
            .with_prompt("Delete source files from backup?")
            .default(false)
            .interact()
            .unwrap_or(false)
        {
            delete_sources(&mappings, &cli.backup_dir);
        }
    }
}

fn resolve_duplicate_mappings(all_mappings: Vec<DetectedMapping>) -> Vec<DetectedMapping> {
    let mut by_dir: HashMap<XdgDir, Vec<DetectedMapping>> = HashMap::new();
    for m in all_mappings {
        by_dir.entry(m.xdg_dir).or_default().push(m);
    }

    let mut chosen = Vec::new();
    for (xdg_dir, candidates) in by_dir {
        if candidates.len() == 1 {
            chosen.push(candidates.into_iter().next().unwrap());
        } else {
            println!(
                "\n{} Multiple '{}' directories found:",
                style("?").yellow().bold(),
                xdg_dir
            );
            let labels: Vec<String> = candidates
                .iter()
                .map(|c| c.source_path.display().to_string())
                .collect();

            let selection = Select::new()
                .with_prompt(format!("Which {} to restore?", xdg_dir))
                .items(&labels)
                .default(0)
                .interact()
                .unwrap();

            chosen.push(candidates.into_iter().nth(selection).unwrap());
        }
    }

    chosen.sort_by(|a, b| a.xdg_dir.dir_name().cmp(b.xdg_dir.dir_name()));
    chosen
}

fn resolve_conflicts(conflicts: &[Conflict]) {
    let options = &[
        "Overwrite all originals with restored versions",
        "Keep all originals (delete .restore files)",
        "Decide per folder",
        "Decide individually",
        "Leave as-is (keep both)",
    ];

    let selection = Select::new()
        .with_prompt("How to handle conflicts?")
        .items(options)
        .default(4)
        .interact()
        .unwrap();

    match selection {
        0 => apply_to_all(conflicts, Resolution::Overwrite),
        1 => apply_to_all(conflicts, Resolution::KeepOriginal),
        2 => resolve_per_folder(conflicts),
        3 => resolve_individually(conflicts),
        4 => apply_to_all(conflicts, Resolution::LeaveAsIs),
        _ => unreachable!(),
    }
}

fn apply_to_all(conflicts: &[Conflict], resolution: Resolution) {
    for c in conflicts {
        if let Err(e) = conflict::apply_resolution(c, resolution) {
            eprintln!(
                "{} Failed to resolve {}: {}",
                style("Error:").red().bold(),
                c.original_path.display(),
                e
            );
        }
    }
}

fn resolve_per_folder(conflicts: &[Conflict]) {
    let mut by_dir: HashMap<XdgDir, Vec<&Conflict>> = HashMap::new();
    for c in conflicts {
        by_dir.entry(c.xdg_dir).or_default().push(c);
    }

    let options = &["Overwrite", "Keep originals", "Leave as-is"];

    for (xdg_dir, folder_conflicts) in &by_dir {
        let selection = Select::new()
            .with_prompt(format!("{} ({} conflicts)", xdg_dir, folder_conflicts.len()))
            .items(options)
            .default(2)
            .interact()
            .unwrap();

        let resolution = match selection {
            0 => Resolution::Overwrite,
            1 => Resolution::KeepOriginal,
            _ => Resolution::LeaveAsIs,
        };

        for c in folder_conflicts {
            if let Err(e) = conflict::apply_resolution(c, resolution) {
                eprintln!(
                    "{} Failed to resolve {}: {}",
                    style("Error:").red().bold(),
                    c.original_path.display(),
                    e
                );
            }
        }
    }
}

fn resolve_individually(conflicts: &[Conflict]) {
    let options = &["Overwrite", "Keep original", "Leave as-is"];

    for c in conflicts {
        let prompt = format!(
            "{} (restore: {} bytes)",
            c.original_path.display(),
            c.size
        );
        let selection = Select::new()
            .with_prompt(prompt)
            .items(options)
            .default(2)
            .interact()
            .unwrap();

        let resolution = match selection {
            0 => Resolution::Overwrite,
            1 => Resolution::KeepOriginal,
            _ => Resolution::LeaveAsIs,
        };

        if let Err(e) = conflict::apply_resolution(c, resolution) {
            eprintln!(
                "{} Failed to resolve {}: {}",
                style("Error:").red().bold(),
                c.original_path.display(),
                e
            );
        }
    }
}

fn delete_sources(mappings: &[DetectedMapping], _backup_dir: &PathBuf) {
    for mapping in mappings {
        if let Err(e) = std::fs::remove_dir_all(&mapping.source_path) {
            eprintln!(
                "{} Failed to delete {}: {}",
                style("Error:").red().bold(),
                mapping.source_path.display(),
                e
            );
        } else {
            println!("  Deleted {}", mapping.source_path.display());
        }
    }
}
