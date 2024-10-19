use std::future::Future;

use anyhow::bail;
use anyhow::Result;
use tokio::select;
use tokio::task::JoinHandle;
use tokio::time::{interval, Duration};
use tracing::info_span;
use tracing::{debug, error, trace, Instrument};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let mut tasks = Vec::new();

    for peer in 0..5 {
        let task = spawn(info_span!("traced"), async move { download(peer).await });
        tasks.push(task);
    }

    for task in tasks {
        match task.await? {
            Ok(_) => {
                println!("Ok");
            }
            Err(why) => {
                eprintln!("Error: {why}");
            }
        }
    }

    Ok(())
}

#[derive(Debug, thiserror::Error)]
#[error("cancelled")]
struct CancelledError;

#[track_caller]
pub fn spawn<T>(span: tracing::Span, future: T) -> JoinHandle<T::Output>
where
    T: Future<Output = anyhow::Result<()>> + Send + 'static,
    T::Output: Send + 'static,
{
    tokio::spawn(
        async {
            trace!("started");
            tokio::pin!(future);
            let mut interval = interval(Duration::from_secs(5));

            loop {
                select! {
                    _ = interval.tick() => trace!("still running"),
                    result = &mut future => {
                        match result {
                            Ok(v) => {trace!("finished"); return Ok(v)},
                            Err(e) => {
                                if e.is::<CancelledError>() {
                                    debug!("task canclled")
                                } else {
                                    error!("finished with error: {e:#}")
                                }
                            }
                        }
                        return Ok(());
                    },
                };
            }
        }
        .instrument(span),
    )
}

async fn download(peer: usize) -> anyhow::Result<()> {
    println!("{peer}: spawned");
    tokio::time::sleep(Duration::from_millis(200)).await;
    if peer == 3 {
        tokio::time::sleep(Duration::from_millis(500)).await;
        bail!("timeout! {peer}")
    }
    println!("{peer}: done!");
    Ok(())
}
