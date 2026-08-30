use std::thread;
use std::time::Duration;

fn main() {
    println!("Daemon OS Core starting...");

    loop {
        println!("Daemon OS Core is running.");

        thread::sleep(Duration::from_secs(10));
    }
}