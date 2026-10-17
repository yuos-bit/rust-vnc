#!/usr/bin/env bash
# End-to-end smoke test for the pairing backend.
# Starts a throwaway rv-server, exercises registration -> heartbeat -> pairing
# -> connect -> session verification -> revocation, then shuts it down.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN="$ROOT/target/debug/rv-server.exe"
[[ -x "$BIN" ]] || BIN="$ROOT/target/debug/rv-server"
if [[ ! -x "$BIN" ]]; then
  echo "Building rv-server..."
  (cd "$ROOT" && cargo build -p rv-server)
fi

PORT="${SMOKE_PORT:-18080}"
BASE="http://127.0.0.1:$PORT"
ADMIN="smoke-admin-token-0123456789abcdef"
TMP="$(mktemp -d)"
PID=""

cleanup() {
  if [[ -n "$PID" ]] && kill -0 "$PID" 2>/dev/null; then
    kill "$PID" 2>/dev/null || true
    wait "$PID" 2>/dev/null || true
  fi
  rm -rf "$TMP"
}
trap cleanup EXIT

RV_BIND="127.0.0.1:$PORT" RV_DB="$TMP/rvnc.db" RV_ADMIN_TOKEN="$ADMIN" \
  "$BIN" >"$TMP/server.log" 2>&1 &
PID=$!

ok=0
fail=0
pass() { ok=$((ok + 1)); printf '  \033[32mPASS\033[0m %s\n' "$1"; }
bad() { fail=$((fail + 1)); printf '  \033[31mFAIL\033[0m %s\n' "$1"; }
json() { python -c "import sys,json; d=json.load(sys.stdin); print(eval('d' + sys.argv[1]))" "$1"; }

for _ in $(seq 1 80); do
  if curl -sf "$BASE/healthz" >/dev/null 2>&1; then break; fi
  sleep 0.15
done
curl -sf "$BASE/healthz" >/dev/null || { echo "server did not start"; cat "$TMP/server.log"; exit 1; }
echo "rv-server is up at $BASE"

REG="$(curl -sS -X POST "$BASE/api/v1/devices/register" \
  -H 'Content-Type: application/json' \
  -d '{"name":"Smoke PC","platform":"windows","fingerprint":"smoke-host","version":"0.1.0"}')"
CODE="$(printf '%s' "$REG" | json "['device_code']")"
PASSWORD="$(printf '%s' "$REG" | json "['password']")"
AGENT_SECRET="$(printf '%s' "$REG" | json "['agent_secret']")"
[[ ${#CODE} -eq 6 ]] && pass "registration issued a 6-character code ($CODE)" || bad "unexpected device code: $CODE"
[[ ${#PASSWORD} -eq 8 ]] && pass "registration issued an 8-character password" || bad "unexpected password length"

HEARTBEAT="$(curl -sS -X POST "$BASE/api/v1/devices/heartbeat" \
  -H 'Content-Type: application/json' \
  -d "{\"device_code\":\"$CODE\",\"agent_secret\":\"$AGENT_SECRET\",\"tailscale_ip\":\"100.64.0.42\",\"listen_port\":21416,\"version\":\"0.1.0\"}")"
[[ "$(printf '%s' "$HEARTBEAT" | json "['online']")" == "True" ]] && pass "heartbeat accepted" || bad "heartbeat rejected"

STATUS="$(curl -s -o /dev/null -w '%{http_code}' -X POST "$BASE/api/v1/pair" \
  -H 'Content-Type: application/json' \
  -d "{\"device_code\":\"$CODE\",\"password\":\"WRONG123\",\"controller_id\":\"smoke-ctrl\",\"controller_name\":\"Smoke Controller\",\"platform\":\"windows\"}")"
[[ "$STATUS" == "401" ]] && pass "wrong password rejected with 401" || bad "wrong password returned $STATUS"

PAIR="$(curl -sS -X POST "$BASE/api/v1/pair" \
  -H 'Content-Type: application/json' \
  -d "{\"device_code\":\"$CODE\",\"password\":\"$PASSWORD\",\"controller_id\":\"smoke-ctrl\",\"controller_name\":\"Smoke Controller\",\"platform\":\"windows\"}")"
TOKEN="$(printf '%s' "$PAIR" | json "['token']")"
TOKEN_ID="$(printf '%s' "$PAIR" | json "['token_id']")"
[[ "$TOKEN" == rvt_* ]] && pass "pairing issued a bearer token" || bad "unexpected token: $TOKEN"

CONNECT="$(curl -sS -X POST "$BASE/api/v1/connect" \
  -H 'Content-Type: application/json' \
  -d "{\"device_code\":\"$CODE\",\"token\":\"$TOKEN\",\"controller_id\":\"smoke-ctrl\"}")"
[[ "$(printf '%s' "$CONNECT" | json "['tailscale_ip']")" == "100.64.0.42" ]] && pass "connect resolved the tailnet address" || bad "connect returned: $CONNECT"

VERIFY="$(curl -sS -X POST "$BASE/api/v1/session/verify" \
  -H 'Content-Type: application/json' \
  -d "{\"device_code\":\"$CODE\",\"agent_secret\":\"$AGENT_SECRET\",\"token\":\"$TOKEN\",\"controller_id\":\"smoke-ctrl\"}")"
[[ "$(printf '%s' "$VERIFY" | json "['valid']")" == "True" ]] && pass "agent-side token verification succeeded" || bad "verify returned: $VERIFY"

STATUS="$(curl -s -o /dev/null -w '%{http_code}' -X POST "$BASE/api/v1/controllers/enroll" \
  -H 'Content-Type: application/json' \
  -d '{"name":"Smoke Controller","platform":"windows","fingerprint":"smoke","controller_id":"smoke-ctrl"}')"
[[ "$STATUS" == "400" ]] && pass "controller enrollment requires Headscale configuration" || bad "controller enrollment returned $STATUS without Headscale"

STATUS="$(curl -s -o /dev/null -w '%{http_code}' "$BASE/api/admin/devices")"
[[ "$STATUS" == "401" ]] && pass "admin API rejects missing token" || bad "admin API returned $STATUS without auth"

LOGIN="$(curl -sS -X POST "$BASE/api/admin/login" \
  -H 'Content-Type: application/json' -d "{\"token\":\"$ADMIN\"}")"
[[ "$(printf '%s' "$LOGIN" | json "['ok']")" == "True" ]] && pass "admin login accepted" || bad "admin login failed: $LOGIN"

DEVICES="$(curl -sS "$BASE/api/admin/devices" -H "Authorization: Bearer $ADMIN")"
[[ "$(printf '%s' "$DEVICES" | python -c 'import sys,json;print(len(json.load(sys.stdin)))')" == "1" ]] && pass "admin devices list contains the registered device" || bad "admin devices list mismatch"

STATS="$(curl -sS "$BASE/api/admin/stats" -H "Authorization: Bearer $ADMIN")"
[[ "$(printf '%s' "$STATS" | json "['active_tokens']")" == "1" ]] && pass "stats counts the active token" || bad "stats mismatch: $STATS"

REVOKE="$(curl -sS -X POST "$BASE/api/v1/agent/tokens/revoke" \
  -H 'Content-Type: application/json' \
  -d "{\"device_code\":\"$CODE\",\"agent_secret\":\"$AGENT_SECRET\",\"token_id\":\"$TOKEN_ID\"}")"
[[ "$(printf '%s' "$REVOKE" | json "['ok']")" == "True" ]] && pass "token revoked by the endpoint" || bad "revoke failed: $REVOKE"

AFTER="$(curl -sS -X POST "$BASE/api/v1/session/verify" \
  -H 'Content-Type: application/json' \
  -d "{\"device_code\":\"$CODE\",\"agent_secret\":\"$AGENT_SECRET\",\"token\":\"$TOKEN\",\"controller_id\":\"smoke-ctrl\"}")"
[[ "$(printf '%s' "$AFTER" | json "['valid']")" == "False" ]] && pass "revoked token no longer verifies" || bad "revoked token still valid: $AFTER"

CONNECT_AFTER="$(curl -s -o /dev/null -w '%{http_code}' -X POST "$BASE/api/v1/connect" \
  -H 'Content-Type: application/json' \
  -d "{\"device_code\":\"$CODE\",\"token\":\"$TOKEN\",\"controller_id\":\"smoke-ctrl\"}")"
[[ "$CONNECT_AFTER" == "401" ]] && pass "connect rejects the revoked token" || bad "connect returned $CONNECT_AFTER"

echo
echo "Result: $ok passed, $fail failed"
[[ $fail -eq 0 ]]
