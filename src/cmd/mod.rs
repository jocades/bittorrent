use clap::Subcommand;

mod decode;
use decode::Decode;

mod info;
use info::Info;

#[derive(Subcommand)]
pub enum Command {
    Decode(Decode),
    Info(Info),
}

impl Command {
    pub fn execute(&self) -> crate::Result<()> {
        use Command::*;
        match self {
            Decode(cmd) => cmd.execute(),
            Info(cmd) => cmd.execute(),
        }
    }
}
