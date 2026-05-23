use std::io;
use std::net::{Ipv6Addr, SocketAddr};
use std::sync::Arc;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::extract::{ConnectInfo, State};
use axum::http::header::{AUTHORIZATION, CONTENT_TYPE, LOCATION};
use axum::http::{HeaderMap, Method, StatusCode, Uri};
use axum::response::Response;
use percent_encoding::percent_decode_str;
use tokio::net::TcpListener;
use url::Url;

use crate::code::{generate_code, is_valid_code};
use crate::config::Config;
use crate::database::{Database, DatabaseError};

const MAX_BODY_SIZE: usize = 8192;

/// The URL shortener HTTP server.
pub struct Shortener {
  state: AppState,
}

#[derive(Clone)]
struct AppState {
  config: Arc<Config>,
  database: Arc<Database>,
}

impl Shortener {
  /// Creates a new `Shortener` from `config`, opening or creating the
  /// SQLite database and running schema initialization.
  ///
  /// # Errors
  ///
  /// Returns [`DatabaseError`] if the database cannot be opened or
  /// initialized.
  pub fn new(config: Config) -> Result<Self, DatabaseError> {
    let database = Database::new(&config.database, true)?;
    database.init()?;

    Ok(Self {
      state: AppState {
        config: Arc::new(config),
        database: Arc::new(database),
      },
    })
  }

  /// Binds the configured TCP port and starts serving HTTP requests.
  /// This future runs until the server encounters an I/O error.
  ///
  /// # Errors
  ///
  /// Returns an [`io::Error`] if the TCP listener cannot be bound or
  /// the server encounters a fatal I/O error while running.
  pub async fn listen_and_serve(self) -> io::Result<()> {
    let address = &SocketAddr::new(
      Ipv6Addr::UNSPECIFIED.into(),
      self.state.config.listen_port,
    );
    let app = Router::new()
      .fallback(handle_request)
      .with_state(self.state);
    let listener = TcpListener::bind(address).await?;
    axum::serve(
      listener,
      app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
  }
}

async fn handle_request(
  State(state): State<AppState>,
  ConnectInfo(remote_addr): ConnectInfo<SocketAddr>,
  method: Method,
  uri: Uri,
  headers: HeaderMap,
  body: Body,
) -> Response {
  match method {
    Method::GET => {
      if uri.path().is_empty() || uri.path() == "/" {
        homepage_handler(&state, remote_addr, &method, &uri, &headers)
      } else {
        redirect_handler(&state, remote_addr, &method, &uri, &headers)
      }
    }
    Method::POST => {
      create_code_handler(&state, remote_addr, &method, &uri, &headers, body)
        .await
    }
    _ => {
      let client_host = get_client_host(&state.config, remote_addr, &headers);
      log::info!("{client_host} {method} {uri} Method not allowed");
      http_error(StatusCode::METHOD_NOT_ALLOWED, "Method not allowed")
    }
  }
}

fn homepage_handler(
  state: &AppState,
  remote_addr: SocketAddr,
  method: &Method,
  uri: &Uri,
  headers: &HeaderMap,
) -> Response {
  let client_host = get_client_host(&state.config, remote_addr, headers);
  log::info!("{client_host} {method} {uri}");

  match &state.config.main_page {
    Some(main_page) => redirect_response(main_page),
    None => plain_response("hello, world\n"),
  }
}

fn redirect_handler(
  state: &AppState,
  remote_addr: SocketAddr,
  method: &Method,
  uri: &Uri,
  headers: &HeaderMap,
) -> Response {
  let code = code_from_path(uri.path());
  let client_host = get_client_host(&state.config, remote_addr, headers);

  match state.database.get_url(&code) {
    Ok(url) => {
      log::info!("{client_host} {method} {uri} => {url}");
      redirect_response(&url)
    }
    Err(DatabaseError::NotFound) => {
      log::info!("{client_host} {method} {uri} [Not found]");
      http_error(StatusCode::NOT_FOUND, "Not found")
    }
    Err(error) => {
      log::info!("{client_host} {method} {uri} [{error}]");
      http_error(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error")
    }
  }
}

async fn create_code_handler(
  state: &AppState,
  remote_addr: SocketAddr,
  method: &Method,
  uri: &Uri,
  headers: &HeaderMap,
  body: Body,
) -> Response {
  let custom_code = if uri.path().is_empty() || uri.path() == "/" {
    String::new()
  } else {
    code_from_path(uri.path())
  };

  let mut username = String::new();
  if state.config.auth {
    let auth_header = headers
      .get(AUTHORIZATION)
      .and_then(|value| value.to_str().ok())
      .unwrap_or_default();
    if !auth_header.starts_with("Bearer ") {
      let client_host = get_client_host(&state.config, remote_addr, headers);
      log::info!("{client_host} {method} {uri} [Missing credentials]");
      return http_error(StatusCode::UNAUTHORIZED, "Unauthorized");
    }

    if let Ok(owner) = state
      .database
      .check_api_key(&auth_header["Bearer ".len()..])
    {
      username = owner;
    } else {
      let client_host = get_client_host(&state.config, remote_addr, headers);
      log::info!("{client_host} {method} {uri} [Invalid credentials]");
      return http_error(StatusCode::UNAUTHORIZED, "Unauthorized");
    }
  }

  let client_host = get_client_host(&state.config, remote_addr, headers);
  let created_by = if state.config.auth {
    username.as_str()
  } else {
    client_host.as_str()
  };

  let Ok(body) = to_bytes(body, MAX_BODY_SIZE).await else {
    log::info!("{client_host} {method} {uri} [Request body too large]");
    return http_error(StatusCode::PAYLOAD_TOO_LARGE, "Request body too large");
  };
  let body = String::from_utf8_lossy(&body);
  let target_url = body.trim().to_owned();

  if !is_valid_http_url(&target_url) {
    log::info!("{client_host} {method} {uri} [Invalid URL]");
    return http_error(StatusCode::BAD_REQUEST, "Invalid URL");
  }

  if !custom_code.is_empty() && !is_valid_code(&custom_code) {
    log::info!("{client_host} {method} {uri} [Invalid code]");
    return http_error(StatusCode::BAD_REQUEST, "Invalid code");
  }

  let code = if custom_code.is_empty() {
    match create_generated_code(
      state,
      &target_url,
      created_by,
      &client_host,
      method,
      uri,
    ) {
      Some(code) => code,
      None => {
        return http_error(
          StatusCode::INTERNAL_SERVER_ERROR,
          "Internal server error",
        );
      }
    }
  } else {
    match state
      .database
      .create_code(&target_url, &custom_code, created_by)
    {
      Ok(()) => custom_code,
      Err(DatabaseError::CodeAlreadyInUse) => {
        log::info!("{client_host} {method} {uri} [Code already in use]");
        return http_error(StatusCode::CONFLICT, "Code already in use");
      }
      Err(error) => {
        log::info!("{client_host} {method} {uri} [{error}]");
        return http_error(
          StatusCode::INTERNAL_SERVER_ERROR,
          "Internal server error",
        );
      }
    }
  };

  let new_url = format!(
    "{url_prefix}{code}",
    url_prefix = state.config.url_prefix,
    code = code,
  );
  log::info!("{client_host} {method} {uri} ({target_url}) => {new_url}");
  plain_response(format!("{new_url}\n"))
}

fn create_generated_code(
  state: &AppState,
  target_url: &str,
  created_by: &str,
  client_host: &str,
  method: &Method,
  uri: &Uri,
) -> Option<String> {
  let mut code = String::new();

  for attempt in 0..3 {
    code = generate_code(state.config.code_length);
    match state.database.create_code(target_url, &code, created_by) {
      Ok(()) => break,
      Err(DatabaseError::CodeAlreadyInUse) => {
        log::info!(
          "{client_host} {method} {uri} [Attempt {attempt}: {code}: Code already in use]"
        );
        code.clear();
      }
      Err(error) => {
        log::info!(
          "{client_host} {method} {uri} [Attempt {attempt}: {code}: {error}]"
        );
        code.clear();
      }
    }
  }

  if code.is_empty() {
    log::info!("{client_host} {method} {uri} [Could not generate code]");
    return None;
  }

  Some(code)
}

fn is_valid_http_url(input: &str) -> bool {
  Url::parse(input).is_ok_and(|url| matches!(url.scheme(), "http" | "https"))
}

fn get_client_host(
  config: &Config,
  remote_addr: SocketAddr,
  headers: &HeaderMap,
) -> String {
  if !config.trust_proxy {
    return remote_addr.ip().to_string();
  }

  let forwarded_for = headers
    .get("x-forwarded-for")
    .and_then(|value| value.to_str().ok())
    .unwrap_or_default();
  let host = forwarded_for.split(',').next().unwrap_or_default().trim();
  if host.is_empty() {
    remote_addr.ip().to_string()
  } else {
    host.to_owned()
  }
}

fn code_from_path(path: &str) -> String {
  let code = path.strip_prefix("/").unwrap_or(path);
  percent_decode_str(code).decode_utf8_lossy().into_owned()
}

fn plain_response(body: impl Into<Body>) -> Response {
  Response::builder()
    .status(StatusCode::OK)
    .body(body.into())
    .expect("response should build")
}

fn http_error(status: StatusCode, message: &str) -> Response {
  Response::builder()
    .status(status)
    .header(CONTENT_TYPE, "text/plain; charset=utf-8")
    .header("X-Content-Type-Options", "nosniff")
    .body(Body::from(format!("{message}\n")))
    .expect("response should build")
}

fn redirect_response(location: &str) -> Response {
  Response::builder()
    .status(StatusCode::FOUND)
    .header(LOCATION, location)
    .body(Body::from(format!("{location}\n")))
    .expect("response should build")
}
