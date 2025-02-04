// src/persistence.rs
use crate::db::{Db, Entry};
use serde_json;
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::{Duration, Instant, SystemTime};
use std::thread::sleep;

pub fn snapshot(db: &Db) {
    let db = db.lock().unwrap();
    let file = File::create("snapshot.json").unwrap();
    serde_json::to_writer(file, &*db).unwrap();
    println!("Snapshot sauvegardé.");
}

pub fn restore_state(db: &Db) {
    if let Ok(file) = File::open("snapshot.json") {
        if let Ok(snapshot_data) = serde_json::from_reader::<_, HashMap<String, Entry>>(file) {
            let mut db_lock = db.lock().unwrap();
            *db_lock = snapshot_data;
            println!("Snapshot chargé avec succès.");
        } else {
            eprintln!("Erreur lors de la lecture du snapshot.");
        }
    } else {
        println!("Aucun snapshot trouvé.");
    }

    if let Ok(file) = File::open("appendonly.aof") {
        let reader = BufReader::new(file);
        for line in reader.lines() {
            if let Ok(cmd_line) = line {
                apply_command(&cmd_line, db);
            }
        }
        println!("AOF appliqué avec succès.");
    } else {
        println!("Aucun AOF trouvé.");
    }
}

fn apply_command(command: &str, db: &Db) {
    let parts: Vec<&str> = command.split_whitespace().collect();
    if parts.is_empty() {
        return;
    }

    match parts[0].to_uppercase().as_str() {
        "SET" | "UPDATE" => {
            if parts.len() >= 3 {
                let key = parts[1].to_string();
                let value = parts[2].to_string();
                let expire_at = if parts.len() >= 5 && parts[3].to_uppercase() == "TTL" {
                    if let Ok(ts) = parts[4].parse::<u64>() {
                        Some(SystemTime::UNIX_EPOCH + Duration::from_secs(ts))
                    } else {
                        None
                    }
                } else {
                    None
                };
                let entry = Entry { value, expire_at };
                let mut db_lock = db.lock().unwrap();
                db_lock.insert(key, entry);
            }
        },
        "DELETE" => {
            if parts.len() >= 2 {
                let key = parts[1].to_string();
                let mut db_lock = db.lock().unwrap();
                db_lock.remove(&key);
            }
        },
        _ => {
        }
    }
}

pub fn run_aof_writer(rx: Receiver<String>) {
    let mut file = BufWriter::new(
        OpenOptions::new()
            .create(true)
            .append(true)
            .open("appendonly.aof")
            .unwrap(),
    );

    loop {
        let mut buffer = Vec::new();
        let start = Instant::now();

        // Buffer pendant 1 milliseconde
        while start.elapsed() < Duration::from_millis(1) {
            match rx.try_recv() {
                Ok(cmd) => buffer.push(cmd),
                Err(TryRecvError::Empty) => sleep(Duration::from_micros(10)),
                Err(TryRecvError::Disconnected) => break,
            }
        }

        for cmd in buffer {
            writeln!(file, "{}", cmd).unwrap();
        }
        file.flush().unwrap();
    }
}
