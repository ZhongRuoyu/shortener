use rand::distr::{Alphanumeric, SampleString};

pub(crate) fn generate_code(length: usize) -> String {
  Alphanumeric.sample_string(&mut rand::rng(), length)
}

pub(crate) fn is_valid_code(code: &str) -> bool {
  !code.is_empty() && code.chars().all(is_valid_code_char)
}

fn is_valid_code_char(ch: char) -> bool {
  ch.is_ascii_alphanumeric() || ['-', '_'].contains(&ch)
}
