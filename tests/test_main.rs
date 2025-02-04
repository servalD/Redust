// tests/test_main.rs
use redust::db::{Db, Entry};
use redust::server;
use redust::persistence::snapshot;
use std::net::{TcpListener, TcpStream};
use std::io::{BufRead, BufReader, Write};
use std::sync::{Arc, Mutex, mpsc};
use std::collections::HashMap;
use std::thread;
use std::time::{Duration, SystemTime};
use redust::persistence;
use std::fs::{remove_file, OpenOptions};

fn start_test_server() -> std::net::SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let db: Db = Arc::new(Mutex::new(HashMap::new()));
    let (aof_tx, aof_rx) = mpsc::channel::<String>();

    // AOF writer pour les tests
    thread::spawn(move || {
        use std::fs::OpenOptions;
        use std::io::{BufWriter, Write};
        let mut file = BufWriter::new(OpenOptions::new()
            .create(true)
            .append(true)
            .open("test_appendonly.aof")
            .unwrap());
        while let Ok(cmd) = aof_rx.recv() {
            writeln!(file, "{}", cmd).unwrap();
            file.flush().unwrap();
            thread::sleep(Duration::from_millis(1));
        }
    });

    // Thread TTL
    {
        let ttl_db = db.clone();
        thread::spawn(move || {
            loop {
                thread::sleep(Duration::from_secs(1));
                let now = SystemTime::now();
                let mut db = ttl_db.lock().unwrap();
                db.retain(|_, entry| entry.expire_at.map_or(true, |exp| exp > now));
            }
        });
    }

    // Lancement du serveur test
    thread::spawn(move || {
        for stream in listener.incoming() {
            let stream = stream.unwrap();
            let db = db.clone();
            let aof_tx = aof_tx.clone();
            thread::spawn(move || {
                server::handle_client(stream, db, aof_tx);
            });
        }
    });
    addr
}

#[test]
fn test_set_get_update_delete() {
    let addr = start_test_server();
    thread::sleep(Duration::from_millis(100));
    let mut stream = TcpStream::connect(addr).unwrap();
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut resp = String::new();

    writeln!(stream, "SET mykey myvalue").unwrap();
    reader.read_line(&mut resp).unwrap();
    assert_eq!(resp.trim(), "OK");

    resp.clear();

    writeln!(stream, "GET mykey").unwrap();
    reader.read_line(&mut resp).unwrap();
    assert_eq!(resp.trim(), "myvalue");

    resp.clear();

    writeln!(stream, "UPDATE mykey updatedvalue").unwrap();
    reader.read_line(&mut resp).unwrap();
    assert_eq!(resp.trim(), "OK");

    resp.clear();

    writeln!(stream, "GET mykey").unwrap();
    reader.read_line(&mut resp).unwrap();
    assert_eq!(resp.trim(), "updatedvalue");

    resp.clear();

    writeln!(stream, "DELETE mykey").unwrap();
    reader.read_line(&mut resp).unwrap();
    assert_eq!(resp.trim(), "OK");

    resp.clear();

    writeln!(stream, "GET mykey").unwrap();
    reader.read_line(&mut resp).unwrap();
    assert_eq!(resp.trim(), "nil");
}

#[test]
fn test_ttl_expiration() {
    let addr = start_test_server();
    thread::sleep(Duration::from_millis(100));
    let mut stream = TcpStream::connect(addr).unwrap();
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut resp = String::new();

    writeln!(stream, "SET tempkey tempvalue TTL 2").unwrap();
    reader.read_line(&mut resp).unwrap();
    assert_eq!(resp.trim(), "OK");

    resp.clear();
    writeln!(stream, "GET tempkey").unwrap();
    reader.read_line(&mut resp).unwrap();
    assert_eq!(resp.trim(), "tempvalue");

    thread::sleep(Duration::from_secs(3));
    resp.clear();
    writeln!(stream, "GET tempkey").unwrap();
    reader.read_line(&mut resp).unwrap();
    assert_eq!(resp.trim(), "nil");
}

#[test]
fn test_quit() {
    let addr = start_test_server();
    thread::sleep(Duration::from_millis(100));
    let mut stream = TcpStream::connect(addr).unwrap();
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut resp = String::new();

    writeln!(stream, "QUIT").unwrap();
    reader.read_line(&mut resp).unwrap();
    assert_eq!(resp.trim(), "BYE");
}

#[test]
fn test_snapshot() {
    let db: Db = Arc::new(Mutex::new(HashMap::new()));
    {
        let mut db_lock = db.lock().unwrap();
        db_lock.insert("snapkey".to_string(), Entry { value: "snapvalue".to_string(), expire_at: None });
    }
    snapshot(&db);
    use std::fs::File;
    use serde_json;
    let file = File::open("snapshot.json").unwrap();
    let loaded: HashMap<String, Entry> = serde_json::from_reader(file).unwrap();
    assert!(loaded.contains_key("snapkey"));
    assert_eq!(loaded.get("snapkey").unwrap().value, "snapvalue");
}

#[test]
fn test_transaction_exec() {
    let addr = start_test_server();
    thread::sleep(Duration::from_millis(100));
    let mut stream = TcpStream::connect(addr).unwrap();
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut resp = String::new();

    writeln!(stream, "MULTI").unwrap();
    reader.read_line(&mut resp).unwrap();
    assert_eq!(resp.trim(), "OK");
    resp.clear();

    writeln!(stream, "SET txkey txvalue").unwrap();
    reader.read_line(&mut resp).unwrap();
    assert_eq!(resp.trim(), "QUEUED");
    resp.clear();

    writeln!(stream, "GET txkey").unwrap();
    reader.read_line(&mut resp).unwrap();
    assert_eq!(resp.trim(), "QUEUED");
    resp.clear();

    writeln!(stream, "EXEC").unwrap();
    reader.read_line(&mut resp).unwrap();
    assert_eq!(resp.trim(), "OK");
    resp.clear();
    reader.read_line(&mut resp).unwrap();
    assert_eq!(resp.trim(), "txvalue");
}

#[test]
fn test_transaction_discard() {
    let addr = start_test_server();
    thread::sleep(Duration::from_millis(100));
    let mut stream = TcpStream::connect(addr).unwrap();
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut resp = String::new();

    writeln!(stream, "MULTI").unwrap();
    reader.read_line(&mut resp).unwrap();
    assert_eq!(resp.trim(), "OK");
    resp.clear();

    writeln!(stream, "SET delkey delvalue").unwrap();
    reader.read_line(&mut resp).unwrap();
    assert_eq!(resp.trim(), "QUEUED");
    resp.clear();

    writeln!(stream, "DISCARD").unwrap();
    reader.read_line(&mut resp).unwrap();
    assert_eq!(resp.trim(), "OK");
    resp.clear();

    writeln!(stream, "GET delkey").unwrap();
    reader.read_line(&mut resp).unwrap();
    assert_eq!(resp.trim(), "nil");
}

#[test]
fn test_restore_state() {
    let _ = remove_file("snapshot.json");
    let _ = remove_file("appendonly.aof");

    // 1. Création d'une base de données initiale et insertion d'entrées.
    let db: Db = Arc::new(Mutex::new(HashMap::new()));
    {
        let mut db_lock = db.lock().unwrap();
        db_lock.insert("key1".to_string(), Entry { value: "value1".to_string(), expire_at: None });
        db_lock.insert("key2".to_string(), Entry { value: "value2".to_string(), expire_at: None });
    }
    
    persistence::snapshot(&db);

    // 2. Simuler des opérations post-snapshot en écrivant directement dans l'AOF.
    {
        // Ouvrir l'AOF en mode append.
        let mut aof_file = OpenOptions::new()
            .create(true)
            .append(true)
            .open("appendonly.aof")
            .expect("Erreur lors de l'ouverture de l'AOF");
            
        writeln!(aof_file, "UPDATE key1 new_value1").expect("Erreur d'écriture UPDATE");
        writeln!(aof_file, "DELETE key2").expect("Erreur d'écriture DELETE");
        writeln!(aof_file, "SET key3 value3").expect("Erreur d'écriture SET");
        aof_file.flush().expect("Erreur lors du flush de l'AOF");
    }

    // Pour simuler un crash, on crée une nouvelle base vide.
    let new_db: Db = Arc::new(Mutex::new(HashMap::new()));

    persistence::restore_state(&new_db);

    let new_db_lock = new_db.lock().unwrap();

    assert_eq!(new_db_lock.get("key1").unwrap().value, "new_value1");

    assert!(new_db_lock.get("key2").is_none());

    assert_eq!(new_db_lock.get("key3").unwrap().value, "value3");

    let _ = remove_file("snapshot.json");
    let _ = remove_file("appendonly.aof");
}