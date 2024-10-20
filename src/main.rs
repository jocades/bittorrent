use clap::Parser;
use tracing::{error, level_filters::LevelFilter};
use tracing_subscriber::EnvFilter;

use bittorrent_starter_rust::{Cli, Result};

#[tokio::main]
async fn main() -> Result<()> {
    setup_logging()?;

    let mut cli = Cli::parse();

    if let Err(e) = cli.command.execute().await {
        error!("App error: {e}");
    }
    Ok(())
}

fn setup_logging() -> Result<()> {
    let filter = EnvFilter::builder()
        .with_default_directive(LevelFilter::INFO.into())
        .from_env()?
        // .add_directive("bittorrent=debug".parse()?)
        .add_directive("hyper::proto=info".parse()?); // Remove noise from external crate logs

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .compact()
        .init();

    Ok(())
}
