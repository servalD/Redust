// manual_tests.rs
use std::io::{self, BufRead, BufReader, Write};
use std::net::TcpStream;
use std::thread;
use std::time::Duration;

fn send_command(stream: &mut TcpStream, command: &str) -> io::Result<String> {
    // Envoie une commande et retourne la première ligne de réponse
    stream.write_all(format!("{}\n", command).as_bytes())?;
    stream.flush()?;
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut response = String::new();
    reader.read_line(&mut response)?;
    Ok(response.trim().to_string())
}

fn test_set_get(stream: &mut TcpStream) {
    println!("Test SET et GET");
    let resp = send_command(stream, "SET test_key test_value TTL 10").unwrap();
    println!("SET => {}", resp);
    let resp = send_command(stream, "GET test_key").unwrap();
    println!("GET => {}", resp);
}

fn test_update(stream: &mut TcpStream) {
    println!("Test UPDATE");
    // On commence par créer une clé
    let _ = send_command(stream, "SET update_key initial").unwrap();
    let resp = send_command(stream, "UPDATE update_key updated_value TTL 10").unwrap();
    println!("UPDATE => {}", resp);
    let resp = send_command(stream, "GET update_key").unwrap();
    println!("GET après UPDATE => {}", resp);
}

fn test_delete(stream: &mut TcpStream) {
    println!("Test DELETE");
    let _ = send_command(stream, "SET delete_key del_value").unwrap();
    let resp = send_command(stream, "DELETE delete_key").unwrap();
    println!("DELETE => {}", resp);
    let resp = send_command(stream, "GET delete_key").unwrap();
    println!("GET après DELETE => {}", resp);
}

fn test_ttl(stream: &mut TcpStream) {
    println!("Test TTL");
    let _ = send_command(stream, "SET ttl_key ttl_value TTL 2").unwrap();
    let resp = send_command(stream, "GET ttl_key").unwrap();
    println!("GET immédiat => {}", resp);
    println!("Attente de 3 secondes...");
    thread::sleep(Duration::from_secs(3));
    let resp = send_command(stream, "GET ttl_key").unwrap();
    println!("GET après expiration => {}", resp);
}

fn test_transaction(stream: &mut TcpStream) {
    println!("Test Transaction (MULTI/EXEC/DISCARD)");

    // Test de MULTI/EXEC
    let resp = send_command(stream, "MULTI").unwrap();
    println!("MULTI => {}", resp);
    let resp = send_command(stream, "SET trans_key1 value1").unwrap();
    println!("SET trans_key1 (QUEUE) => {}", resp);
    let resp = send_command(stream, "UPDATE trans_key1 value2 TTL 5").unwrap();
    println!("UPDATE trans_key1 with TTL 5(QUEUE) => {}", resp);
    let resp = send_command(stream, "EXEC").unwrap();
    println!("EXEC => {}", resp);
    let resp = send_command(stream, "GET trans_key1").unwrap();
    println!("GET trans_key1 => {}", resp);

    // Test de MULTI/DISCARD
    let resp = send_command(stream, "MULTI").unwrap();
    println!("MULTI => {}", resp);
    let _ = send_command(stream, "SET trans_key2 value1").unwrap();
    let resp = send_command(stream, "DISCARD").unwrap();
    println!("DISCARD => {}", resp);
    let resp = send_command(stream, "GET trans_key2").unwrap();
    println!("GET trans_key2 (après DISCARD) => {}", resp);
}

pub fn auto_test() -> TcpStream{
    let mut stream = TcpStream::connect("127.0.0.1:7878")
        .expect("Impossible de se connecter au serveur");
    println!("Connecté au serveur pour les tests.");

    test_set_get(&mut stream);
    test_update(&mut stream);
    test_delete(&mut stream);
    test_ttl(&mut stream);
    test_transaction(&mut stream);
    
    println!("Fin des tests.");
    stream
}
