use rand::distr::{Alphanumeric, SampleString};

/// Generate a random alphanumeric code of the specified length.
#[must_use]
pub fn generate_code(length: usize) -> String {
  Alphanumeric.sample_string(&mut rand::rng(), length)
}

/// Check if a code is non-empty and contains only valid characters
/// (letters, digits, hyphens, and underscores).
#[must_use]
pub fn is_valid_code(code: &str) -> bool {
  !code.is_empty() && code.chars().all(is_valid_code_char)
}

fn is_valid_code_char(ch: char) -> bool {
  ch.is_ascii_alphanumeric() || ['-', '_'].contains(&ch)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn generate_code_has_correct_length() {
    for length in [1, 5, 6, 10, 20] {
      let code = generate_code(length);
      assert_eq!(code.len(), length);
    }
  }

  #[test]
  fn generate_code_is_alphanumeric() {
    let code = generate_code(100);
    assert!(code.chars().all(|c| c.is_ascii_alphanumeric()));
  }

  #[test]
  fn is_valid_code_accepts_alphanumeric() {
    assert!(is_valid_code("abc123"));
    assert!(is_valid_code("ABC"));
    assert!(is_valid_code("123"));
  }

  #[test]
  fn is_valid_code_accepts_dash_and_underscore() {
    assert!(is_valid_code("hello-world"));
    assert!(is_valid_code("hello_world"));
    assert!(is_valid_code("a-b_c"));
  }

  #[test]
  fn is_valid_code_rejects_empty_string() {
    assert!(!is_valid_code(""));
  }

  #[test]
  fn is_valid_code_rejects_special_characters() {
    assert!(!is_valid_code("hello world"));
    assert!(!is_valid_code("hello/world"));
    assert!(!is_valid_code("hello.world"));
  }
}
