#[derive(Clone, Debug)]
pub struct Config {
  pub auth: bool,
  pub listen_port: u16,
  pub url_prefix: String,
  pub main_page: Option<String>,
  pub code_length: usize,
  pub sqlite_db: String,
  pub log_file: String,
  pub trust_proxy: bool,
}
