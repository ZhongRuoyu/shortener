use std::error;
use std::io;
use std::process;

use chrono::{TimeZone, Utc};
use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::Shell;
use serde::Serialize;
use shortener::UrlInfo;
use tabled::Table;
use tabled::Tabled;
use url::Url;

use shortener::{Database, DatabaseError, generate_code, is_valid_code};

/// Manage shortened URLs
#[derive(Parser)]
#[command(
  version,
  long_version = shortener::version_string(),
)]
struct Cli {
  /// Path to SQLite database for URL storage
  #[arg(short, long, default_value = "shortener.db")]
  database: String,

  #[command(subcommand)]
  action: Action,
}

#[derive(Subcommand)]
enum Action {
  /// Create a short code (generates a code if none is provided)
  Create {
    /// Target URL to shorten
    url: String,

    /// Custom short code; generated if omitted
    code: Option<String>,

    /// Length of generated short codes
    #[arg(long, default_value_t = 6)]
    code_length: usize,

    /// Creator name recorded for the new short code (defaults to current system
    /// user)
    #[arg(long)]
    created_by: Option<String>,
  },

  /// List all short codes
  List {
    /// Output format for listing URLs
    #[arg(long, default_value = "table", value_enum)]
    format: ListFormat,
  },

  /// Get information about a short code
  Get {
    /// Short code to look up
    code: String,

    /// Output format for code information;
    /// if omitted, only the target URL is shown
    #[arg(long, value_enum)]
    format: Option<GetFormat>,
  },

  /// Delete a short code
  Delete {
    /// Short code to delete
    code: String,
  },

  /// Output a shell completion script to stdout
  Completions {
    /// Shell to generate completions for
    shell: Shell,
  },
}

#[derive(clap::ValueEnum, Clone)]
enum ListFormat {
  Table,
  Json,
  Csv,
}

#[derive(clap::ValueEnum, Clone)]
enum GetFormat {
  Plain,
  Json,
  Csv,
}

#[derive(Tabled)]
struct UrlInfoTabled {
  code: String,
  url: String,
  created_at: String,
  created_by: String,
  hits: i64,
  last_hit: String,
}

impl From<UrlInfo> for UrlInfoTabled {
  fn from(url_info: UrlInfo) -> Self {
    UrlInfoTabled {
      code: url_info.code,
      url: url_info.url,
      created_at: format_timestamp(url_info.created_at),
      created_by: url_info.created_by,
      hits: url_info.hits,
      last_hit: format_optional_timestamp(url_info.last_hit),
    }
  }
}

#[derive(Serialize)]
struct UrlInfoRaw {
  code: String,
  url: String,
  created_at: i64,
  created_by: String,
  hits: i64,
  last_hit: Option<i64>,
}

impl From<UrlInfo> for UrlInfoRaw {
  fn from(url_info: UrlInfo) -> Self {
    UrlInfoRaw {
      code: url_info.code,
      url: url_info.url,
      created_at: url_info.created_at,
      created_by: url_info.created_by,
      hits: url_info.hits,
      last_hit: url_info.last_hit,
    }
  }
}

fn die(message: impl AsRef<str>) -> ! {
  eprintln!("{message}", message = message.as_ref());
  process::exit(1);
}

fn format_timestamp(ts: i64) -> String {
  Utc.timestamp_opt(ts, 0).single().map_or_else(
    || ts.to_string(),
    |dt| dt.format("%Y-%m-%d %H:%M:%S UTC").to_string(),
  )
}

fn format_optional_timestamp(ts: Option<i64>) -> String {
  ts.map_or_else(|| "never".to_owned(), format_timestamp)
}

fn is_valid_http_url(input: &str) -> bool {
  Url::parse(input).is_ok_and(|url| matches!(url.scheme(), "http" | "https"))
}

fn default_created_by() -> String {
  std::env::var("USER")
    .or_else(|_| std::env::var("USERNAME"))
    .unwrap_or_else(|_| "unknown".to_owned())
}

fn main() {
  let cli = Cli::parse();

  if let Action::Completions { shell } = cli.action {
    clap_complete::generate(
      shell,
      &mut Cli::command(),
      "shortener-url",
      &mut io::stdout(),
    );
    return;
  }

  let db_path = cli.database;
  let database = match Database::new(&db_path, false) {
    Ok(database) => database,
    Err(error) => die(format!("Error opening database: {error}")),
  };
  if let Err(error) = database.init() {
    die(format!("Error initializing database: {error}"));
  }

  match cli.action {
    Action::Create {
      url,
      code: None,
      code_length,
      created_by,
    } => {
      create_generated(&database, &url, code_length, created_by);
    }
    Action::Create {
      url,
      code: Some(code),
      code_length: _,
      created_by,
    } => {
      create_custom(&database, &url, &code, created_by);
    }
    Action::List { format } => list_codes(&database, &format),
    Action::Get { code, format } => get_code(&database, &code, format.as_ref()),
    Action::Delete { code } => delete_code(&database, &code),
    Action::Completions { .. } => unreachable!(),
  }
}

fn create_generated(
  database: &Database,
  url: &str,
  code_length: usize,
  created_by: Option<String>,
) {
  if !is_valid_http_url(url) {
    die("Invalid URL");
  }

  let created_by = created_by.unwrap_or_else(default_created_by);
  for _ in 0..3 {
    let code = generate_code(code_length);
    match database.create_code(url, &code, created_by.as_str()) {
      Ok(()) => {
        println!("{code}");
        return;
      }
      Err(DatabaseError::CodeAlreadyInUse) => (),
      Err(error) => die(format!("Error creating code: {error}")),
    }
  }

  die("Could not generate a code; try again");
}

fn create_custom(
  database: &Database,
  url: &str,
  code: &str,
  created_by: Option<String>,
) {
  if !is_valid_http_url(url) {
    die("Invalid URL");
  }
  if !is_valid_code(code) {
    die("Invalid code");
  }

  let created_by = created_by.unwrap_or_else(default_created_by);
  match database.create_code(url, code, created_by.as_str()) {
    Ok(()) => println!("{code}"),
    Err(DatabaseError::CodeAlreadyInUse) => die("Code already in use"),
    Err(error) => die(format!("Error creating code: {error}")),
  }
}

fn list_codes(database: &Database, format: &ListFormat) {
  let codes = match database.list_codes() {
    Ok(codes) => codes,
    Err(error) => die(format!("Error listing codes: {error}")),
  };

  match format {
    ListFormat::Table => {
      let list: Vec<UrlInfoTabled> =
        codes.into_iter().map(UrlInfoTabled::from).collect();
      let table = Table::new(list);
      println!("{table}");
    }
    ListFormat::Json => {
      let list: Vec<UrlInfoRaw> =
        codes.into_iter().map(UrlInfoRaw::from).collect();
      let json = match serde_json::to_string_pretty(&list) {
        Ok(json) => json,
        Err(error) => die(format!("Error serializing JSON: {error}")),
      };
      println!("{json}");
    }
    ListFormat::Csv => {
      let mut wtr = csv::Writer::from_writer(Vec::new());
      match move || -> Result<String, Box<dyn error::Error>> {
        for info in codes {
          wtr.serialize(UrlInfoRaw::from(info))?;
        }
        wtr.flush()?;
        Ok(String::from_utf8(wtr.into_inner()?).unwrap_or_default())
      }() {
        Ok(csv) => print!("{csv}"),
        Err(error) => die(format!("Error writing CSV: {error}")),
      }
    }
  }
}

fn get_code(database: &Database, code: &str, format: Option<&GetFormat>) {
  let info = match database.get_url_info(code) {
    Ok(info) => info,
    Err(DatabaseError::NotFound) => die("Code not found"),
    Err(error) => die(format!("Error retrieving code: {error}")),
  };

  match format {
    None => {
      println!("{}", info.url);
    }
    Some(GetFormat::Plain) => {
      println!("Code:       {}", info.code);
      println!("URL:        {}", info.url);
      println!("Created at: {}", format_timestamp(info.created_at));
      println!("Created by: {}", info.created_by);
      println!("Hits:       {}", info.hits);
      println!("Last hit:   {}", format_optional_timestamp(info.last_hit));
    }
    Some(GetFormat::Json) => {
      let json = match serde_json::to_string_pretty(&UrlInfoRaw::from(info)) {
        Ok(json) => json,
        Err(error) => die(format!("Error serializing JSON: {error}")),
      };
      println!("{json}");
    }
    Some(GetFormat::Csv) => {
      let mut wtr = csv::Writer::from_writer(Vec::new());
      match move || -> Result<String, Box<dyn error::Error>> {
        wtr.serialize(UrlInfoRaw::from(info))?;
        wtr.flush()?;
        Ok(String::from_utf8(wtr.into_inner()?).unwrap_or_default())
      }() {
        Ok(csv) => print!("{csv}"),
        Err(error) => die(format!("Error writing CSV: {error}")),
      }
    }
  }
}

fn delete_code(database: &Database, code: &str) {
  match database.delete_code(code) {
    Ok(()) => println!("Code deleted successfully"),
    Err(DatabaseError::NotFound) => die("Code not found"),
    Err(error) => die(format!("Error deleting code: {error}")),
  }
}
