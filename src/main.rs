mod cmd;
use cmd::Command;

mod torrent;
use torrent::Torrent;

mod peer;
use peer::PEER_ID;

use anyhow::Result;
use clap::Parser;

#[derive(Parser)]
#[command(version, author, propagate_version = true)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();
    cli.command.execute().await
}
