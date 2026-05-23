use base64::{Engine, engine::general_purpose::URL_SAFE};
use rand::RngExt;
use sha2::{Digest, Sha256};

const API_KEY_SIZE: usize = 32;

pub(crate) fn generate_api_key() -> String {
  let mut bytes = [0_u8; API_KEY_SIZE];
  rand::rng().fill(&mut bytes);
  URL_SAFE.encode(bytes)
}

pub(crate) fn hash_api_key(key: &str) -> Result<String, base64::DecodeError> {
  let bytes = URL_SAFE.decode(key)?;
  let hash = Sha256::digest(bytes);
  Ok(hex::encode(hash))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn generate_api_key_is_non_empty() {
    let key = generate_api_key();
    assert!(!key.is_empty());
  }

  #[test]
  fn hash_api_key_is_deterministic() {
    let key = generate_api_key();
    let hash1 = hash_api_key(&key).unwrap();
    let hash2 = hash_api_key(&key).unwrap();
    assert_eq!(hash1, hash2);
  }

  #[test]
  fn hash_api_key_differs_for_different_keys() {
    let key1 = generate_api_key();
    let key2 = generate_api_key();
    let hash1 = hash_api_key(&key1).unwrap();
    let hash2 = hash_api_key(&key2).unwrap();
    assert_ne!(hash1, hash2);
  }

  #[test]
  fn hash_api_key_is_64_hex_chars() {
    let key = generate_api_key();
    let hash = hash_api_key(&key).unwrap();
    assert_eq!(hash.len(), 64);
    assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
  }

  #[test]
  fn hash_api_key_errors_on_invalid_base64() {
    assert!(hash_api_key("not!valid!base64").is_err());
  }
}
