mod highlight;
mod scanner;
mod trash;
mod ui;

use clap::Parser;
use std::path::PathBuf;
use std::sync::mpsc;

#[derive(Parser)]
#[command(name = "disk-tree", about = "macOS disk space scanner TUI")]
struct Cli {
    /// Directory to scan (defaults to $HOME)
    path: Option<PathBuf>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let scan_path = cli
        .path
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| PathBuf::from("."));
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));

    let (tx, rx) = mpsc::channel();
    let bg_path = scan_path.clone();
    std::thread::spawn(move || {
        let result = scanner::scan_directory(&bg_path);
        let _ = tx.send(result);
    });

    let app = ui::App::new_scanning(&scan_path);
    ui::run(app, Some(rx), home)?;

    Ok(())
}
