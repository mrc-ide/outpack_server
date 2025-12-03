FROM rust:latest AS builder
WORKDIR /usr/src/outpack_server
COPY . .
RUN cargo install --locked --path .

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y tini

COPY --from=builder /usr/local/cargo/bin/* /usr/local/bin/
COPY start /usr/local/bin
EXPOSE 8000
ENTRYPOINT ["/usr/bin/tini", "--", "start"]
