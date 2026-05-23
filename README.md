# shortener

`shortener` is a small URL shortener written in Rust.
It serves HTTP redirects, stores mappings in SQLite, and optionally protects
URL creation with API keys.

## Features

- Plain HTTP interface with no frontend dependency.
- SQLite-backed URL storage.
- Random alphanumeric codes with configurable length.
- Custom aliases using letters, digits, dashes (`-`), and underscores (`_`).
- Optional bearer-token authentication for URL creation.
- User and API key management through `shortener-key`.
- Access logging to stdout and a log file.
- Reverse-proxy support through `--trust-proxy`.
- Optional redirect from `/` to a configured main page.

## Installation

- Cargo

  ```sh
  cargo install shortener
  ```

  By default, `shortener` uses the system's SQLite library.
  If you don't have it installed, or want to use the bundled version, add
  `--features bundled-sqlite` to the `cargo install` command.

- Homebrew

  ```sh
  brew install zhongruoyu/tap/shortener
  ```

- Release binaries

  `shortener`'s GitHub releases come with pre-built binaries for
  Linux, macOS, and Windows.
  Download the binaries from
  [the latest release](https://github.com/ZhongRuoyu/shortener/releases/latest).

- Docker

  A Docker image is available on Docker Hub as
  [`zhongruoyu/shortener`](https://hub.docker.com/r/zhongruoyu/shortener),
  and on GitHub Container Registry as
  [`ghcr.io/zhongruoyu/shortener`](https://ghcr.io/zhongruoyu/shortener).
  Use the `latest` tag or a specific version tag like `v0.1.0` to track
  releases, and `main` to track the latest commit on the main branch.

  See [Run with Docker](#run-with-docker) for usage instructions with Docker.

## Usage

`shortener` comes with two executables:

- `shortener`: the HTTP server.
- `shortener-key`: the user and API key management CLI.

### Starting the server

Start a local instance:

```sh
shortener \
  --listen-port 8080 \
  --url-prefix http://localhost:8080/ \
  --sqlite-db shortener.db \
  --log-file access.log
```

Useful flags:

- `--auth`: require `Authorization: Bearer ...` for `POST` requests.
- `--main-page URL`: redirect `/` to a separate landing page.
- `--code-length N`: change generated code length. The default is `6`.
- `--trust-proxy`: use the first `X-Forwarded-For` address for logging.

See the full CLI help with:

```sh
shortener --help
shortener-key --help
```

### Creating and using short URLs

Create a short URL by sending the target URL as the plain-text request body:

```sh
curl -X POST http://localhost:8080/ -d 'https://example.com/some/long/path'
```

The response is the new shortened URL as plain text.

Create a custom alias by posting to `/<code>`:

```sh
curl -X POST http://localhost:8080/docs -d 'https://example.com/docs'
```

Open the generated short URL and the server returns a `302 Found` redirect to
the stored destination.

### Authentication and API keys

When `--auth` is enabled, only authenticated `POST` requests can create short
URLs, though `GET` requests for URL redirection remain unauthenticated.
Use `shortener-key` to manage users and API keys against the same SQLite
database:

```sh
shortener-key --database shortener.db create-user alice
shortener-key --database shortener.db create-key alice
```

Then create a short URL with the returned key:

```sh
curl -X POST http://localhost:8080/ \
  -H 'Authorization: Bearer MY_API_KEY' \
  -d 'https://example.com/some/long/path'
```

Other management commands include:

- `list-users`
- `delete-user <username>`
- `check-key <key-or-hash>`
- `list-keys <username>`
- `delete-key <key-or-hash>`

### Run with Docker

Run the server and persist the database and logs in a local directory:

```sh
mkdir -p data
docker run --rm \
  -p 8080:8080 \
  -v "$PWD/data:/data" \
  zhongruoyu/shortener \
  --listen-port 8080 \
  --url-prefix http://localhost:8080/ \
  --sqlite-db /data/shortener.db \
  --log-file /data/access.log
```

You can also run `shortener-key` inside the same image by overriding the
entrypoint:

```sh
docker run --rm \
  --entrypoint shortener-key \
  -v "$PWD/data:/data" \
  zhongruoyu/shortener \
  --database /data/shortener.db create-user alice
```

### Shell completions

Shell completions are available for `shortener` and `shortener-key`.
To enable them, add the relevant command to your shell's profile:

```sh
# Bash
source <(shortener completions bash)
source <(shortener-key completions bash)
# Zsh
source <(shortener completions zsh)
source <(shortener-key completions zsh)
# Fish
shortener completions fish | source
shortener-key completions fish | source
# PowerShell
shortener completions powershell | Out-String | Invoke-Expression
shortener-key completions powershell | Out-String | Invoke-Expression
```

## License

This project is licensed under the MIT License.
See [LICENSE](./LICENSE).
