#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
ENV_FILE="$SCRIPT_DIR/.env"
EXAMPLE_FILE="$SCRIPT_DIR/.env.example"

# Simple arg parsing for maintenance commands (e.g., --stop)
STOP=0
for arg in "$@"; do
  case "$arg" in
    --stop|stop)
      STOP=1
      ;;
  esac
done

# Load .env if present; otherwise create from example
if [[ -f "$ENV_FILE" ]]; then
  set -a; source "$ENV_FILE"; set +a
else
  if [[ -f "$EXAMPLE_FILE" ]]; then
    cp "$EXAMPLE_FILE" "$ENV_FILE"
    echo "Created $ENV_FILE from template" >&2
    set -a; source "$ENV_FILE"; set +a
  fi
fi

hex() {
  if command -v openssl >/dev/null 2>&1; then
    openssl rand -hex "$1"
  else
    # Fallback: read from /dev/urandom
    od -vAn -N "$1" -tx1 /dev/urandom | tr -d ' \n'
  fi
}

append_env() { # key value
  if ! grep -qE "^$1=" "$ENV_FILE" 2>/dev/null; then
    echo "$1=$2" >> "$ENV_FILE"
  fi
}

# Required config (can be set via environment or .env)
WRPC_URL=${WRPC_URL:-${WRPC_URL:-}}
MAINNET=${MAINNET:-${MAINNET:-0}}
EPISODE_ID=${EPISODE_ID:-${EPISODE_ID:-42}}
MERCHANT_PORT=${MERCHANT_PORT:-${MERCHANT_PORT:-3000}}
WEBHOOK_PORT=${WEBHOOK_PORT:-${WEBHOOK_PORT:-9655}}
WATCHER_PORT=${WATCHER_PORT:-${WATCHER_PORT:-9591}}
GUARDIAN_PORT=${GUARDIAN_PORT:-${GUARDIAN_PORT:-9650}}
MERCHANT_DB_PATH=${MERCHANT_DB_PATH:-${MERCHANT_DB_PATH:-}}

# WRPC_URL optional: if empty, kdapp proxy uses built-in resolver pool

# Secrets: generate and persist if missing
API_KEY=${API_KEY:-${API_KEY:-}}
WEBHOOK_SECRET=${WEBHOOK_SECRET:-${WEBHOOK_SECRET:-}}
MERCHANT_SK=${MERCHANT_SK:-${MERCHANT_SK:-}}
KASPA_SK=${KASPA_SK:-${KASPA_SK:-}}

mkdir -p "$SCRIPT_DIR"
touch "$ENV_FILE"

# Load persisted .env so Windows/WSL use the same secrets
if [[ -f "$ENV_FILE" ]]; then
  # shellcheck disable=SC2046
  set -a; . "$ENV_FILE" 2>/dev/null; set +a
fi

# Normalize in case CRLF snuck in from Windows edits
API_KEY=${API_KEY%%[$'\r\n']}
WEBHOOK_SECRET=${WEBHOOK_SECRET%%[$'\r\n']}
MERCHANT_SK=${MERCHANT_SK%%[$'\r\n']}
KASPA_SK=${KASPA_SK%%[$'\r\n']}

if [[ -z "${API_KEY:-}" ]]; then API_KEY=$(hex 16); append_env API_KEY "$API_KEY"; fi
if [[ -z "${WEBHOOK_SECRET:-}" ]]; then WEBHOOK_SECRET=$(hex 32); append_env WEBHOOK_SECRET "$WEBHOOK_SECRET"; fi
if [[ -z "${MERCHANT_SK:-}" ]]; then MERCHANT_SK=$(hex 32); append_env MERCHANT_SK "$MERCHANT_SK"; fi
if [[ -z "${KASPA_SK:-}" ]]; then KASPA_SK=$(hex 32); append_env KASPA_SK "$KASPA_SK"; fi
# Choose a WSL/Linux-friendly default DB location if unset
if [[ -z "${MERCHANT_DB_PATH:-}" ]]; then
  DATA_HOME=${XDG_DATA_HOME:-"$HOME/.local/share"}
  MERCHANT_DB_PATH="$DATA_HOME/onlykas/merchant-live.db"
fi
mkdir -p "$(dirname "$MERCHANT_DB_PATH")"
append_env MERCHANT_DB_PATH "$MERCHANT_DB_PATH" >/dev/null 2>&1 || true
if [[ -n "${WRPC_URL:-}" && "$WRPC_URL" != "wss://node:port" ]]; then
  append_env WRPC_URL "$WRPC_URL" >/dev/null 2>&1 || true
fi
append_env MAINNET "$MAINNET" >/dev/null 2>&1 || true
append_env EPISODE_ID "$EPISODE_ID" >/dev/null 2>&1 || true
append_env MERCHANT_PORT "$MERCHANT_PORT" >/dev/null 2>&1 || true
append_env WEBHOOK_PORT "$WEBHOOK_PORT" >/dev/null 2>&1 || true
append_env WATCHER_PORT "$WATCHER_PORT" >/dev/null 2>&1 || true
append_env GUARDIAN_PORT "$GUARDIAN_PORT" >/dev/null 2>&1 || true

export MERCHANT_DB_PATH
export RUST_LOG=${RUST_LOG:-info,kdapp=info,kdapp_merchant=info}

NET_ARGS=()
[[ "$MAINNET" == "1" ]] && NET_ARGS+=("--mainnet")

LOG_PREFIX="$SCRIPT_DIR"

# Maintenance: stop previously started processes and exit
if [[ "$STOP" == "1" ]]; then
  pkill -f 'kdapp-merchant( |$)' 2>/dev/null || true
  pkill -f 'onlykas-tui( |$)' 2>/dev/null || true
  pkill -f 'guardian-service( |$)' 2>/dev/null || true
  # Best-effort: also stop cargo-wrapped processes if still around
  pkill -f 'cargo run -p kdapp-merchant' 2>/dev/null || true
  pkill -f 'cargo run -p onlykas-tui' 2>/dev/null || true
  pkill -f 'cargo run -p kdapp-guardian' 2>/dev/null || true
  echo "Stopped onlykas processes"
  exit 0
fi

# Start merchant server + proxy in one process (shared engine)
if [[ "${DEBUG:-0}" == "1" ]]; then
  cargo run -p kdapp-merchant -- serve-proxy \
    --bind 127.0.0.1:"$MERCHANT_PORT" \
    --episode-id "$EPISODE_ID" \
    --api-key "$API_KEY" \
    --merchant-private-key "$MERCHANT_SK" \
    --webhook-url "http://127.0.0.1:$WEBHOOK_PORT/hook" \
    --webhook-secret "$WEBHOOK_SECRET" \
    ${WRPC_URL:+--wrpc-url "$WRPC_URL"} \
    ${NET_ARGS[@]:-} \
    2>&1 | tee -a "$LOG_PREFIX/merchant-serve.log" &
else
  cargo run -p kdapp-merchant -- serve-proxy \
    --bind 127.0.0.1:"$MERCHANT_PORT" \
    --episode-id "$EPISODE_ID" \
    --api-key "$API_KEY" \
    --merchant-private-key "$MERCHANT_SK" \
    --webhook-url "http://127.0.0.1:$WEBHOOK_PORT/hook" \
    --webhook-secret "$WEBHOOK_SECRET" \
    ${WRPC_URL:+--wrpc-url "$WRPC_URL"} \
    ${NET_ARGS[@]:-} \
    > "$LOG_PREFIX/merchant-serve.out" 2> "$LOG_PREFIX/merchant-serve.err" &
fi
sleep 1

# Start watcher (UDP + HTTP metrics)
if [[ "${DEBUG:-0}" == "1" ]]; then
  cargo run -p kdapp-merchant -- watcher \
    --bind 127.0.0.1:9590 \
    --kaspa-private-key "$KASPA_SK" \
    ${WRPC_URL:+--wrpc-url "$WRPC_URL"} \
    ${NET_ARGS[@]:-} \
    --http-port "$WATCHER_PORT" \
    2>&1 | tee -a "$LOG_PREFIX/watcher.log" &
else
  cargo run -p kdapp-merchant -- watcher \
    --bind 127.0.0.1:9590 \
    --kaspa-private-key "$KASPA_SK" \
    ${WRPC_URL:+--wrpc-url "$WRPC_URL"} \
    ${NET_ARGS[@]:-} \
    --http-port "$WATCHER_PORT" \
    > "$LOG_PREFIX/watcher.out" 2> "$LOG_PREFIX/watcher.err" &
fi
sleep 1

# Start guardian (optional demo service)
if [[ "${DEBUG:-0}" == "1" ]]; then
  cargo run -p kdapp-guardian --bin guardian-service -- \
    --listen-addr 127.0.0.1:"$GUARDIAN_PORT" \
    --http-port "$GUARDIAN_PORT" \
    2>&1 | tee -a "$LOG_PREFIX/guardian.log" &
else
  cargo run -p kdapp-guardian --bin guardian-service -- \
    --listen-addr 127.0.0.1:"$GUARDIAN_PORT" \
    --http-port "$GUARDIAN_PORT" \
    > "$LOG_PREFIX/guardian.out" 2> "$LOG_PREFIX/guardian.err" &
fi
sleep 1

echo "API key:        $API_KEY"
echo "Webhook secret: $WEBHOOK_SECRET"
echo "Merchant SK:    $MERCHANT_SK"
echo "Kaspa SK:       $KASPA_SK"
echo "Episode ID:     $EPISODE_ID"
echo "Merchant URL:   http://127.0.0.1:$MERCHANT_PORT"
if [[ "${DEBUG:-0}" == "1" ]]; then
  echo -n "Kaspa Address:  "
  cargo run -p kdapp-merchant -- kaspa-addr --kaspa-private-key "$KASPA_SK" ${NET_ARGS[@]:-}
fi

# Seed a demo subscription for testing
CUSTOMER_SK=$(hex 32)
CUSTOMER_INFO=$(cargo run -p kdapp-merchant -- register-customer --customer-private-key "$CUSTOMER_SK")
echo "$CUSTOMER_INFO"
CUSTOMER_PK=$(echo "$CUSTOMER_INFO" | grep 'registered customer pubkey' | awk '{print $4}')
if curl -s -f -H "X-API-Key: $API_KEY" -H "Content-Type: application/json" \
  -d "{\"subscription_id\":1,\"customer_public_key\":\"$CUSTOMER_PK\",\"amount\":1000,\"interval\":60}" \
  "http://127.0.0.1:$MERCHANT_PORT/subscribe" >/dev/null; then
  echo "Seeded demo subscription (id=1)"
else
  echo "Failed to seed demo subscription"
fi

# Launch TUI in foreground
exec cargo run -p onlykas-tui -- \
  --merchant-url http://127.0.0.1:"$MERCHANT_PORT" \
  --guardian-url http://127.0.0.1:"$GUARDIAN_PORT" \
  --watcher-url http://127.0.0.1:"$WATCHER_PORT" \
  --webhook-secret "$WEBHOOK_SECRET" \
  --api-key "$API_KEY" \
  --webhook-port "$WEBHOOK_PORT"
