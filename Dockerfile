FROM rust:1.95.0-alpine AS build

RUN --mount=type=cache,target=/var/cache/apk \
  apk add git

WORKDIR /app
RUN \
  --mount=type=bind,source=.git,target=.git \
  --mount=type=bind,source=src,target=src \
  --mount=type=bind,source=build.rs,target=build.rs \
  --mount=type=bind,source=Cargo.toml,target=Cargo.toml \
  --mount=type=bind,source=Cargo.lock,target=Cargo.lock \
  --mount=type=cache,target=/app/target \
  --mount=type=cache,target=/usr/local/cargo/registry \
  --mount=type=cache,target=/usr/local/cargo/git/db \
  <<RUN
  set -ex
  cargo build --locked --release --features bundled-sqlite
  cp target/release/shorten /bin/shorten
  cp target/release/shortenkey /bin/shortenkey
RUN

FROM scratch
COPY --from=build /bin/shorten /bin/shorten
COPY --from=build /bin/shortenkey /bin/shortenkey
ENTRYPOINT ["/bin/shorten"]
