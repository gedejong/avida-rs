FROM rust:1-bookworm AS build
COPY rust/avida-native /src
WORKDIR /src
RUN cargo build --release -p avida

FROM debian:bookworm-slim
COPY --from=build /src/target/release/avida /usr/local/bin/avida
COPY avida-core/support/config /config
WORKDIR /config
ENTRYPOINT ["/usr/local/bin/avida"]
