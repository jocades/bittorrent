use clap::Subcommand;

mod decode;
use decode::Decode;

mod info;
use info::Info;

mod peers;
use peers::Peers;

mod handshake;
use handshake::Handshake;

mod download_piece;
use download_piece::DownloadPiece;

#[derive(Subcommand)]
pub enum Command {
    Decode(Decode),
    Info(Info),
    Peers(Peers),
    Handshake(Handshake),
    DownloadPiece(DownloadPiece),
}

impl Command {
    pub async fn execute(&self) -> crate::Result<()> {
        use Command::*;
        match self {
            Decode(cmd) => cmd.execute(),
            Info(cmd) => cmd.execute(),
            Peers(cmd) => cmd.execute().await,
            Handshake(cmd) => cmd.execute().await,
            DownloadPiece(cmd) => cmd.execute().await,
        }
    }
}
