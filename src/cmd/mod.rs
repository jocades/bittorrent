use clap::Subcommand;

mod decode;
use decode::Decode;

mod info;
use info::Info;

mod peers;
use peers::Peers;

#[derive(Subcommand)]
pub enum Command {
    Decode(Decode),
    Info(Info),
    Peers(Peers),
}

impl Command {
    pub async fn execute(&self) -> crate::Result<()> {
        use Command::*;
        match self {
            Decode(cmd) => cmd.execute(),
            Info(cmd) => cmd.execute(),
            Peers(cmd) => cmd.execute().await,
        }
    }
}
