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
