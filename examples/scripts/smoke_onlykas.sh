#!/usr/bin/env bash
set -euo pipefail

# random ports
GUARDIAN_PORT=$(shuf -i 30000-35000 -n1)
WATCHER_PORT=$(shuf -i 35001-40000 -n1)
MERCHANT_PORT=$(shuf -i 40001-45000 -n1)
WEBHOOK_PORT=$(shuf -i 45001-50000 -n1)

KASPA_SK=0000000000000000000000000000000000000000000000000000000000000001
MERCHANT_SK=0000000000000000000000000000000000000000000000000000000000000002
API_KEY=test

WEBHOOK_LOG=$(mktemp)
python3 -u - <<'PY' "$WEBHOOK_PORT" "$WEBHOOK_LOG" &
import http.server, sys, json, pathlib
port=int(sys.argv[1]); log=pathlib.Path(sys.argv[2])
class H(http.server.BaseHTTPRequestHandler):
    def do_POST(self):
        length=int(self.headers.get('Content-Length',0))
        body=self.rfile.read(length)
        log.write_text((log.read_text() if log.exists() else '')+body.decode()+"\n")
        self.send_response(200); self.end_headers()
    def log_message(self, *a): pass
http.server.HTTPServer(('127.0.0.1', port), H).serve_forever()
PY
WEBHOOK_PID=$!

guardian-service --listen-addr 127.0.0.1:${GUARDIAN_PORT} --wrpc-url ws://127.0.0.1:16110 \
  --state-path ./tmp/guardian.state --http-port $((GUARDIAN_PORT+1)) &
GUARDIAN_PID=$!

kdapp-merchant watcher --bind 127.0.0.1:0 --kaspa-private-key $KASPA_SK \
  --fee-policy static --static-fee 5000 --http-port ${WATCHER_PORT} &
WATCHER_PID=$!

kdapp-merchant serve --bind 127.0.0.1:${MERCHANT_PORT} --episode-id 1 \
  --merchant-private-key $MERCHANT_SK --api-key $API_KEY \
  --webhook-url http://127.0.0.1:${WEBHOOK_PORT}/hook --webhook-secret deadbeef &
SERVER_PID=$!

sleep 2

curl -s -X POST http://127.0.0.1:${MERCHANT_PORT}/invoice -H "x-api-key: $API_KEY" \
  -d '{"invoice_id":1,"amount":1000}' >/dev/null
curl -s -X POST http://127.0.0.1:${MERCHANT_PORT}/pay -H "x-api-key: $API_KEY" \
  -d '{"invoice_id":1,"payer_public_key":"02b4635d5e5e5c1f4a266cb14f0bf4b1e9d5f0f4a5b257a7b0b3a149345b3f1a2e"}' >/dev/null
curl -s -X POST http://127.0.0.1:${MERCHANT_PORT}/ack -H "x-api-key: $API_KEY" \
  -d '{"invoice_id":1}' >/dev/null

FEE=$(curl -s http://127.0.0.1:${WATCHER_PORT}/mempool | python3 -c 'import sys,json;print(json.load(sys.stdin)["selected_fee"])')
EVENTS=$(wc -l < "$WEBHOOK_LOG")
if grep -q invoice_created "$WEBHOOK_LOG" && grep -q invoice_paid "$WEBHOOK_LOG" \
   && grep -q invoice_acked "$WEBHOOK_LOG"; then
  echo "PASS fee=$FEE events=$EVENTS"
else
  echo "FAIL" && exit 1
fi

kill $SERVER_PID $WATCHER_PID $GUARDIAN_PID $WEBHOOK_PID
