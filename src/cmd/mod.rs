use clap::Subcommand;

mod decode;
use decode::Decode;

#[derive(Subcommand)]
pub enum Command {
    Decode(Decode),
}

impl Command {
    pub fn execute(&self) -> crate::Result<()> {
        use Command::*;
        match self {
            Decode(cmd) => cmd.execute(),
        }
    }
}
