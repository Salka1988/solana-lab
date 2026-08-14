#!/usr/bin/env bash
set -euo pipefail

docker compose up --build -d exchange-service

until curl -fsS http://127.0.0.1:3000/ready >/dev/null; do
  sleep 1
done

echo "Exchange service ready at http://127.0.0.1:3000"
