# Dedicated server image (the game client is not needed on a server).
FROM rust:1-slim AS build
WORKDIR /src
COPY . .
RUN cargo build --release -p toon_server

FROM debian:stable-slim
COPY --from=build /src/target/release/toon-server /usr/local/bin/toon-server
WORKDIR /data
EXPOSE 27015/udp 27015/tcp
# stdin stays open so the admin console keeps working with `docker attach`
ENTRYPOINT ["toon-server", "--config", "/data/server.json"]
