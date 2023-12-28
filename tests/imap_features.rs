use std::process::Command;
use std::{thread, time};

static ONE_SEC: time::Duration = time::Duration::from_secs(1);

fn main() {
    let mut daemon = Command::new(env!("CARGO_BIN_EXE_aerogramme"))
        .arg("--dev")
        .arg("provider")
        .arg("daemon")
        .spawn()
        .expect("daemon should be started");

    thread::sleep(ONE_SEC);

    daemon.kill().expect("daemon should be killed");
}
