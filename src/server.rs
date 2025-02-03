use crate::db::{Db, Entry};
use crate::persistence::snapshot;
use std::net::{TcpListener, TcpStream};
use std::io::{BufRead, BufReader, Write};
use std::sync::mpsc::{Sender, channel};
use std::thread;
use std::time::{Duration, SystemTime};

pub fn run_server(addr: &str, db: Db) {
    let listener = TcpListener::bind(addr).expect("Binding Error");
    println!("Server listening on {}", addr);

    // Communication channel for the AOF writer
    let (aof_tx, aof_rx) = channel::<String>();

    // Start the AOF thread
    thread::spawn(move || {
        crate::persistence::run_aof_writer(aof_rx);
    });

    // Thread Snapshot (toutes les 5 minutes)
    let snapshot_db = db.clone();
    thread::spawn(move || {
        loop {
            thread::sleep(Duration::from_secs(300));
            snapshot(&snapshot_db);
        }
    });

    // Thread de nettoyage TTL
    let ttl_db = db.clone();
    thread::spawn(move || {
        loop {
            thread::sleep(Duration::from_secs(1));
            let now = SystemTime::now();
            let mut db = ttl_db.lock().unwrap();
            db.retain(|_, entry| entry.expire_at.map_or(true, |exp| exp > now));
        }
    });

    // Acceptation des connexions
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let db = db.clone();
                let aof_tx = aof_tx.clone();
                thread::spawn(move || {
                    handle_client(stream, db, aof_tx);
                });
            },
            Err(e) => eprintln!("Erreur: {}", e),
        }
    }
}

pub fn handle_client(stream: TcpStream, db: Db, aof_tx: Sender<String>) {
    let mut reader = BufReader::new(&stream);
    let mut buffer = String::new();
    while reader.read_line(&mut buffer).unwrap() > 0 {
        let parts: Vec<&str> = buffer.trim().split_whitespace().collect();
        if parts.is_empty() {
            buffer.clear();
            continue;
        }
        match parts[0].to_uppercase().as_str() {
            "SET" => {
                // Syntaxe : SET key value [TTL secondes]
                if parts.len() < 3 {
                    writeln!(&stream, "ERR: Usage: SET key value [TTL secondes]").unwrap();
                } else {
                    let key = parts[1].to_string();
                    let value = parts[2].to_string();
                    let expire_at = if parts.len() == 5 && parts[3].to_uppercase() == "TTL" {
                        if let Ok(sec) = parts[4].parse::<u64>() {
                            Some(SystemTime::now() + Duration::from_secs(sec))
                        } else { None }
                    } else { None };
                    let entry = Entry { value: value.clone(), expire_at };
                    {
                        let mut db = db.lock().unwrap();
                        db.insert(key.clone(), entry);
                    }
                    let cmd = if let Some(exp) = expire_at {
                        let ts = exp.duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs();
                        format!("SET {} {} TTL {}", key, value, ts)
                    } else {
                        format!("SET {} {}", key, value)
                    };
                    aof_tx.send(cmd).unwrap();
                    writeln!(&stream, "OK").unwrap();
                }
            },
            "GET" => {
                if parts.len() < 2 {
                    writeln!(&stream, "ERR: Usage: GET key").unwrap();
                } else {
                    let key = parts[1];
                    let db = db.lock().unwrap();
                    if let Some(entry) = db.get(key) {
                        if let Some(exp) = entry.expire_at {
                            if SystemTime::now() > exp {
                                writeln!(&stream, "nil").unwrap();
                            } else {
                                writeln!(&stream, "{}", entry.value).unwrap();
                            }
                        } else {
                            writeln!(&stream, "{}", entry.value).unwrap();
                        }
                    } else {
                        writeln!(&stream, "nil").unwrap();
                    }
                }
            },
            "QUIT" => {
                writeln!(&stream, "BYE").unwrap();
                break;
            },
            _ => {
                writeln!(&stream, "ERR: Commande inconnue").unwrap();
            }
        }
        buffer.clear();
    }
}
