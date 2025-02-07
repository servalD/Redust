# Redust – Un Mini Redis en Rust

Redust est une implémentation simplifiée d'un système de base de données en mémoire inspiré de Redis, écrite en Rust. Ce projet démontre les concepts de gestion de la concurrence, de persistance des données et de gestion des transactions dans un environnement multi-threads.

## Architecture

Le projet est organisé en plusieurs modules principaux :

### 1. Module **db**

- **Rôle** : Gérer la structure de stockage de la base de données.
- **Fonctionnalités** :
  - Définit la structure `Entry` qui contient une valeur (`String`) et une option `expire_at` (pour le TTL).
  - Utilise une `HashMap` pour stocker les paires clé/valeur.
  - Le type de la base (`Db`) est défini comme un `Arc<Mutex<HashMap<String, Entry>>>` pour permettre un accès en toute sécurité.

### 2. Module **persistence**

- **Rôle** : Assurer la persistance des données.
- **Fonctionnalités** :
  - **Snapshot** : Sauvegarde périodique de l'état complet de la base dans un fichier JSON (`snapshot.json`).
  - **AOF (Append-Only File)** : Enregistre chaque commande dans un fichier (`appendonly.aof`) afin de pouvoir rejouer les commandes lors d'une restauration.
  - **Restauration** : À l'initialisation, le système tente de recharger l'état de la base à partir du snapshot et de l'AOF.
  - **AOF Writer** : Utilise un thread dédié qui récupère les commandes via un canal pour les écrire dans le fichier AOF de manière groupée, optimisant ainsi les écritures sur disque.

### 3. Module **server**

- **Rôle** : Gérer les connexions clients via TCP et le traitement des commandes.
- **Fonctionnalités** :
  - **Serveur TCP** : Écoute sur une adresse (par exemple `127.0.0.1:7878`) et accepte les connexions entrantes.
  - **Traitement des commandes** : Gère les commandes standards (`SET`, `GET`, `UPDATE`, `DELETE`) ainsi que les commandes de transaction (`MULTI`, `EXEC`, `DISCARD`).
  - **Transactions** : Permet de mettre en file des commandes lors d'une transaction (`MULTI`), puis de les exécuter en une seule opération (`EXEC`) ou d'annuler la transaction (`DISCARD`).
  - **Nettoyage des TTL** : Un thread dédié parcourt la base toutes les secondes pour supprimer les entrées dont le temps d'expiration est dépassé.

### 4. Point d'entrée – **main**

- **Rôle** : Initialiser la base de données, restaurer l'état précédent et lancer le serveur.
- **Fonctionnalités** :
  - Création de la base de données vide.
  - Restauration de l'état via les modules de persistance.
  - Démarrage du serveur pour écouter les connexions clients.

## Fonctionnalités Clés

- **Base en mémoire** : Utilisation d'une `HashMap` partagée et protégée pour stocker les données.
- **Persistance hybride** : Combinaison d'un snapshot complet et d'un journal d'opérations (AOF) pour une restauration fine.
- **Gestion du TTL** : Suppression automatique des entrées expirées grâce à un thread dédié.
- **Transactions** : Support basique des transactions permettant de grouper plusieurs commandes en une seule opération atomique.
- **Concurrence** : Utilisation de threads et de mécanismes comme `Arc` et `Mutex` pour un accès sécurisé à la base.