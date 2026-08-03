#!/usr/bin/env bash
set -euo pipefail

export DATABASE_URL="${DATABASE_URL:-postgres://exchange:exchange@127.0.0.1:55432/exchange}"

scripts/postgres-up.sh

cargo test -p persistence -- --ignored
cargo test -p exchange-service app_with_postgres_boots_and_reports_ready -- --ignored
