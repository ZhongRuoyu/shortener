# shorten

`shorten` is a small URL shortener written in Rust.
It serves HTTP redirects, stores mappings in SQLite, and optionally protects
URL creation with API keys.
The repository also includes `shortenkey`, a companion CLI for managing users
and API keys.

## Features

- Plain HTTP interface with no frontend dependency.
- SQLite-backed URL storage.
- Random alphanumeric codes with configurable length.
- Custom aliases using letters, digits, dashes (`-`), and underscores (`_`).
- Optional bearer-token authentication for URL creation.
- User and API key management through `shortenkey`.
- Access logging to stdout and a log file.
- Reverse-proxy support through `--trust-proxy`.
- Optional redirect from `/` to a configured main page.

## Installation

All installation methods come with two executables:

- `shorten`: the HTTP server.
- `shortenkey`: the user and API key management CLI.

### Homebrew

`shorten` can be installed on macOS and Linux with Homebrew:

```sh
brew install zhongruoyu/tap/shorten
```

### Release binaries

`shorten`'s GitHub releases come with pre-built binaries for Linux, macOS, and
Windows.
Download the binaries from
[the latest release](https://github.com/ZhongRuoyu/shorten/releases/latest).

### Cargo

Install `shorten` with Cargo as follows:

```sh
cargo install --locked --git https://github.com/ZhongRuoyu/shorten.git
```

By default, `shorten` uses the system's SQLite library.
If you don't have it installed, or want to use the bundled version, add
`--features bundled-sqlite` to the `cargo install` command.

### Docker

A Docker image is available on Docker Hub as
[`zhongruoyu/shorten`](https://hub.docker.com/r/zhongruoyu/shorten),
and on GitHub Container Registry as
[`ghcr.io/zhongruoyu/shorten`](https://ghcr.io/zhongruoyu/shorten).
Use the `latest` tag or a specific version tag like `v0.1.0` to track releases,
and `main` to track the latest commit on the main branch.

See ["Run with Docker"](#run-with-docker) for usage instructions with Docker.

## Usage

### Starting the server

Start a local instance:

```sh
shorten \
  --listen-port 8080 \
  --url-prefix http://localhost:8080/ \
  --sqlite-db shorten.db \
  --log-file access.log
```

Useful flags:

- `--auth`: require `Authorization: Bearer ...` for `POST` requests.
- `--main-page URL`: redirect `/` to a separate landing page.
- `--code-length N`: change generated code length. The default is `6`.
- `--trust-proxy`: use the first `X-Forwarded-For` address for logging.

See the full CLI help with:

```sh
shorten --help
shortenkey --help
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
Use `shortenkey` to manage users and API keys against the same SQLite database:

```sh
shortenkey --database shorten.db create-user alice
shortenkey --database shorten.db create-key alice
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
  zhongruoyu/shorten \
  --listen-port 8080 \
  --url-prefix http://localhost:8080/ \
  --sqlite-db /data/shorten.db \
  --log-file /data/access.log
```

You can also run `shortenkey` inside the same image by overriding the
entrypoint:

```sh
docker run --rm \
  --entrypoint shortenkey \
  -v "$PWD/data:/data" \
  zhongruoyu/shorten \
  --database /data/shorten.db create-user alice
```

## License

This project is licensed under the MIT License.
See [LICENSE](LICENSE).
