use clap::Parser;
use tracing::{error, level_filters::LevelFilter};
use tracing_subscriber::EnvFilter;

use bittorrent_starter_rust::{Cli, Result};

#[tokio::main]
async fn main() -> Result<()> {
    let filter = EnvFilter::builder()
        .with_default_directive(LevelFilter::DEBUG.into())
        .from_env()?
        .add_directive("bittorrent=trace".parse()?)
        .add_directive("hyper::proto=info".parse()?); // Remove noise from external crate logs

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .compact()
        .init();

    let mut cli = Cli::parse();

    if let Err(e) = cli.command.execute().await {
        error!("App error: {e}");
    }
    Ok(())
}
