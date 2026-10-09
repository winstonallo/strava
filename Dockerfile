FROM rust:1 AS build
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src src
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=build /app/target/release/strava /usr/local/bin/strava
# refresh_token lives here; mount a volume so rotated tokens persist
WORKDIR /data
EXPOSE 3000
CMD ["strava"]
