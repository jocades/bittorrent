use clap::Subcommand;

mod decode;
use decode::Decode;

mod info;
use info::Info;

mod peers;
use peers::Peers;

mod handshake;
use handshake::Handshake;

#[derive(Subcommand)]
pub enum Command {
    Decode(Decode),
    Info(Info),
    Peers(Peers),
    Handshake(Handshake),
}

impl Command {
    pub async fn execute(&self) -> crate::Result<()> {
        use Command::*;
        match self {
            Decode(cmd) => cmd.execute(),
            Info(cmd) => cmd.execute(),
            Peers(cmd) => cmd.execute().await,
            Handshake(cmd) => cmd.execute().await,
        }
    }
}
