mod cmd;

use anyhow::Result;
use clap::Parser;

#[derive(Parser)]
#[command(version, author, propagate_version = true)]
struct Cli {
    #[command(subcommand)]
    command: cmd::Command,
}

// Usage: your_bittorrent.sh decode "<encoded_value>"
fn main() -> Result<()> {
    let cli = Cli::parse();
    cli.command.execute()
}
