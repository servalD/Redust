use std::io::{self, BufRead, BufReader, Write};
use std::time::Duration;

use redust_client::automated_test::auto_test;

fn main() {
    
    let mut stream = auto_test();

    // Définir un timeout court pour la lecture afin de récupérer plusieurs réponses
    stream.set_read_timeout(Some(Duration::from_millis(100))).unwrap();

    let mut reader = BufReader::new(stream.try_clone().unwrap());

    println!("Client connecté. Tapez vos commandes (ex : SET key value, GET key, MULTI, EXEC, DISCARD, UPDATE, DELETE, QUIT):");

    loop {
        let mut input = String::new();
        print!("> ");
        io::stdout().flush().unwrap();
        if io::stdin().read_line(&mut input).unwrap() == 0 {
            break;
        }
        let trimmed = input.trim();
        if trimmed.is_empty() {
            continue;
        }

        // Envoyer la commande au serveur
        stream.write_all(input.as_bytes()).unwrap();
        stream.flush().unwrap();

        // Lire toutes les réponses envoyées par le serveur
        loop {
            let mut response = String::new();
            match reader.read_line(&mut response) {
                Ok(0) => break, // fin de connexion
                Ok(_) => {
                    let resp = response.trim();
                    if !resp.is_empty() {
                        println!("Réponse: {}", resp);
                    }
                },
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock
                           || e.kind() == std::io::ErrorKind::TimedOut => {
                    // Timeout atteint, plus de réponses disponibles pour l'instant
                    break;
                },
                Err(e) => {
                    eprintln!("Erreur de lecture: {}", e);
                    break;
                }
            }
        }

        if trimmed.eq_ignore_ascii_case("QUIT") {
            break;
        }
    }
}
