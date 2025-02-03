// src/db.rs
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;
use serde::{Serialize, Deserialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Entry {
    pub value: String,
    pub expire_at: Option<SystemTime>,
}

pub type Db = Arc<Mutex<HashMap<String, Entry>>>;
