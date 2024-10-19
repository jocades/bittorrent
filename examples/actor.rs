//! https://docs.rs/tokio/latest/tokio/sync/index.html

//! The mpsc and oneshot channels can be combined to provide a request / response
//! type synchronization pattern with a shared resource. A task is spawned to
//! synchronize a resource and waits on commands received on a mpsc channel.
//! Each command includes a oneshot Sender on which the result of the command is sent.

//! Example: use a task to synchronize a u64 counter. Each task sends an “fetch
//! and increment” command. The counter value before the increment is sent over
//! the provided oneshot channel.

use tokio::sync::{mpsc, oneshot};
use Command::Increment;

enum Command {
    Increment,
    // Other commands can be added here
}

#[tokio::main]
async fn main() {
    let (cmd_tx, mut cmd_rx) = mpsc::channel::<(Command, oneshot::Sender<u64>)>(100);

    // Spawn a task to manage the counter
    let manager = tokio::spawn(async move {
        let mut counter: u64 = 0;

        while let Some((cmd, response)) = cmd_rx.recv().await {
            match cmd {
                Increment => {
                    let prev = counter;
                    counter += 1;
                    response.send(prev).unwrap();
                }
            }
        }
    });

    let mut join_handles = vec![];

    // Spawn tasks that will send the increment command.
    for i in 0..10 {
        let cmd_tx = cmd_tx.clone();

        join_handles.push(tokio::spawn(async move {
            let (resp_tx, resp_rx) = oneshot::channel();

            cmd_tx.send((Increment, resp_tx)).await.ok().unwrap();
            let res = resp_rx.await.unwrap();

            println!("task {i}: previous value = {}", res);
        }));
    }

    // The `rx` half of the channel returns `None` once **all** `tx` clones
    // drop. To ensure `None` is returned, drop the handle owned by the
    // current task. If this `tx` handle is not dropped, there will always
    // be a single outstanding `tx` handle.
    drop(cmd_tx);

    println!("main:tasks = {}", join_handles.len());

    // Wait for all tasks to complete
    for join_handle in join_handles.drain(..) {
        join_handle.await.unwrap();
    }

    println!("main:tasks = {}", join_handles.len());

    let _ = manager.await;
}
