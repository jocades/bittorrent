use std::time::Duration;

use anyhow::bail;
use tokio::task;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut tasks = Vec::new();

    for peer in 0..20 {
        let task = task::spawn(async move { download(peer).await });
        tasks.push(task);
    }

    for task in tasks {
        match task.await? {
            Ok(result) => {
                println!("OK: {result}");
            }
            Err(why) => {
                eprintln!("Error: {why}");
            }
        }
    }

    Ok(())
}

async fn download(peer: usize) -> anyhow::Result<String> {
    tokio::time::sleep(Duration::from_millis(200)).await;
    if peer == 10 {
        bail!("wrong peer: {peer}")
    }

    Ok(format!("{peer}: done!"))
}
