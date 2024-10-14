mod cmd;
use cmd::Command;

mod torrent;
use torrent::Torrent;

use anyhow::Result;
use clap::Parser;

#[derive(Parser)]
#[command(version, author, propagate_version = true)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    cli.command.execute().await
}
