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

mod download;
use download::Download;

#[derive(Subcommand)]
#[clap(rename_all = "snake_case")]
pub enum Command {
    Decode(Decode),
    Info(Info),
    Peers(Peers),
    Handshake(Handshake),
    DownloadPiece(DownloadPiece),
    Download(Download),
}

impl Command {
    pub async fn execute(&mut self) -> crate::Result<()> {
        use Command::*;
        match self {
            Decode(cmd) => cmd.execute(),
            Info(cmd) => cmd.execute(),
            Peers(cmd) => cmd.execute().await,
            Handshake(cmd) => cmd.execute().await,
            DownloadPiece(cmd) => cmd.execute().await,
            Download(cmd) => cmd.execute().await,
        }
    }
}
