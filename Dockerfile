FROM rust:1.73-slim-buster AS builder

WORKDIR /usr/src/wazzup-rust
COPY . .
RUN cargo install --path .

FROM debian:buster-slim

COPY --from=builder /usr/local/cargo/bin/wazzup-rust /usr/local/bin/wazzup-rust

CMD ["wazzup-rust"]