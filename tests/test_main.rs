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
fn test_set_get_update() {
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
