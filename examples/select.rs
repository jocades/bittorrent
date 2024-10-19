use tokio::time::{interval, sleep, Duration};
use tokio::{signal, sync::oneshot};

#[tokio::main]
async fn main() {
    let (send, mut recv) = oneshot::channel();
    let mut interval = interval(Duration::from_millis(100));

    tokio::spawn(async move {
        sleep(Duration::from_secs(1)).await;
        send.send("shut down").unwrap();
    });

    loop {
        tokio::select! {
            _ = interval.tick() => println!("Another 100ms"),
            msg = &mut recv => {
                println!("Got message: {}", msg.unwrap());
                break;
            }
        }
    }
}

#[allow(dead_code)]
async fn simple_select() {
    let (tx1, rx1) = oneshot::channel();
    let (tx2, rx2) = oneshot::channel();

    tokio::spawn(async {
        let _ = tx1.send("one");
    });

    tokio::spawn(async {
        let _ = tx2.send("two");
    });

    tokio::select! {
        val = rx1 => {
            println!("rx1 completed first with {:?}", val);
        }
        val = rx2 => {
            println!("rx2 completed first with {:?}", val);
        }
    }

    // ... spawn application as separate task ...

    match signal::ctrl_c().await {
        Ok(()) => println!("Gracefully shutting down"),
        Err(err) => {
            eprintln!("Unable to listen for shutdown signal: {}", err);
            // we also shut down in case of error
        }
    }

    // send shutdown signal to application and wait
}
