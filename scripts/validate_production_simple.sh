#!/bin/bash
# Simple Production Validation Script

echo "==================================================================="
echo "PRODUCTION DEPLOYMENT VALIDATION"
echo "==================================================================="
echo ""

PASS=0
FAIL=0

# 1. WebSocket Configuration
echo "1. WebSocket Configuration:"
if grep -q "wss://ws-api-futures.kucoin.com" src/streaming/websocket_manager.rs; then
    echo "  ✅ WebSocket URL configured"
    PASS=$((PASS+1))
else
    echo "  ❌ WebSocket URL missing"
    FAIL=$((FAIL+1))
fi

if grep -q "max_concurrent_connections: 45" src/streaming/websocket_manager.rs; then
    echo "  ✅ Connection limit compliant (45/50)"
    PASS=$((PASS+1))
else
    echo "  ⚠️  Check connection limit"
fi

echo ""

# 2. Data Quality
echo "2. Data Quality:"
if [ -f "src/core/integration/data_quality.rs" ]; then
    echo "  ✅ Data Quality Manager exists"
    PASS=$((PASS+1))
else
    echo "  ❌ Data Quality Manager missing"
    FAIL=$((FAIL+1))
fi

if grep -q "completeness > 0.99" src/core/integration/data_quality.rs; then
    echo "  ✅ 99% completeness threshold set"
    PASS=$((PASS+1))
else
    echo "  ❌ Completeness threshold not set"
    FAIL=$((FAIL+1))
fi

echo ""

# 3. Pre-Trade Validation
echo "3. Pre-Trade Validation:"
if [ -f "src/core/integration/pre_trade_validator.rs" ]; then
    echo "  ✅ Pre-Trade Validator exists"
    PASS=$((PASS+1))
else
    echo "  ❌ Pre-Trade Validator missing"
    FAIL=$((FAIL+1))
fi

LAYERS=("DataQuality" "MarketConditions" "RiskLimits" "Regulatory" "Confidence")
for layer in "${LAYERS[@]}"; do
    if grep -q "ValidationLayer::$layer" src/core/integration/pre_trade_validator.rs; then
        echo "  ✅ Layer: $layer"
        PASS=$((PASS+1))
    else
        echo "  ❌ Layer: $layer missing"
        FAIL=$((FAIL+1))
    fi
done

echo ""

# 4. Token Monitoring
echo "4. Token Monitoring:"
if [ -f "src/monitoring/token_registry.rs" ]; then
    echo "  ✅ Token Registry exists"
    PASS=$((PASS+1))
else
    echo "  ❌ Token Registry missing"
    FAIL=$((FAIL+1))
fi

if grep -q "is_new_listing" src/monitoring/token_registry.rs; then
    echo "  ✅ NEW listing detection"
    PASS=$((PASS+1))
fi

echo ""

# 5. Risk Management
echo "5. Risk Management:"
if [ -f "src/trading/risk_manager.rs" ]; then
    echo "  ✅ Risk Manager exists"
    PASS=$((PASS+1))
else
    echo "  ❌ Risk Manager missing"
    FAIL=$((FAIL+1))
fi

echo ""

# 6. Paper Trading
echo "6. Paper Trading:"
if grep -q "true, // PAPER TRADING MODE" src/main.rs; then
    echo "  ✅ Paper trading enabled by default"
    PASS=$((PASS+1))
else
    echo "  ⚠️  Check paper trading mode"
fi

echo ""

# 7. Build Test
echo "7. Build Verification:"
echo "  Building project..."
if cargo build --release 2>&1 | grep -q "Finished"; then
    echo "  ✅ Build successful"
    PASS=$((PASS+1))
else
    echo "  ❌ Build failed"
    FAIL=$((FAIL+1))
fi

echo ""
echo "==================================================================="
echo "SUMMARY"
echo "==================================================================="
echo "Passed: $PASS"
echo "Failed: $FAIL"

if [ $FAIL -eq 0 ]; then
    echo ""
    echo "✅ VALIDATION PASSED - System ready for deployment!"
    exit 0
else
    echo ""
    echo "❌ VALIDATION FAILED - Address issues before deployment"
    exit 1
fi
