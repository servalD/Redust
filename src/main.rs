// src/main.rs
use redust::db::Db;
use redust::server::run_server;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

fn main() {
    let db: Db = Arc::new(Mutex::new(HashMap::new()));

    redust::persistence::restore_state(&db);

    
    run_server("127.0.0.1:7878", db);
}
