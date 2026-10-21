mod tui;

use clap::Parser;

#[derive(Parser)]
#[command(version, about = "Terminal interface for yt-dlp")]
struct Cli {}

#[tokio::main]
async fn main() {
    let _ = Cli::parse();
    if let Err(error) = tui::run().await {
        eprintln!("Quiver: {error}");
        std::process::exit(1);
    }
}
