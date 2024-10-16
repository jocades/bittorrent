use anyhow::{bail, Context};
use clap::Args;
use serde_json::Value;

#[derive(Args)]
pub struct Decode {
    encoded: String,
}

fn decode(encoded: &str) -> crate::Result<(Value, &str)> {
    match encoded.chars().next() {
        Some('0'..='9') => {
            let (len, rest) = encoded
                .split_once(':')
                .context("string delimiter not found")?;
            let len = len.parse::<usize>()?;
            return Ok((rest[..len].into(), &rest[len..]));
        }
        Some('i') => {
            let (n, rest) = &encoded[1..]
                .split_once('e')
                .context("number delimiter not found")?;
            let n = n.parse::<i64>()?;
            return Ok((n.into(), rest));
        }
        Some('l') => {
            let mut values = Vec::new();
            let mut rest = &encoded[1..];
            while !rest.is_empty() && !rest.starts_with('e') {
                let (v, remainder) = decode(rest)?;
                values.push(v);
                rest = remainder;
            }
            return Ok((values.into(), &rest[1..]));
        }
        Some('d') => {
            let mut values = serde_json::Map::new();
            let mut rest = &encoded[1..];
            while !rest.is_empty() && !rest.starts_with('e') {
                let (Value::String(k), remainder) = decode(rest)? else {
                    bail!("invalid dict key, expected string")
                };
                let (v, remainder) = decode(remainder)?;
                values.insert(k, v);
                rest = remainder;
            }
            return Ok((Value::Object(values), &rest[1..]));
        }
        _ => {}
    }

    bail!("unhandled encoded value: {}", encoded)
}

impl Decode {
    pub fn execute(&self) -> crate::Result<()> {
        let decoded = decode(&self.encoded)?.0;
        println!("{decoded}");

        Ok(())
    }
}
