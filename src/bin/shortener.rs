use std::process;

use clap::Parser;

use shortener::{Config, Logger, Shortener};

/// URL shortener server
#[derive(Parser)]
#[command(
  version,
  long_version = shortener::version_string(),
)]
struct Cli {
  /// Enable authentication for URL shortening
  #[arg(long)]
  auth: bool,

  /// Port to listen for HTTP requests
  #[arg(long, default_value_t = 8080)]
  listen_port: u16,

  /// Prefix to shortened URL, e.g. https://example.com/
  #[arg(long)]
  url_prefix: Option<String>,

  /// URL for main page of shortener; leave blank for default home page
  #[arg(long)]
  main_page: Option<String>,

  /// Length of shortened code
  #[arg(long, default_value_t = 6)]
  code_length: usize,

  /// Path to SQLite database for URL and API key storage
  #[arg(long, default_value = "shortener.db")]
  sqlite_db: String,

  /// Path to access log file
  #[arg(long, default_value = "access.log")]
  log_file: String,

  /// Trust X-Forwarded-For header from reverse proxy
  #[arg(long)]
  trust_proxy: bool,
}

impl From<Cli> for Config {
  fn from(cli: Cli) -> Config {
    let url_prefix = cli.url_prefix.unwrap_or_else(|| {
      if cli.listen_port == 80 {
        "http://localhost/".to_owned()
      } else {
        format!("http://localhost:{}/", cli.listen_port)
      }
    });
    Config {
      auth: cli.auth,
      listen_port: cli.listen_port,
      url_prefix,
      main_page: cli.main_page,
      code_length: cli.code_length,
      sqlite_db: cli.sqlite_db,
      log_file: cli.log_file,
      trust_proxy: cli.trust_proxy,
    }
  }
}

#[tokio::main]
async fn main() {
  let config: Config = Cli::parse().into();

  if let Err(error) = Logger::init(&config.log_file) {
    eprintln!("Failed to open log file: {}", error);
    process::exit(1);
  }

  log::info!("Config: {:?}", config);

  let shortener = match Shortener::new(config) {
    Ok(shortener) => shortener,
    Err(error) => {
      log::error!("Failed to create shortener: {}", error);
      process::exit(1);
    }
  };

  log::info!("Starting HTTP server");
  if let Err(error) = shortener.listen_and_serve().await {
    log::error!("{}", error);
    process::exit(1);
  }
}
