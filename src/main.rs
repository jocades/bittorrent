mod cmd;
use cmd::Command;

mod torrent;
use torrent::Torrent;

mod tracker;

mod peer;
use peer::{Frame, Peer, PEER_ID};

mod download;

use anyhow::Result;
use clap::Parser;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(version, author, propagate_version = true)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[tokio::main]
async fn main() -> Result<()> {
    let filter = EnvFilter::builder()
        .with_default_directive(LevelFilter::INFO.into())
        .from_env()?
        .add_directive("bittorrent=trace".parse()?)
        .add_directive("hyper::proto=info".parse()?); // Remove noise from external crate logs

    tracing_subscriber::fmt().with_env_filter(filter).init();

    let cli = Cli::parse();
    cli.command.execute().await
}
