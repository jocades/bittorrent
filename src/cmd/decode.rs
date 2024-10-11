use anyhow::{bail, Context};
use clap::Args;
// use serde_bencode

#[derive(Args)]
pub struct Decode {
    encoded: String,
}

impl Decode {
    pub fn execute(&self) -> crate::Result<()> {
        let c = self.encoded.chars().next().context("no next char")?;

        let value: serde_json::Value = if c.is_digit(10) {
            let colon_index = self.encoded.find(':').unwrap();
            let number_string = &self.encoded[..colon_index];
            let number = number_string.parse::<i64>().unwrap();
            self.encoded[colon_index + 1..colon_index + 1 + number as usize].into()
        } else if c == 'i' {
            let i = self
                .encoded
                .find("e")
                .context("number delimiter not found")?;

            let n = self.encoded[1..i].parse::<i64>()?;
            n.into()
        } else {
            bail!("unhandled encoded value: {}", self.encoded)
        };

        println!("{value}");

        Ok(())
    }
}
