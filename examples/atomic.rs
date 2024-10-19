use std::sync::{
    atomic::{AtomicU32, Ordering},
    Arc,
};

use tokio::{
    task::{self, JoinSet},
    time::{sleep, Duration},
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let counter = Arc::new(AtomicU32::new(0));

    let mut tasks = JoinSet::new();
    for i in 0..10 {
        let counter = counter.clone();
        // let task = task::spawn(async move {
        tasks.spawn(async move {
            println!("task {i} spawned");
            if i == 3 {
                sleep(Duration::from_millis(500)).await;
            }
            if i == 8 {
                panic!("oops!")
            }
            let prev = counter.fetch_add(1, Ordering::SeqCst);
            println!("task={i} prev={prev} curr={}", prev + 1);
        });
        // tasks.push(task);
    }

    // for task in tasks {
    //     task.await?;
    // }

    while let Some(Ok(())) = tasks.join_next().await {}

    Ok(())
}

// One of many ouputs:
//
// task 0 spawned
// task=0 prev=0 curr=1
// task 1 spawned
// task=1 prev=1 curr=2
// task 3 spawned
// task 2 spawned
// task=2 prev=2 curr=3
// task 4 spawned
// task=4 prev=3 curr=4
// task=3 prev=4 curr=5
