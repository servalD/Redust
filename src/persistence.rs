// src/persistence.rs
use crate::db::Db;
use std::fs::OpenOptions;
use std::io::{BufWriter, Write};
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::{Duration, Instant};
use std::thread::sleep;

pub fn snapshot(db: &Db) {
    use std::fs::File;
    use serde_json;
    let db = db.lock().unwrap();
    let file = File::create("snapshot.json").unwrap();
    serde_json::to_writer(file, &*db).unwrap();
    println!("Snapshot sauvegardé.");
}

pub fn run_aof_writer(rx: Receiver<String>) {
    let mut file = BufWriter::new(OpenOptions::new()
        .create(true)
        .append(true)
        .open("appendonly.aof")
        .unwrap());

    loop {
        let mut buffer = Vec::new();
        let start = Instant::now();

        // Buffer during 1 millisecond
        while start.elapsed() < Duration::from_millis(1) {
            match rx.try_recv() {
                Ok(cmd) => buffer.push(cmd),
                Err(TryRecvError::Empty) => sleep(Duration::from_micros(10)),
                Err(TryRecvError::Disconnected) => break,
            }
        }

        // Écriture groupée dans le fichier
        for cmd in buffer {
            writeln!(file, "{}", cmd).unwrap();
        }
        file.flush().unwrap();
    }
}
