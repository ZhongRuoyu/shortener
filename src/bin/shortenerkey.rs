use std::process;

use clap::{Parser, Subcommand};

use shortener::{Database, DatabaseError};

/// Manage users and API keys for the URL shortener
#[derive(Parser)]
#[command(
  version,
  long_version = shortener::version_string(),
)]
struct Cli {
  /// Path to the SQLite database
  #[arg(short, long)]
  database: String,

  #[command(subcommand)]
  action: Action,
}

#[derive(Subcommand)]
enum Action {
  /// Create a new user
  CreateUser {
    /// Username to create
    username: String,
  },
  /// List all users
  ListUsers,
  /// Delete a user
  DeleteUser {
    /// Username to delete
    username: String,
  },
  /// Create an API key for a user
  CreateKey {
    /// Username to create a key for
    username: String,
  },
  /// Check an API key or key hash
  CheckKey {
    /// API key or key hash to check
    key: String,
  },
  /// List API keys for a user
  ListKeys {
    /// Username to list keys for
    username: String,
  },
  /// Delete an API key or key hash
  DeleteKey {
    /// API key or key hash to delete
    key: String,
  },
}

fn die(message: impl AsRef<str>) -> ! {
  eprintln!("{}", message.as_ref());
  process::exit(1);
}

fn main() {
  let cli = Cli::parse();

  let database = match Database::new(&cli.database, false) {
    Ok(database) => database,
    Err(error) => die(format!("Error opening database: {}", error)),
  };
  if let Err(error) = database.init() {
    die(format!("Error initializing database: {}", error));
  }

  match cli.action {
    Action::CreateUser { username } => create_user(&database, &username),
    Action::ListUsers => list_users(&database),
    Action::DeleteUser { username } => delete_user(&database, &username),
    Action::CreateKey { username } => create_key(&database, &username),
    Action::CheckKey { key } => check_key(&database, &key),
    Action::ListKeys { username } => list_keys(&database, &username),
    Action::DeleteKey { key } => delete_key(&database, &key),
  }
}

fn create_user(database: &Database, username: &str) {
  match database.create_user(username) {
    Ok(()) => println!("User created successfully"),
    Err(DatabaseError::UsernameAlreadyInUse) => die("User already exists"),
    Err(error) => die(format!("Error creating user: {}", error)),
  }
}

fn list_users(database: &Database) {
  match database.list_users() {
    Ok(users) => {
      for user in users {
        println!("{}", user);
      }
    }
    Err(error) => die(format!("Error listing users: {}", error)),
  }
}

fn delete_user(database: &Database, username: &str) {
  match database.delete_user(username) {
    Ok(()) => println!("User deleted successfully"),
    Err(DatabaseError::NotFound) => die("User not found"),
    Err(error) => die(format!("Error deleting user: {}", error)),
  }
}

fn create_key(database: &Database, username: &str) {
  match database.create_api_key(username) {
    Ok(key) => println!("{}", key),
    Err(DatabaseError::NotFound) => die("User not found"),
    Err(error) => die(format!("Error creating API key: {}", error)),
  }
}

fn check_key(database: &Database, key: &str) {
  match database.check_api_key(key) {
    Ok(username) => println!("Valid (user: {})", username),
    Err(DatabaseError::NotFound) => match database.check_api_key_by_hash(key) {
      Ok(username) => println!("Valid (user: {})", username),
      Err(DatabaseError::NotFound) => die("API key not valid"),
      Err(error) => die(format!("Error checking API key: {}", error)),
    },
    Err(error) => die(format!("Error checking API key: {}", error)),
  }
}

fn list_keys(database: &Database, username: &str) {
  match database.list_api_keys(username) {
    Ok(keys) => {
      for key in keys {
        println!("{}", key);
      }
    }
    Err(error) => die(format!("Error listing API keys: {}", error)),
  }
}

fn delete_key(database: &Database, key: &str) {
  match database.delete_api_key(key) {
    Ok(()) => println!("API key deleted successfully"),
    Err(DatabaseError::NotFound) => {
      match database.delete_api_key_by_hash(key) {
        Ok(()) => println!("API key deleted successfully"),
        Err(DatabaseError::NotFound) => die("API key not found"),
        Err(error) => die(format!("Error deleting API key: {}", error)),
      }
    }
    Err(error) => die(format!("Error deleting API key: {}", error)),
  }
}
