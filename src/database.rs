use std::path::Path;
use std::sync::{Mutex, MutexGuard};

use rusqlite::{Connection, OpenFlags, OptionalExtension, named_params};
use thiserror::Error;

use crate::api_key::{generate_api_key, hash_api_key};

/// Information about a shortened URL entry.
#[derive(Debug)]
pub struct UrlInfo {
  /// The short code.
  pub code: String,
  /// The target URL.
  pub url: String,
  /// Unix timestamp of when this code was created.
  pub created_at: i64,
  /// The creator of this code.
  pub created_by: String,
  /// Number of times this code has been accessed.
  pub hits: i64,
  /// Unix timestamp of the last access, or `None` if never accessed.
  pub last_hit: Option<i64>,
}

/// Errors that can arise from database operations.
#[derive(Debug, Error)]
pub enum DatabaseError {
  /// The requested resource does not exist.
  #[error("not found")]
  NotFound,
  /// A user with the given username already exists.
  #[error("username already in use")]
  UsernameAlreadyInUse,
  /// A short code with the given value already exists.
  #[error("code already in use")]
  CodeAlreadyInUse,
  /// A unique API key could not be generated after several attempts.
  #[error("could not generate API key")]
  CouldNotGenerateApiKey,
  /// A base64 decoding error while processing an API key.
  #[error(transparent)]
  ApiKey(#[from] base64::DecodeError),
  /// An underlying SQLite error.
  #[error(transparent)]
  Sqlite(#[from] rusqlite::Error),
}

/// Thread-safe wrapper around a SQLite connection for URL and API key storage.
pub struct Database {
  connection: Mutex<Connection>,
}

impl Database {
  /// Opens (and optionally creates) the database at `path`.
  ///
  /// Set `create` to `true` to allow creating a new database file;
  /// set it to `false` to fail if the file does not already exist.
  ///
  /// # Errors
  ///
  /// Returns [`DatabaseError::Sqlite`] if the file cannot be opened.
  pub fn new(
    path: impl AsRef<Path>,
    create: bool,
  ) -> Result<Self, DatabaseError> {
    let flags = if create {
      OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE
    } else {
      OpenFlags::SQLITE_OPEN_READ_WRITE
    };
    let connection = Connection::open_with_flags(path, flags)?;
    Ok(Self {
      connection: Mutex::new(connection),
    })
  }

  /// Creates the required tables if they do not already exist.
  ///
  /// # Errors
  ///
  /// Returns [`DatabaseError::Sqlite`] if the schema cannot be applied.
  pub fn init(&self) -> Result<(), DatabaseError> {
    let connection = self.connection();
    connection.execute_batch(
      r"
        PRAGMA foreign_keys = ON;

        CREATE TABLE IF NOT EXISTS Users(
          username TEXT PRIMARY KEY,
          active   INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS Urls(
          code       TEXT PRIMARY KEY,
          url        TEXT NOT NULL,
          created_at INTEGER NOT NULL,
          created_by TEXT NOT NULL,
          hits       INTEGER NOT NULL,
          last_hit   INTEGER
        );

        CREATE TABLE IF NOT EXISTS ApiKeys(
          key_hash TEXT PRIMARY KEY,
          username TEXT NOT NULL,
          active   INTEGER NOT NULL,
          FOREIGN KEY(username) REFERENCES Users(username)
        );
      ",
    )?;
    Ok(())
  }

  /// Creates a new active user with the given `username`.
  ///
  /// # Errors
  ///
  /// Returns [`DatabaseError::UsernameAlreadyInUse`] if the username
  /// is already taken, or [`DatabaseError::Sqlite`] on a database error.
  pub fn create_user(&self, username: &str) -> Result<(), DatabaseError> {
    let connection = self.connection();
    match connection.execute(
      r"
        INSERT INTO Users(username, active)
        VALUES (:username, 1);
      ",
      named_params! { ":username": username },
    ) {
      Ok(_) => Ok(()),
      Err(error) if is_constraint(&error) => {
        Err(DatabaseError::UsernameAlreadyInUse)
      }
      Err(error) => Err(error.into()),
    }
  }

  /// Returns the usernames of all active users.
  ///
  /// # Errors
  ///
  /// Returns [`DatabaseError::Sqlite`] on a database error.
  pub fn list_users(&self) -> Result<Vec<String>, DatabaseError> {
    let connection = self.connection();
    let mut statement = connection.prepare(
      r"
        SELECT username
        FROM Users
        WHERE active = 1;
      ",
    )?;
    let users = statement
      .query_map([], |row| row.get::<_, String>(0))?
      .collect::<Result<Vec<_>, _>>()?;
    Ok(users)
  }

  /// Deactivates the user with the given `username` and all of their
  /// API keys.
  ///
  /// # Errors
  ///
  /// Returns [`DatabaseError::NotFound`] if no active user with that
  /// name exists, or [`DatabaseError::Sqlite`] on a database error.
  pub fn delete_user(&self, username: &str) -> Result<(), DatabaseError> {
    let connection = self.connection();
    connection.execute(
      r"
        UPDATE ApiKeys
        SET active = 0
        WHERE username = :username
          AND active = 1;
      ",
      named_params! { ":username": username },
    )?;

    let rows = connection.execute(
      r"
        UPDATE Users
        SET active = 0
        WHERE username = :username
          AND active = 1;
      ",
      named_params! { ":username": username },
    )?;
    if rows == 0 {
      return Err(DatabaseError::NotFound);
    }
    Ok(())
  }

  /// Generates and stores a new API key for `username`, returning the
  /// plaintext key.
  ///
  /// # Errors
  ///
  /// Returns [`DatabaseError::NotFound`] if no active user with that
  /// name exists, [`DatabaseError::CouldNotGenerateApiKey`] if a
  /// unique key could not be generated, or [`DatabaseError::Sqlite`]
  /// on a database error.
  pub fn create_api_key(
    &self,
    username: &str,
  ) -> Result<String, DatabaseError> {
    let connection = self.connection();
    let active = connection
      .query_row(
        r"
          SELECT active
          FROM Users
          WHERE username = :username
            AND active = 1;
        ",
        named_params! { ":username": username },
        |row| row.get::<_, i64>(0),
      )
      .optional()?;
    if active.is_none() {
      return Err(DatabaseError::NotFound);
    }

    for _attempt in 0..3 {
      let key = generate_api_key();
      let key_hash = hash_api_key(&key)?;
      match connection.execute(
        r"
          INSERT INTO ApiKeys(key_hash, username, active)
          VALUES (:key_hash, :username, 1);
        ",
        named_params! { ":key_hash": key_hash, ":username": username },
      ) {
        Ok(_) => return Ok(key),
        Err(error) if is_constraint(&error) => (),
        Err(error) => return Err(error.into()),
      }
    }

    Err(DatabaseError::CouldNotGenerateApiKey)
  }

  /// Validates a plaintext API key and returns the associated username.
  ///
  /// # Errors
  ///
  /// Returns [`DatabaseError::NotFound`] if the key is invalid or
  /// belongs to an inactive user, [`DatabaseError::ApiKey`] if the key
  /// cannot be decoded, or [`DatabaseError::Sqlite`] on a database error.
  pub fn check_api_key(&self, key: &str) -> Result<String, DatabaseError> {
    let key_hash = hash_api_key(key)?;
    self.check_api_key_by_hash(&key_hash)
  }

  /// Validates an API key by its SHA-256 hex hash and returns the
  /// associated username.
  ///
  /// # Errors
  ///
  /// Returns [`DatabaseError::NotFound`] if the hash is not found or
  /// belongs to an inactive user, or [`DatabaseError::Sqlite`] on a
  /// database error.
  pub fn check_api_key_by_hash(
    &self,
    key_hash: &str,
  ) -> Result<String, DatabaseError> {
    let connection = self.connection();
    let username = connection
      .query_row(
        r"
          SELECT ak.username
          FROM ApiKeys ak
          JOIN Users u
            ON ak.username = u.username
          WHERE ak.key_hash = :key_hash
            AND ak.active = 1
            AND u.active = 1;
        ",
        named_params! { ":key_hash": key_hash },
        |row| row.get::<_, String>(0),
      )
      .optional()?;
    username.ok_or(DatabaseError::NotFound)
  }

  /// Returns the SHA-256 hex hashes of all active API keys for
  /// `username`.
  ///
  /// # Errors
  ///
  /// Returns [`DatabaseError::Sqlite`] on a database error.
  pub fn list_api_keys(
    &self,
    username: &str,
  ) -> Result<Vec<String>, DatabaseError> {
    let connection = self.connection();
    let mut statement = connection.prepare(
      r"
        SELECT key_hash
        FROM ApiKeys
        WHERE username = :username
          AND active = 1;
      ",
    )?;
    let keys = statement
      .query_map(named_params! { ":username": username }, |row| {
        row.get::<_, String>(0)
      })?
      .collect::<Result<Vec<_>, _>>()?;
    Ok(keys)
  }

  /// Deactivates an API key identified by its plaintext value.
  ///
  /// # Errors
  ///
  /// Returns [`DatabaseError::NotFound`] if the key is not found,
  /// [`DatabaseError::ApiKey`] if the key cannot be decoded, or
  /// [`DatabaseError::Sqlite`] on a database error.
  pub fn delete_api_key(&self, key: &str) -> Result<(), DatabaseError> {
    let key_hash = hash_api_key(key)?;
    self.delete_api_key_by_hash(&key_hash)
  }

  /// Deactivates an API key identified by its SHA-256 hex hash.
  ///
  /// # Errors
  ///
  /// Returns [`DatabaseError::NotFound`] if the hash is not found, or
  /// [`DatabaseError::Sqlite`] on a database error.
  pub fn delete_api_key_by_hash(
    &self,
    key_hash: &str,
  ) -> Result<(), DatabaseError> {
    let connection = self.connection();
    let rows = connection.execute(
      r"
        UPDATE ApiKeys
        SET active = 0
        WHERE key_hash = :key_hash
          AND active = 1;
      ",
      named_params! { ":key_hash": key_hash },
    )?;
    if rows == 0 {
      return Err(DatabaseError::NotFound);
    }
    Ok(())
  }

  /// Looks up the full URL for a short `code` and increments its hit
  /// counter.
  ///
  /// # Errors
  ///
  /// Returns [`DatabaseError::NotFound`] if the code does not exist, or
  /// [`DatabaseError::Sqlite`] on a database error.
  pub fn get_url(&self, code: &str) -> Result<String, DatabaseError> {
    let connection = self.connection();
    let url = connection
      .query_row(
        r"
            SELECT url
            FROM Urls
            WHERE code = :code;
          ",
        named_params! { ":code": code },
        |row| row.get::<_, String>(0),
      )
      .optional()?;
    let url = url.ok_or(DatabaseError::NotFound)?;

    connection.execute(
      r"
        UPDATE Urls
        SET hits = hits + 1, last_hit = UNIXEPOCH()
        WHERE code = :code;
      ",
      named_params! { ":code": code },
    )?;

    Ok(url)
  }

  /// Stores a new short `code` mapping to `url`, attributed to
  /// `created_by`.
  ///
  /// # Errors
  ///
  /// Returns [`DatabaseError::CodeAlreadyInUse`] if `code` is already
  /// taken, or [`DatabaseError::Sqlite`] on a database error.
  pub fn create_code(
    &self,
    url: &str,
    code: &str,
    created_by: &str,
  ) -> Result<(), DatabaseError> {
    let connection = self.connection();
    match connection.execute(
      r"
        INSERT INTO Urls(code, url, created_at, created_by, hits, last_hit)
        VALUES (:code, :url, UNIXEPOCH(), :created_by, 0, NULL);
      ",
      named_params! { ":code": code, ":url": url, ":created_by": created_by },
    ) {
      Ok(_) => Ok(()),
      Err(error) if is_constraint(&error) => {
        Err(DatabaseError::CodeAlreadyInUse)
      }
      Err(error) => Err(error.into()),
    }
  }

  /// Returns information about all shortened URLs, ordered by creation
  /// time.
  ///
  /// # Errors
  ///
  /// Returns [`DatabaseError::Sqlite`] on a database error.
  pub fn list_codes(&self) -> Result<Vec<UrlInfo>, DatabaseError> {
    let connection = self.connection();
    let mut statement = connection.prepare(
      r"
        SELECT code, url, created_at, created_by, hits, last_hit
        FROM Urls
        ORDER BY created_at ASC;
      ",
    )?;
    let infos = statement
      .query_map([], |row| {
        Ok(UrlInfo {
          code: row.get(0)?,
          url: row.get(1)?,
          created_at: row.get(2)?,
          created_by: row.get(3)?,
          hits: row.get(4)?,
          last_hit: row.get(5)?,
        })
      })?
      .collect::<Result<Vec<_>, _>>()?;
    Ok(infos)
  }

  /// Returns information about a shortened URL without incrementing its
  /// hit counter.
  ///
  /// # Errors
  ///
  /// Returns [`DatabaseError::NotFound`] if the code does not exist, or
  /// [`DatabaseError::Sqlite`] on a database error.
  pub fn get_url_info(&self, code: &str) -> Result<UrlInfo, DatabaseError> {
    let connection = self.connection();
    let info = connection
      .query_row(
        r"
          SELECT code, url, created_at, created_by, hits, last_hit
          FROM Urls
          WHERE code = :code;
        ",
        named_params! { ":code": code },
        |row| {
          Ok(UrlInfo {
            code: row.get(0)?,
            url: row.get(1)?,
            created_at: row.get(2)?,
            created_by: row.get(3)?,
            hits: row.get(4)?,
            last_hit: row.get(5)?,
          })
        },
      )
      .optional()?;
    info.ok_or(DatabaseError::NotFound)
  }

  /// Deletes the short code and its URL mapping from the database.
  ///
  /// # Errors
  ///
  /// Returns [`DatabaseError::NotFound`] if the code does not exist, or
  /// [`DatabaseError::Sqlite`] on a database error.
  pub fn delete_code(&self, code: &str) -> Result<(), DatabaseError> {
    let connection = self.connection();
    let rows = connection.execute(
      r"
        DELETE FROM Urls
        WHERE code = :code;
      ",
      named_params! { ":code": code },
    )?;
    if rows == 0 {
      return Err(DatabaseError::NotFound);
    }
    Ok(())
  }

  fn connection(&self) -> MutexGuard<'_, Connection> {
    self.connection.lock().expect("database mutex poisoned")
  }

  /// Creates a new in-memory database for testing.
  #[cfg(test)]
  fn new_in_memory() -> Result<Self, DatabaseError> {
    let connection = Connection::open_in_memory()?;
    Ok(Self {
      connection: Mutex::new(connection),
    })
  }
}

fn is_constraint(error: &rusqlite::Error) -> bool {
  matches!(
    error,
    rusqlite::Error::SqliteFailure(sqlite_error, _)
      if sqlite_error.code == rusqlite::ErrorCode::ConstraintViolation
  )
}

#[cfg(test)]
mod tests {
  use super::*;

  fn setup() -> Database {
    let db = Database::new_in_memory().unwrap();
    db.init().unwrap();
    db
  }

  #[test]
  fn create_and_list_users() {
    let db = setup();
    db.create_user("alice").unwrap();
    db.create_user("bob").unwrap();
    let users = db.list_users().unwrap();
    assert!(users.contains(&"alice".to_owned()));
    assert!(users.contains(&"bob".to_owned()));
  }

  #[test]
  fn create_duplicate_user_fails() {
    let db = setup();
    db.create_user("alice").unwrap();
    let err = db.create_user("alice").unwrap_err();
    assert!(matches!(err, DatabaseError::UsernameAlreadyInUse));
  }

  #[test]
  fn delete_user() {
    let db = setup();
    db.create_user("alice").unwrap();
    db.delete_user("alice").unwrap();
    let users = db.list_users().unwrap();
    assert!(!users.contains(&"alice".to_owned()));
  }

  #[test]
  fn delete_nonexistent_user_fails() {
    let db = setup();
    let err = db.delete_user("nobody").unwrap_err();
    assert!(matches!(err, DatabaseError::NotFound));
  }

  #[test]
  fn create_and_check_api_key() {
    let db = setup();
    db.create_user("alice").unwrap();
    let key = db.create_api_key("alice").unwrap();
    let username = db.check_api_key(&key).unwrap();
    assert_eq!(username, "alice");
  }

  #[test]
  fn check_api_key_by_hash() {
    let db = setup();
    db.create_user("alice").unwrap();
    let key = db.create_api_key("alice").unwrap();
    let hash = crate::api_key::hash_api_key(&key).unwrap();
    let username = db.check_api_key_by_hash(&hash).unwrap();
    assert_eq!(username, "alice");
  }

  #[test]
  fn create_api_key_for_nonexistent_user_fails() {
    let db = setup();
    let err = db.create_api_key("nobody").unwrap_err();
    assert!(matches!(err, DatabaseError::NotFound));
  }

  #[test]
  fn check_nonexistent_api_key_fails() {
    let db = setup();
    // Valid base64 but not in the database.
    let err = db
      .check_api_key("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=")
      .unwrap_err();
    assert!(matches!(err, DatabaseError::NotFound));
  }

  #[test]
  fn check_api_key_invalid_base64_fails() {
    let db = setup();
    let err = db.check_api_key("not!valid!base64").unwrap_err();
    assert!(matches!(err, DatabaseError::ApiKey(_)));
  }

  #[test]
  fn list_api_keys() {
    let db = setup();
    db.create_user("alice").unwrap();
    let key1 = db.create_api_key("alice").unwrap();
    let key2 = db.create_api_key("alice").unwrap();
    let hashes = db.list_api_keys("alice").unwrap();
    let hash1 = crate::api_key::hash_api_key(&key1).unwrap();
    let hash2 = crate::api_key::hash_api_key(&key2).unwrap();
    assert!(hashes.contains(&hash1));
    assert!(hashes.contains(&hash2));
  }

  #[test]
  fn delete_api_key() {
    let db = setup();
    db.create_user("alice").unwrap();
    let key = db.create_api_key("alice").unwrap();
    db.delete_api_key(&key).unwrap();
    let err = db.check_api_key(&key).unwrap_err();
    assert!(matches!(err, DatabaseError::NotFound));
  }

  #[test]
  fn delete_api_key_by_hash() {
    let db = setup();
    db.create_user("alice").unwrap();
    let key = db.create_api_key("alice").unwrap();
    let hash = crate::api_key::hash_api_key(&key).unwrap();
    db.delete_api_key_by_hash(&hash).unwrap();
    let err = db.check_api_key(&key).unwrap_err();
    assert!(matches!(err, DatabaseError::NotFound));
  }

  #[test]
  fn delete_nonexistent_api_key_fails() {
    let db = setup();
    let err = db.delete_api_key_by_hash("nonexistenthash").unwrap_err();
    assert!(matches!(err, DatabaseError::NotFound));
  }

  #[test]
  fn deleting_user_deactivates_api_keys() {
    let db = setup();
    db.create_user("alice").unwrap();
    let key = db.create_api_key("alice").unwrap();
    db.delete_user("alice").unwrap();
    let err = db.check_api_key(&key).unwrap_err();
    assert!(matches!(err, DatabaseError::NotFound));
  }

  #[test]
  fn create_and_get_url() {
    let db = setup();
    db.create_code("https://example.com", "abc", "alice")
      .unwrap();
    let url = db.get_url("abc").unwrap();
    assert_eq!(url, "https://example.com");
  }

  #[test]
  fn get_url_increments_hits() {
    let db = setup();
    db.create_code("https://example.com", "abc", "alice")
      .unwrap();
    db.get_url("abc").unwrap();
    db.get_url("abc").unwrap();
    let info = db.get_url_info("abc").unwrap();
    assert_eq!(info.hits, 2);
  }

  #[test]
  fn create_duplicate_code_fails() {
    let db = setup();
    db.create_code("https://example.com", "abc", "alice")
      .unwrap();
    let err = db
      .create_code("https://other.com", "abc", "alice")
      .unwrap_err();
    assert!(matches!(err, DatabaseError::CodeAlreadyInUse));
  }

  #[test]
  fn get_nonexistent_url_fails() {
    let db = setup();
    let err = db.get_url("nope").unwrap_err();
    assert!(matches!(err, DatabaseError::NotFound));
  }

  #[test]
  fn list_codes() {
    let db = setup();
    db.create_code("https://example.com", "abc", "alice")
      .unwrap();
    db.create_code("https://other.com", "def", "bob").unwrap();
    let codes = db.list_codes().unwrap();
    assert_eq!(codes.len(), 2);
    let code_values: Vec<_> = codes.iter().map(|c| c.code.as_str()).collect();
    assert!(code_values.contains(&"abc"));
    assert!(code_values.contains(&"def"));
  }

  #[test]
  fn list_codes_empty() {
    let db = setup();
    let codes = db.list_codes().unwrap();
    assert!(codes.is_empty());
  }

  #[test]
  fn get_url_info() {
    let db = setup();
    db.create_code("https://example.com", "abc", "alice")
      .unwrap();
    let info = db.get_url_info("abc").unwrap();
    assert_eq!(info.code, "abc");
    assert_eq!(info.url, "https://example.com");
    assert_eq!(info.created_by, "alice");
    assert_eq!(info.hits, 0);
    assert!(info.last_hit.is_none());
  }

  #[test]
  fn get_url_info_sets_last_hit_after_access() {
    let db = setup();
    db.create_code("https://example.com", "abc", "alice")
      .unwrap();
    db.get_url("abc").unwrap();
    let info = db.get_url_info("abc").unwrap();
    assert!(info.last_hit.is_some());
  }

  #[test]
  fn get_url_info_nonexistent_fails() {
    let db = setup();
    let err = db.get_url_info("nope").unwrap_err();
    assert!(matches!(err, DatabaseError::NotFound));
  }

  #[test]
  fn delete_code() {
    let db = setup();
    db.create_code("https://example.com", "abc", "alice")
      .unwrap();
    db.delete_code("abc").unwrap();
    let err = db.get_url("abc").unwrap_err();
    assert!(matches!(err, DatabaseError::NotFound));
  }

  #[test]
  fn delete_nonexistent_code_fails() {
    let db = setup();
    let err = db.delete_code("nope").unwrap_err();
    assert!(matches!(err, DatabaseError::NotFound));
  }
}
