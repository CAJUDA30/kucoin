#!/bin/bash
# KuCoin WebSocket Configuration Validator
# Version: 1.0.0
# Purpose: Automated validation of WebSocket configuration against API requirements

set -e

echo "╔══════════════════════════════════════════════════════════════════════╗"
echo "║     KuCoin WebSocket Configuration Validator v1.0.0                 ║"
echo "╚══════════════════════════════════════════════════════════════════════╝"
echo ""

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Counters
PASSED=0
FAILED=0
WARNINGS=0

# Test function
test_check() {
    local name="$1"
    local status="$2"
    local message="$3"
    
    if [ "$status" = "pass" ]; then
        echo -e "${GREEN}✅${NC} $name"
        ((PASSED++))
    elif [ "$status" = "fail" ]; then
        echo -e "${RED}❌${NC} $name"
        echo -e "   ${RED}Error: $message${NC}"
        ((FAILED++))
    elif [ "$status" = "warn" ]; then
        echo -e "${YELLOW}⚠️${NC}  $name"
        echo -e "   ${YELLOW}Warning: $message${NC}"
        ((WARNINGS++))
    fi
}

echo "🔍 1. Network Connectivity Tests"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

# Test 1.1: DNS Resolution
if host ws-api-futures.kucoin.com > /dev/null 2>&1; then
    test_check "DNS Resolution (ws-api-futures.kucoin.com)" "pass"
else
    test_check "DNS Resolution" "fail" "Cannot resolve ws-api-futures.kucoin.com"
fi

# Test 1.2: Port Connectivity
if timeout 5 bash -c "cat < /dev/null > /dev/tcp/ws-api-futures.kucoin.com/443" 2>/dev/null; then
    test_check "Port 443 Connectivity" "pass"
else
    test_check "Port 443 Connectivity" "fail" "Cannot connect to port 443"
fi

# Test 1.3: TLS Certificate
if command -v openssl >/dev/null 2>&1; then
    if echo | openssl s_client -connect ws-api-futures.kucoin.com:443 -servername ws-api-futures.kucoin.com 2>/dev/null | grep -q "Verify return code: 0"; then
        test_check "TLS Certificate Validity" "pass"
    else
        test_check "TLS Certificate Validity" "warn" "Certificate validation failed or expired"
    fi
else
    test_check "TLS Certificate Validity" "warn" "OpenSSL not available, skipping"
fi

echo ""
echo "🔧 2. Configuration Parameter Validation"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

# Test 2.1: Check max_concurrent_connections
if [ -f "src/streaming/websocket_manager.rs" ]; then
    MAX_CONN=$(grep -A 10 "impl Default for ConnectionConfig" src/streaming/websocket_manager.rs | grep "max_concurrent_connections:" | grep -o '[0-9]\+' | head -1)
    
    if [ -n "$MAX_CONN" ]; then
        if [ "$MAX_CONN" -le 50 ]; then
            test_check "Connection Limit (≤50)" "pass"
        else
            test_check "Connection Limit (≤50)" "fail" "Current: $MAX_CONN, API Limit: 50"
        fi
    else
        test_check "Connection Limit Check" "warn" "Could not extract value"
    fi
else
    test_check "WebSocket Manager File" "fail" "src/streaming/websocket_manager.rs not found"
fi

# Test 2.2: Check ping interval
if [ -f "src/streaming/websocket_manager.rs" ]; then
    PING_INTERVAL=$(grep -A 10 "impl Default for ConnectionConfig" src/streaming/websocket_manager.rs | grep "ping_interval_secs:" | grep -o '[0-9]\+' | head -1)
    
    if [ -n "$PING_INTERVAL" ]; then
        if [ "$PING_INTERVAL" -ge 18 ] && [ "$PING_INTERVAL" -le 30 ]; then
            test_check "Ping Interval (18-30s)" "pass"
        else
            test_check "Ping Interval (18-30s)" "warn" "Current: ${PING_INTERVAL}s, Recommended: 18-30s"
        fi
    else
        test_check "Ping Interval Check" "warn" "Could not extract value"
    fi
fi

# Test 2.3: Check connection timeout
if [ -f "src/streaming/websocket_manager.rs" ]; then
    TIMEOUT=$(grep -A 10 "impl Default for ConnectionConfig" src/streaming/websocket_manager.rs | grep "connection_timeout_secs:" | grep -o '[0-9]\+' | head -1)
    
    if [ -n "$TIMEOUT" ]; then
        if [ "$TIMEOUT" -eq 10 ]; then
            test_check "Connection Timeout (10s)" "pass"
        else
            test_check "Connection Timeout (10s)" "warn" "Current: ${TIMEOUT}s, Recommended: 10s"
        fi
    else
        test_check "Connection Timeout Check" "warn" "Could not extract value"
    fi
fi

echo ""
echo "⚡ 3. Rate Limiter Configuration"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

# Test 3.1: Check max_requests
if [ -f "src/streaming/rate_limiter.rs" ]; then
    MAX_REQ=$(grep -A 10 "impl Default for RateLimiterConfig" src/streaming/rate_limiter.rs | grep "max_requests:" | grep -o '[0-9]\+' | head -1)
    
    if [ -n "$MAX_REQ" ]; then
        if [ "$MAX_REQ" -le 100 ]; then
            test_check "Rate Limit (≤100 req/window)" "pass"
        else
            test_check "Rate Limit (≤100 req/window)" "fail" "Current: $MAX_REQ, API Limit: 100"
        fi
    else
        test_check "Rate Limit Check" "warn" "Could not extract value"
    fi
else
    test_check "Rate Limiter File" "fail" "src/streaming/rate_limiter.rs not found"
fi

# Test 3.2: Check window duration
if [ -f "src/streaming/rate_limiter.rs" ]; then
    WINDOW=$(grep -A 10 "impl Default for RateLimiterConfig" src/streaming/rate_limiter.rs | grep "window_duration_secs:" | grep -o '[0-9]\+' | head -1)
    
    if [ -n "$WINDOW" ]; then
        if [ "$WINDOW" -le 10 ]; then
            test_check "Rate Window (≤10s)" "pass"
        else
            test_check "Rate Window (≤10s)" "warn" "Current: ${WINDOW}s, Recommended: ≤10s"
        fi
    else
        test_check "Rate Window Check" "warn" "Could not extract value"
    fi
fi

echo ""
echo "🔒 4. Security Configuration"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

# Test 4.1: Check for hardcoded secrets
if grep -r "api_secret.*=" src/ --include="*.rs" | grep -v "env::var" | grep -q "\""; then
    test_check "No Hardcoded Secrets" "fail" "Found potential hardcoded secrets in source"
else
    test_check "No Hardcoded Secrets" "pass"
fi

# Test 4.2: Check .env.example exists
if [ -f ".env.example" ]; then
    test_check ".env.example Present" "pass"
else
    test_check ".env.example Present" "warn" ".env.example not found"
fi

# Test 4.3: Check .gitignore includes .env
if grep -q "^\.env$" .gitignore 2>/dev/null; then
    test_check ".env in .gitignore" "pass"
else
    test_check ".env in .gitignore" "warn" ".env should be in .gitignore"
fi

echo ""
echo "📦 5. Build & Test Validation"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

# Test 5.1: Check if project builds
if cargo check --quiet 2>/dev/null; then
    test_check "Project Builds" "pass"
else
    test_check "Project Builds" "fail" "cargo check failed"
fi

# Test 5.2: Check for tests
if [ -d "tests" ] && ls tests/*.rs >/dev/null 2>&1; then
    test_check "Tests Directory Exists" "pass"
else
    test_check "Tests Directory Exists" "warn" "No test files found"
fi

# Test 5.3: Check documentation
if [ -f "docs/KUCOIN_WEBSOCKET_FORENSIC_ANALYSIS.md" ]; then
    test_check "Forensic Documentation" "pass"
else
    test_check "Forensic Documentation" "warn" "Forensic analysis document not found"
fi

echo ""
echo "╔══════════════════════════════════════════════════════════════════════╗"
echo "║                      VALIDATION SUMMARY                              ║"
echo "╚══════════════════════════════════════════════════════════════════════╝"
echo ""
echo -e "${GREEN}✅ Passed:${NC} $PASSED"
echo -e "${YELLOW}⚠️  Warnings:${NC} $WARNINGS"
echo -e "${RED}❌ Failed:${NC} $FAILED"
echo ""

if [ "$FAILED" -eq 0 ]; then
    if [ "$WARNINGS" -eq 0 ]; then
        echo -e "${GREEN}🎉 All validations passed! Configuration is optimal.${NC}"
        exit 0
    else
        echo -e "${YELLOW}⚠️  Configuration is valid but has warnings. Review recommended.${NC}"
        exit 0
    fi
else
    echo -e "${RED}❌ Configuration validation failed. Fix required issues before deployment.${NC}"
    echo ""
    echo "See docs/KUCOIN_WEBSOCKET_FORENSIC_ANALYSIS.md for detailed fixes."
    exit 1
fi

