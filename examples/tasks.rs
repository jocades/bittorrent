use std::time::Duration;

use anyhow::bail;
use tokio::task::JoinSet;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // let mut tasks = Vec::new();
    let mut tasks = JoinSet::new();

    for peer in 0..5 {
        // tasks.push(task::spawn(async move { download(peer).await }));
        tasks.spawn(async move { download(peer).await });
    }

    /* for task in tasks {
        match task.await? {
            Ok(v) => {
                println!("OK: {v}");
            }
            Err(why) => {
                eprintln!("Error: {why}");
            }
        }
    } */

    if let Some("loop") = std::env::args().nth(1).as_deref() {
        println!("LOOP");
        loop {
            match tasks.join_next().await {
                // The actual return type is wrapped in the result of the join handle: Result<Result<..>.
                Some(result) => match result? {
                    Ok(v) => {
                        println!("OK: {v}");
                    }
                    Err(why) => {
                        eprintln!("Error: {why}");
                    }
                },
                None => break,
            };
        }
        return Ok(());
    }

    println!("WHILE");
    // Matching only the Ok() of the Join Handle Result
    while let Some(Ok(result)) = tasks.join_next().await {
        match result {
            Ok(v) => {
                println!("OK: {v}");
            }
            Err(why) => {
                eprintln!("Error: {why}");
            }
        }
    }

    Ok(())
}

async fn download(peer: usize) -> anyhow::Result<usize> {
    println!("{peer}: spawned");
    tokio::time::sleep(Duration::from_millis(200)).await;
    if peer == 3 {
        tokio::time::sleep(Duration::from_millis(500)).await;
        bail!("timeout! {peer}")
    }
    println!("{peer}: done!");
    Ok(peer)
}
