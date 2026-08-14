FROM rust:1.96-slim AS builder

WORKDIR /app
COPY . .
RUN cargo build --release -p exchange-service

FROM debian:bookworm-slim AS runtime

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/exchange-service /usr/local/bin/exchange-service

ENV EXCHANGE_HTTP_ADDR=0.0.0.0:3000
ENV EXCHANGE_BOOT_MODE=postgres

EXPOSE 3000
CMD ["exchange-service"]
