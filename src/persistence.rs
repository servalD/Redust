// src/persistence.rs
use crate::db::Db;
use std::time::Duration;
use std::thread;
use std::sync::mpsc::Receiver;

pub fn snapshot(db: &Db) {
    use std::fs::File;
    use serde_json;
    let db = db.lock().unwrap();
    let file = File::create("snapshot.json").unwrap();
    serde_json::to_writer(file, &*db).unwrap();
    println!("Snapshot sauvegardé.");
}

pub fn run_aof_writer(rx: Receiver<String>) {
    use std::fs::OpenOptions;
    use std::io::{BufWriter, Write};
    let mut file = BufWriter::new(OpenOptions::new()
        .create(true)
        .append(true)
        .open("appendonly.aof")
        .unwrap());
    while let Ok(cmd) = rx.recv() {
        writeln!(file, "{}", cmd).unwrap();
        file.flush().unwrap();
        thread::sleep(Duration::from_millis(1));
    }
}
