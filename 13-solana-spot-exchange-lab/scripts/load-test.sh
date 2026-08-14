#!/usr/bin/env bash
set -euo pipefail

SERVICE_URL="${SERVICE_URL:-http://127.0.0.1:3000}"
ITERATIONS="${ITERATIONS:-100}"
COMMAND_OFFSET="${COMMAND_OFFSET:-1000000}"
PRICE="${PRICE:-100}"
QUANTITY="${QUANTITY:-1}"
BUYER_TRADER_ID="${BUYER_TRADER_ID:-1}"
SELLER_TRADER_ID="${SELLER_TRADER_ID:-2}"
QUOTE_ASSET_ID="${QUOTE_ASSET_ID:-2}"
BASE_ASSET_ID="${BASE_ASSET_ID:-1}"

post_json() {
  local path="$1"
  local body="$2"

  curl -fsS \
    -X POST "${SERVICE_URL}${path}" \
    -H "content-type: application/json" \
    --data-raw "${body}" >/dev/null
}

get_ready() {
  curl -fsS "${SERVICE_URL}/ready" >/dev/null
}

if ! [[ "${ITERATIONS}" =~ ^[0-9]+$ ]] || [ "${ITERATIONS}" -eq 0 ]; then
  echo "ITERATIONS must be a positive integer" >&2
  exit 2
fi

if ! [[ "${COMMAND_OFFSET}" =~ ^[0-9]+$ ]]; then
  echo "COMMAND_OFFSET must be an integer" >&2
  exit 2
fi

get_ready

quote_deposit=$((ITERATIONS * PRICE * QUANTITY))
base_deposit=$((ITERATIONS * QUANTITY))

post_json "/deposits" \
  "{\"command_id\":${COMMAND_OFFSET},\"trader_id\":${BUYER_TRADER_ID},\"asset_id\":${QUOTE_ASSET_ID},\"amount\":${quote_deposit}}"
post_json "/deposits" \
  "{\"command_id\":$((COMMAND_OFFSET + 1)),\"trader_id\":${SELLER_TRADER_ID},\"asset_id\":${BASE_ASSET_ID},\"amount\":${base_deposit}}"

start_seconds="$(date +%s)"

for ((i = 0; i < ITERATIONS; i++)); do
  ask_command_id=$((COMMAND_OFFSET + 2 + (i * 2)))
  bid_command_id=$((COMMAND_OFFSET + 3 + (i * 2)))
  ask_order_id=$((COMMAND_OFFSET + 100000 + i))
  bid_order_id=$((COMMAND_OFFSET + 200000 + i))
  ask_sequence=$((COMMAND_OFFSET + 300000 + (i * 2)))
  bid_sequence=$((COMMAND_OFFSET + 300001 + (i * 2)))

  post_json "/orders" \
    "{\"command_id\":${ask_command_id},\"order_id\":${ask_order_id},\"trader_id\":${SELLER_TRADER_ID},\"market_id\":1,\"side\":\"ask\",\"price\":${PRICE},\"quantity\":${QUANTITY},\"sequence\":${ask_sequence}}"
  post_json "/orders" \
    "{\"command_id\":${bid_command_id},\"order_id\":${bid_order_id},\"trader_id\":${BUYER_TRADER_ID},\"market_id\":1,\"side\":\"bid\",\"price\":${PRICE},\"quantity\":${QUANTITY},\"sequence\":${bid_sequence}}"
done

end_seconds="$(date +%s)"
elapsed_seconds=$((end_seconds - start_seconds))
order_count=$((ITERATIONS * 2))

if [ "${elapsed_seconds}" -eq 0 ]; then
  echo "submitted ${order_count} orders in <1s"
else
  echo "submitted ${order_count} orders in ${elapsed_seconds}s"
  echo "approx_orders_per_second=$((order_count / elapsed_seconds))"
fi
