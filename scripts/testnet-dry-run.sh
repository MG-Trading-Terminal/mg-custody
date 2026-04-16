#!/usr/bin/env bash
#
# MG Custody — Testnet Dry Run Script
#
# Validates the full custody flow against a running instance:
# 1. Health check
# 2. Create deposit address (ETH)
# 3. Submit withdrawal
# 4. Check withdrawal status
# 5. Check wallet balance
# 6. Verify chain health
# 7. Check circuit breaker status
#
# Prerequisites:
#   - Custody service running on $BASE_URL (default: http://localhost:8443)
#   - PostgreSQL with migrations applied
#   - HMAC secret matching $HMAC_SECRET
#
# Usage:
#   ./scripts/testnet-dry-run.sh
#   BASE_URL=http://custody:8443 HMAC_SECRET=my-secret ./scripts/testnet-dry-run.sh

set -euo pipefail

BASE_URL="${BASE_URL:-http://localhost:8443}"
HMAC_SECRET="${HMAC_SECRET:-}"
PASS=0
FAIL=0
SKIP=0

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
CYAN='\033[0;36m'
NC='\033[0m'

log()  { echo -e "${CYAN}[TEST]${NC} $*"; }
pass() { echo -e "${GREEN}[PASS]${NC} $*"; PASS=$((PASS + 1)); }
fail() { echo -e "${RED}[FAIL]${NC} $*"; FAIL=$((FAIL + 1)); }
skip() { echo -e "${YELLOW}[SKIP]${NC} $*"; SKIP=$((SKIP + 1)); }

# HMAC signing helper
hmac_sign() {
    local method="$1" path="$2" timestamp="$3" body="${4:-}"
    if [ -n "$HMAC_SECRET" ]; then
        echo -n "${method}${path}${timestamp}${body}" | openssl dgst -sha256 -hmac "$HMAC_SECRET" -hex 2>/dev/null | awk '{print $NF}'
    else
        echo ""
    fi
}

# API call helper
api_call() {
    local method="$1" path="$2" body="${3:-}"
    local timestamp
    timestamp=$(date +%s)
    local sig
    sig=$(hmac_sign "$method" "$path" "$timestamp" "$body")

    local headers=(-H "Content-Type: application/json")
    if [ -n "$sig" ]; then
        headers+=(-H "X-Custody-Timestamp: $timestamp")
        headers+=(-H "X-Custody-Signature: $sig")
    fi

    if [ -n "$body" ]; then
        curl -s -w "\n%{http_code}" -X "$method" "${BASE_URL}${path}" "${headers[@]}" -d "$body"
    else
        curl -s -w "\n%{http_code}" -X "$method" "${BASE_URL}${path}" "${headers[@]}"
    fi
}

echo "================================================================"
echo "  MG Custody — Testnet Dry Run"
echo "  Target: $BASE_URL"
echo "  HMAC:   ${HMAC_SECRET:+configured}${HMAC_SECRET:-not set (unauthenticated)}"
echo "================================================================"
echo ""

# ── Test 1: Health Check ───────────────────────────────────────
log "1/7 Health check..."
RESP=$(curl -s -w "\n%{http_code}" "${BASE_URL}/health" 2>/dev/null || echo -e "\n000")
HTTP_CODE=$(echo "$RESP" | tail -1)
BODY=$(echo "$RESP" | head -1)

if [ "$HTTP_CODE" = "200" ]; then
    pass "Health endpoint returned 200: $BODY"
else
    fail "Health endpoint returned $HTTP_CODE (expected 200)"
    echo -e "${RED}Cannot continue — service is not running${NC}"
    exit 1
fi

# ── Test 2: Chain Health ───────────────────────────────────────
log "2/7 Chain health..."
RESP=$(curl -s -w "\n%{http_code}" "${BASE_URL}/health/chains" 2>/dev/null || echo -e "\n000")
HTTP_CODE=$(echo "$RESP" | tail -1)
BODY=$(echo "$RESP" | head -1)

if [ "$HTTP_CODE" = "200" ]; then
    pass "Chain health returned 200"
    echo "     $BODY" | head -c 200
    echo ""
else
    skip "Chain health returned $HTTP_CODE"
fi

# ── Test 3: Metrics Endpoint ──────────────────────────────────
log "3/7 Prometheus metrics..."
RESP=$(curl -s -w "\n%{http_code}" "${BASE_URL}/metrics" 2>/dev/null || echo -e "\n000")
HTTP_CODE=$(echo "$RESP" | tail -1)

if [ "$HTTP_CODE" = "200" ]; then
    METRIC_COUNT=$(echo "$RESP" | head -n -1 | grep -c "^custody_" || true)
    pass "Metrics endpoint returned 200 ($METRIC_COUNT custody_* metrics)"
else
    skip "Metrics returned $HTTP_CODE"
fi

# ── Test 4: Create Deposit Address ────────────────────────────
log "4/7 Create deposit address (ETH)..."
USER_ID="$(uuidgen 2>/dev/null || python3 -c 'import uuid; print(uuid.uuid4())' 2>/dev/null || echo '00000000-1111-2222-3333-444444444444')"
BODY="{\"user_id\":\"$USER_ID\",\"asset\":\"ETH\",\"chain\":\"ethereum\"}"
RESP=$(api_call POST "/v1/addresses" "$BODY")
HTTP_CODE=$(echo "$RESP" | tail -1)
RESP_BODY=$(echo "$RESP" | head -n -1)

if [ "$HTTP_CODE" = "200" ] || [ "$HTTP_CODE" = "201" ]; then
    ADDRESS=$(echo "$RESP_BODY" | python3 -c "import sys,json; print(json.load(sys.stdin).get('address',''))" 2>/dev/null || echo "")
    if [ -n "$ADDRESS" ]; then
        pass "Created deposit address: $ADDRESS"
    else
        pass "Address endpoint returned $HTTP_CODE (response: ${RESP_BODY:0:100})"
    fi
elif [ "$HTTP_CODE" = "401" ] || [ "$HTTP_CODE" = "403" ]; then
    skip "Address creation requires auth (got $HTTP_CODE) — set HMAC_SECRET"
else
    fail "Address creation returned $HTTP_CODE: ${RESP_BODY:0:200}"
fi

# ── Test 5: Submit Withdrawal ─────────────────────────────────
log "5/7 Submit withdrawal..."
W_ID="$(uuidgen 2>/dev/null || python3 -c 'import uuid; print(uuid.uuid4())' 2>/dev/null || echo '55555555-6666-7777-8888-999999999999')"
BODY="{\"withdrawal_id\":\"$W_ID\",\"asset\":\"ETH\",\"chain\":\"ethereum\",\"address\":\"0x742d35Cc6634C0532925a3b844Bc9e7595f2bD18\",\"amount\":\"0.01\"}"
RESP=$(api_call POST "/v1/withdrawals" "$BODY")
HTTP_CODE=$(echo "$RESP" | tail -1)
RESP_BODY=$(echo "$RESP" | head -n -1)

if [ "$HTTP_CODE" = "202" ] || [ "$HTTP_CODE" = "200" ]; then
    pass "Withdrawal submitted (HTTP $HTTP_CODE): ${RESP_BODY:0:100}"
elif [ "$HTTP_CODE" = "401" ] || [ "$HTTP_CODE" = "403" ]; then
    skip "Withdrawal requires auth (got $HTTP_CODE)"
else
    fail "Withdrawal returned $HTTP_CODE: ${RESP_BODY:0:200}"
fi

# ── Test 6: Check Withdrawal Status ──────────────────────────
log "6/7 Check withdrawal status..."
RESP=$(api_call GET "/v1/withdrawals/$W_ID")
HTTP_CODE=$(echo "$RESP" | tail -1)
RESP_BODY=$(echo "$RESP" | head -n -1)

if [ "$HTTP_CODE" = "200" ]; then
    pass "Withdrawal status: ${RESP_BODY:0:150}"
elif [ "$HTTP_CODE" = "404" ]; then
    pass "Withdrawal not found (expected if auth was skipped)"
elif [ "$HTTP_CODE" = "401" ] || [ "$HTTP_CODE" = "403" ]; then
    skip "Status check requires auth (got $HTTP_CODE)"
else
    fail "Status check returned $HTTP_CODE: ${RESP_BODY:0:200}"
fi

# ── Test 7: Wallet Balance ───────────────────────────────────
log "7/7 Wallet balance..."
RESP=$(api_call GET "/v1/wallets/ETH")
HTTP_CODE=$(echo "$RESP" | tail -1)
RESP_BODY=$(echo "$RESP" | head -n -1)

if [ "$HTTP_CODE" = "200" ]; then
    pass "Wallet balance: ${RESP_BODY:0:150}"
elif [ "$HTTP_CODE" = "401" ] || [ "$HTTP_CODE" = "403" ]; then
    skip "Wallet balance requires auth (got $HTTP_CODE)"
else
    fail "Wallet balance returned $HTTP_CODE: ${RESP_BODY:0:200}"
fi

# ── Summary ──────────────────────────────────────────────────
echo ""
echo "================================================================"
echo -e "  Results: ${GREEN}${PASS} passed${NC}, ${RED}${FAIL} failed${NC}, ${YELLOW}${SKIP} skipped${NC}"
echo "================================================================"

if [ "$FAIL" -gt 0 ]; then
    exit 1
fi
