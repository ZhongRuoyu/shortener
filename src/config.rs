/// Runtime configuration for the shortener server.
#[derive(Clone, Debug)]
pub struct Config {
  /// Whether authentication is required for creating short URLs.
  pub auth: bool,
  /// TCP port the HTTP server listens on.
  pub listen_port: u16,
  /// URL prefix prepended to short codes, e.g. `https://example.com/`.
  pub url_prefix: String,
  /// Optional URL to redirect the root path (`/`) to.
  pub main_page: Option<String>,
  /// Number of random characters in a generated short code.
  pub code_length: usize,
  /// Filesystem path to the SQLite database file.
  pub database: String,
  /// Filesystem path to the access log file.
  pub log_file: String,
  /// Whether to trust `X-Forwarded-For` headers from a reverse proxy.
  pub trust_proxy: bool,
}
