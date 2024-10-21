#![allow(unused)]

struct HasDrop;

impl Drop for HasDrop {
    fn drop(&mut self) {
        println!("Dropping HasDrop!");
    }
}

struct DropGuard {
    one: HasDrop,
    two: HasDrop,
}

impl Drop for DropGuard {
    fn drop(&mut self) {
        println!("Dropping DropGuard!");
    }
}

fn main() {
    let _x = DropGuard {
        one: HasDrop,
        two: HasDrop,
    };
    println!("Running!");
    drop(_x);
    println!("dropped")
}
