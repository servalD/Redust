// src/server.rs
use crate::db::{Db, Entry};
use crate::persistence::snapshot;
use std::net::{TcpListener, TcpStream};
use std::io::{BufRead, BufReader, Write};
use std::sync::mpsc::Sender;
use std::thread;
use std::time::{Duration, SystemTime};
use std::collections::HashMap;

pub fn run_server(addr: &str, db: Db) {
    let listener = TcpListener::bind(addr).expect("Binding Error");
    println!("Server listening on {}", addr);

    // Communication channel pour l'AOF writer
    let (aof_tx, aof_rx) = std::sync::mpsc::channel::<String>();

    // Démarrage du thread AOF
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

    // Thread de nettoyage des TTL
    let ttl_db = db.clone();
    thread::spawn(move || {
        loop {
            thread::sleep(Duration::from_secs(1));
            let now = SystemTime::now();
            let mut db = ttl_db.lock().unwrap();
            db.retain(|_, entry| entry.expire_at.map_or(true, |exp| exp > now));
        }
    });

    // Acceptation des connexions entrantes
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

/// Gestion des clients avec support de transaction (MULTI/EXEC/DISCARD)
pub fn handle_client(stream: TcpStream, db: Db, aof_tx: Sender<String>) {
    let mut reader = BufReader::new(&stream);
    let mut buffer = String::new();

    // Variables de gestion de transaction
    let mut in_transaction = false;
    let mut transaction_queue: Vec<String> = Vec::new();

    loop {
        buffer.clear();
        let bytes_read = reader.read_line(&mut buffer).unwrap();
        if bytes_read == 0 {
            break; // fin de connexion
        }
        let trimmed = buffer.trim();
        if trimmed.is_empty() {
            continue;
        }

        // Si on est dans une transaction, on met en file d'attente ou on exécute selon la commande reçue
        if in_transaction {
            match trimmed.to_uppercase().as_str() {
                "EXEC" => {
                    let mut responses = Vec::new();
                    {
                        // On verrouille la base de données une seule fois pour exécuter la transaction
                        let mut db_guard = db.lock().unwrap();
                        for cmd in &transaction_queue {
                            let parts: Vec<&str> = cmd.split_whitespace().collect();
                            let response = process_command_parts(&parts, &mut db_guard, &aof_tx);
                            responses.push(response);
                        }
                    }
                    // Réinitialisation de l'état transactionnel
                    in_transaction = false;
                    transaction_queue.clear();
                    // Envoi des réponses de chaque commande de la transaction
                    for response in responses {
                        writeln!(&stream, "{}", response).unwrap();
                    }
                },
                "DISCARD" => {
                    in_transaction = false;
                    transaction_queue.clear();
                    writeln!(&stream, "OK").unwrap();
                },
                _ => {
                    // Toute autre commande est mise en file d'attente
                    transaction_queue.push(trimmed.to_string());
                    writeln!(&stream, "QUEUED").unwrap();
                }
            }
        } else {
            // En mode normal (pas de transaction)
            match trimmed.to_uppercase().as_str() {
                "MULTI" => {
                    in_transaction = true;
                    transaction_queue.clear();
                    writeln!(&stream, "OK").unwrap();
                },
                _ => {
                    let parts: Vec<&str> = trimmed.split_whitespace().collect();
                    let mut db_guard = db.lock().unwrap();
                    let response = process_command_parts(&parts, &mut db_guard, &aof_tx);
                    writeln!(&stream, "{}", response).unwrap();
                    if parts[0].to_uppercase() == "QUIT" {
                        break;
                    }
                }
            }
        }
    }
}


fn process_command_parts(parts: &[&str], db: &mut HashMap<String, Entry>, aof_tx: &Sender<String>) -> String {
    match parts[0].to_uppercase().as_str() {
        "SET" => {
            if parts.len() < 3 {
                return "ERR: Usage: SET key value [TTL seconds]".to_string();
            }
            let key = parts[1].to_string();
            if db.contains_key(&key) {
                return "ERR: La clé existe déjà.".to_string();
            }
            let value = parts[2].to_string();
            let expire_at = if parts.len() >= 5 && parts[3].to_uppercase() == "TTL" {
                if let Ok(sec) = parts[4].parse::<u64>() {
                    Some(SystemTime::now() + Duration::from_secs(sec))
                } else { None }
            } else { None };
            let entry = Entry { value: value.clone(), expire_at };
            db.insert(key.clone(), entry);
            let cmd = if let Some(exp) = expire_at {
                let ts = exp.duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs();
                format!("SET {} {} TTL {}", key, value, ts)
            } else {
                format!("SET {} {}", key, value)
            };
            aof_tx.send(cmd).unwrap();
            "OK".to_string()
        },
        "UPDATE" => {
            if parts.len() < 3 {
                return "ERR: Usage: UPDATE key value [TTL seconds]".to_string();
            }
            let key = parts[1].to_string();
            if !db.contains_key(&key) {
                return "ERR: La clé n'existe pas.".to_string();
            }
            let value = parts[2].to_string();
            let expire_at = if parts.len() >= 5 && parts[3].to_uppercase() == "TTL" {
                if let Ok(sec) = parts[4].parse::<u64>() {
                    Some(SystemTime::now() + Duration::from_secs(sec))
                } else { None }
            } else { None };
            let entry = Entry { value: value.clone(), expire_at };
            db.insert(key.clone(), entry);
            let cmd = if let Some(exp) = expire_at {
                let ts = exp.duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs();
                format!("UPDATE {} {} TTL {}", key, value, ts)
            } else {
                format!("UPDATE {} {}", key, value)
            };
            aof_tx.send(cmd).unwrap();
            "OK".to_string()
        },
        "GET" => {
            if parts.len() < 2 {
                return "ERR: Usage: GET key".to_string();
            }
            let key = parts[1];
            if let Some(entry) = db.get(key) {
                if let Some(exp) = entry.expire_at {
                    if SystemTime::now() > exp {
                        "nil".to_string()
                    } else {
                        entry.value.clone()
                    }
                } else {
                    entry.value.clone()
                }
            } else {
                "nil".to_string()
            }
        },
        "DELETE" => {
            if parts.len() < 2 {
                return "ERR: Usage: DELETE key".to_string();
            }
            let key = parts[1].to_string();
            if db.remove(&key).is_some() {
                aof_tx.send(format!("DELETE {}", key)).unwrap();
                "OK".to_string()
            } else {
                "ERR: La clé n'existe pas.".to_string()
            }
        },
        "QUIT" => "BYE".to_string(),
        _ => "ERR: Commande inconnue".to_string(),
    }
}
