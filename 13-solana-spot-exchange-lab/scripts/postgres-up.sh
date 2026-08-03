#!/usr/bin/env bash
set -euo pipefail

docker compose up -d postgres

until docker compose exec -T postgres pg_isready -U exchange -d exchange >/dev/null 2>&1; do
  sleep 1
done

echo "Postgres ready at postgres://exchange:exchange@127.0.0.1:55432/exchange"
