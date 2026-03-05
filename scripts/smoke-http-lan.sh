#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
FEATURES="${FEATURES:-llama-backend}"
PORT="${PORT:-18080}"
API_KEY="${API_KEY:-test-key}"
LAN_MODE="${LAN_MODE:-0}"
TLS_CERT="${TLS_CERT:-}"
TLS_KEY="${TLS_KEY:-}"
SCHEME="http"
if [[ -n "$TLS_CERT" && -n "$TLS_KEY" ]]; then
  SCHEME="https"
fi
BASE_URL="${SCHEME}://127.0.0.1:${PORT}"

cleanup() {
  if [[ -n "${SERVER_PID:-}" ]] && kill -0 "$SERVER_PID" >/dev/null 2>&1; then
    kill "$SERVER_PID" >/dev/null 2>&1 || true
    wait "$SERVER_PID" >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT

serve_args=(serve --port "$PORT" --api-key "$API_KEY")
if [[ "$LAN_MODE" == "1" ]]; then
  serve_args+=(--lan)
fi
if [[ -n "$TLS_CERT" && -n "$TLS_KEY" ]]; then
  serve_args+=(--tls-cert "$TLS_CERT" --tls-key "$TLS_KEY")
fi

cargo run -p mai --features "$FEATURES" -- "${serve_args[@]}" >/tmp/mai-serve.log 2>&1 &
SERVER_PID=$!

echo "started server pid=$SERVER_PID"

for _ in $(seq 1 30); do
  if [[ "$SCHEME" == "https" ]]; then
    probe_args=(-fsS -k)
  else
    probe_args=(-fsS)
  fi
  if curl "${probe_args[@]}" "$BASE_URL/health" >/dev/null 2>&1; then
    break
  fi
  sleep 1
done

curl_args=(-fsS)
if [[ "$SCHEME" == "https" ]]; then
  curl_args+=(-k)
fi

curl "${curl_args[@]}" "$BASE_URL/health" | sed -n '1,120p'

echo "\n== /metrics (auth) =="
curl "${curl_args[@]}" -H "Authorization: Bearer $API_KEY" "$BASE_URL/metrics" | sed -n '1,120p'

echo "\n== /v1/models (auth) =="
curl "${curl_args[@]}" -H "Authorization: Bearer $API_KEY" "$BASE_URL/v1/models" | sed -n '1,120p'

echo "\n== /v1/embeddings (auth) =="
curl "${curl_args[@]}" \
  -H "Authorization: Bearer $API_KEY" \
  -H "Content-Type: application/json" \
  -d '{"model":"dummy","input":"hello"}' \
  "$BASE_URL/v1/embeddings" | sed -n '1,120p'

echo "\nsmoke test complete"
