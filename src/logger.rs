use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::sync::Mutex;

use chrono::Local;
use log::{LevelFilter, Log, Metadata, Record};

/// A simple logger that writes timestamped messages to both stdout
/// and a file.
pub struct Logger {
  file: Mutex<File>,
}

impl Logger {
  /// Initializes the global logger, writing log messages to `path`.
  ///
  /// The log file is created if it does not exist and appended to if
  /// it does. On Unix systems the file is created with `0600`
  /// permissions. Returns an error if the file cannot be opened or the
  /// logger has already been set.
  pub fn init(path: &str) -> io::Result<()> {
    let mut options = OpenOptions::new();
    options.create(true).append(true).write(true);

    #[cfg(unix)]
    {
      use std::os::unix::fs::OpenOptionsExt;
      options.mode(0o600);
    }

    let logger = Box::new(Self {
      file: Mutex::new(options.open(path)?),
    });
    log::set_boxed_logger(logger)
      .map(|()| log::set_max_level(LevelFilter::Info))
      .map_err(io::Error::other)
  }
}

impl Log for Logger {
  fn enabled(&self, _metadata: &Metadata) -> bool {
    true
  }

  fn log(&self, record: &Record) {
    let line = format!("[{}] {}", timestamp(), record.args());
    println!("{}", line);
    if let Ok(mut file) = self.file.lock() {
      let _ = writeln!(file, "{}", line);
    }
  }

  fn flush(&self) {}
}

fn timestamp() -> String {
  Local::now().format("%Y-%m-%d %H:%M:%S%.6f").to_string()
}
