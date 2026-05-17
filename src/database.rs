use std::path::Path;
use std::sync::{Mutex, MutexGuard};

use rusqlite::{Connection, OpenFlags, OptionalExtension, named_params};
use thiserror::Error;

use crate::api_key::{generate_api_key, hash_api_key};

#[derive(Debug, Error)]
pub enum DatabaseError {
  #[error("not found")]
  NotFound,
  #[error("username already in use")]
  UsernameAlreadyInUse,
  #[error("code already in use")]
  CodeAlreadyInUse,
  #[error("could not generate API key")]
  CouldNotGenerateApiKey,
  #[error(transparent)]
  ApiKey(#[from] base64::DecodeError),
  #[error(transparent)]
  Sqlite(#[from] rusqlite::Error),
}

pub struct Database {
  connection: Mutex<Connection>,
}

impl Database {
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

  pub fn init(&self) -> Result<(), DatabaseError> {
    let connection = self.connection();
    connection.execute_batch(
      r#"
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
      "#,
    )?;
    Ok(())
  }

  pub fn create_user(&self, username: &str) -> Result<(), DatabaseError> {
    let connection = self.connection();
    match connection.execute(
      r#"
        INSERT INTO Users(username, active)
        VALUES (:username, 1);
      "#,
      named_params! { ":username": username },
    ) {
      Ok(_) => Ok(()),
      Err(error) if is_constraint(&error) => {
        Err(DatabaseError::UsernameAlreadyInUse)
      }
      Err(error) => Err(error.into()),
    }
  }

  pub fn list_users(&self) -> Result<Vec<String>, DatabaseError> {
    let connection = self.connection();
    let mut statement = connection.prepare(
      r#"
        SELECT username
        FROM Users
        WHERE active = 1;
      "#,
    )?;
    let users = statement
      .query_map([], |row| row.get::<_, String>(0))?
      .collect::<Result<Vec<_>, _>>()?;
    Ok(users)
  }

  pub fn delete_user(&self, username: &str) -> Result<(), DatabaseError> {
    let connection = self.connection();
    connection.execute(
      r#"
        UPDATE ApiKeys
        SET active = 0
        WHERE username = :username
          AND active = 1;
      "#,
      named_params! { ":username": username },
    )?;

    let rows = connection.execute(
      r#"
        UPDATE Users
        SET active = 0
        WHERE username = :username
          AND active = 1;
      "#,
      named_params! { ":username": username },
    )?;
    if rows == 0 {
      return Err(DatabaseError::NotFound);
    }
    Ok(())
  }

  pub fn create_api_key(
    &self,
    username: &str,
  ) -> Result<String, DatabaseError> {
    let connection = self.connection();
    let active = connection
      .query_row(
        r#"
          SELECT active
          FROM Users
          WHERE username = :username
            AND active = 1;
        "#,
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
        r#"
          INSERT INTO ApiKeys(key_hash, username, active)
          VALUES (:key_hash, :username, 1);
        "#,
        named_params! { ":key_hash": key_hash, ":username": username },
      ) {
        Ok(_) => return Ok(key),
        Err(error) if is_constraint(&error) => continue,
        Err(error) => return Err(error.into()),
      }
    }

    Err(DatabaseError::CouldNotGenerateApiKey)
  }

  pub fn check_api_key(&self, key: &str) -> Result<String, DatabaseError> {
    let key_hash = hash_api_key(key)?;
    self.check_api_key_by_hash(&key_hash)
  }

  pub fn check_api_key_by_hash(
    &self,
    key_hash: &str,
  ) -> Result<String, DatabaseError> {
    let connection = self.connection();
    let username = connection
      .query_row(
        r#"
          SELECT ak.username
          FROM ApiKeys ak
          JOIN Users u
            ON ak.username = u.username
          WHERE ak.key_hash = :key_hash
            AND ak.active = 1
            AND u.active = 1;
        "#,
        named_params! { ":key_hash": key_hash },
        |row| row.get::<_, String>(0),
      )
      .optional()?;
    username.ok_or(DatabaseError::NotFound)
  }

  pub fn list_api_keys(
    &self,
    username: &str,
  ) -> Result<Vec<String>, DatabaseError> {
    let connection = self.connection();
    let mut statement = connection.prepare(
      r#"
        SELECT key_hash
        FROM ApiKeys
        WHERE username = :username
          AND active = 1;
      "#,
    )?;
    let keys = statement
      .query_map(named_params! { ":username": username }, |row| {
        row.get::<_, String>(0)
      })?
      .collect::<Result<Vec<_>, _>>()?;
    Ok(keys)
  }

  pub fn delete_api_key(&self, key: &str) -> Result<(), DatabaseError> {
    let key_hash = hash_api_key(key)?;
    self.delete_api_key_by_hash(&key_hash)
  }

  pub fn delete_api_key_by_hash(
    &self,
    key_hash: &str,
  ) -> Result<(), DatabaseError> {
    let connection = self.connection();
    let rows = connection.execute(
      r#"
        UPDATE ApiKeys
        SET active = 0
        WHERE key_hash = :key_hash
          AND active = 1;
      "#,
      named_params! { ":key_hash": key_hash },
    )?;
    if rows == 0 {
      return Err(DatabaseError::NotFound);
    }
    Ok(())
  }

  pub fn get_url(&self, code: &str) -> Result<String, DatabaseError> {
    let connection = self.connection();
    let url = connection
      .query_row(
        r#"
            SELECT url
            FROM Urls
            WHERE code = :code;
          "#,
        named_params! { ":code": code },
        |row| row.get::<_, String>(0),
      )
      .optional()?;
    let url = url.ok_or(DatabaseError::NotFound)?;

    connection.execute(
      r#"
        UPDATE Urls
        SET hits = hits + 1, last_hit = UNIXEPOCH()
        WHERE code = :code;
      "#,
      named_params! { ":code": code },
    )?;

    Ok(url)
  }

  pub fn create_code(
    &self,
    url: &str,
    code: &str,
    created_by: &str,
  ) -> Result<(), DatabaseError> {
    let connection = self.connection();
    match connection.execute(
      r#"
        INSERT INTO Urls(code, url, created_at, created_by, hits, last_hit)
        VALUES (:code, :url, UNIXEPOCH(), :created_by, 0, NULL);
      "#,
      named_params! { ":code": code, ":url": url, ":created_by": created_by },
    ) {
      Ok(_) => Ok(()),
      Err(error) if is_constraint(&error) => {
        Err(DatabaseError::CodeAlreadyInUse)
      }
      Err(error) => Err(error.into()),
    }
  }

  fn connection(&self) -> MutexGuard<'_, Connection> {
    self.connection.lock().expect("database mutex poisoned")
  }
}

fn is_constraint(error: &rusqlite::Error) -> bool {
  matches!(
    error,
    rusqlite::Error::SqliteFailure(sqlite_error, _)
      if sqlite_error.code == rusqlite::ErrorCode::ConstraintViolation
  )
}
