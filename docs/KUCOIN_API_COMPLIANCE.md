# KuCoin API Compliance & Rate Limit Enforcement

**Document Version:** 1.0.0  
**Last Updated:** 2025-11-16  
**Status:** 🔴 **CRITICAL - MANDATORY COMPLIANCE**

---

## ⚠️ CRITICAL WARNING

**VIOLATION OF THESE LIMITS WILL RESULT IN:**
- ❌ Immediate API request throttling (429 errors)
- ❌ Temporary account suspension (hours to days)
- ❌ Permanent API key revocation
- ❌ Possible trading account termination
- ❌ Loss of access to KuCoin Futures platform

**ALL TEAM MEMBERS MUST READ AND ACKNOWLEDGE THIS DOCUMENT BEFORE WORKING WITH THE KUCOIN INTEGRATION.**

---

## 1. KUCOIN OFFICIAL RATE LIMITS

### Futures API Rate Limits (VIP Level 0 - Default)

| Resource Pool | Limit | Window | Reset Type |
|--------------|-------|---------|------------|
| **Futures Pool** | 1,000 weight | 30 seconds | Rolling window |

**Source:** https://www.kucoin.com/docs/beginners/rate-limiting-futures

### VIP Level Limits

| VIP Level | Futures Pool Limit |
|-----------|-------------------|
| VIP 0 (Default) | 1,000 / 30s |
| VIP 1 | 1,200 / 30s |
| VIP 2 | 1,400 / 30s |
| VIP 3+ | Contact KuCoin |

**Current Account VIP Level:** VIP 0 (Default)

---

## 2. ENDPOINT WEIGHTS

Each API endpoint consumes a specific amount of "weight" from your rate limit quota:

### Market Data Endpoints (Public)

| Endpoint | Weight | Description |
|----------|--------|-------------|
| `GET /api/v1/contracts/active` | 2 | Get all active symbols |
| `GET /api/v1/contracts/{symbol}` | 2 | Get symbol details |
| `GET /api/v1/ticker` | 2 | Get ticker data |
| `GET /api/v1/level2/snapshot` | 2 | Get order book |
| `GET /api/v1/kline/query` | 2 | Get candlestick data |
| `GET /api/v1/trade/history` | **10** | Get trade history (HIGH WEIGHT) |

### Account Endpoints (Private)

| Endpoint | Weight | Description |
|----------|--------|-------------|
| `GET /api/v1/account-overview` | **5** | Get account info |
| `GET /api/v1/positions` | **5** | Get positions |
| `GET /api/v1/orders` | **5** | Get orders |
| `POST /api/v1/orders` | **5** | Place order |
| `DELETE /api/v1/orders/{id}` | 2 | Cancel order |

---

## 3. IMPLEMENTED SAFEGUARDS

### 3.1 Hard-Coded Rate Limits

Our implementation enforces **80% of official limits** as a safety margin:

```rust
const KUCOIN_FUTURES_POOL_LIMIT: u32 = 1000;  // Official limit
const SAFETY_MARGIN: f32 = 0.80;               // We enforce 800/30s
```

**You cannot bypass these limits without modifying core security code.**

### 3.2 Concurrent Request Limiting

Maximum concurrent requests: **20** (down from previous 50)

This prevents burst violations that could trigger immediate suspension.

### 3.3 Circuit Breaker System

When usage reaches **90% of capacity**, the circuit breaker automatically:
1. Pauses all new requests
2. Forces a 5-second cooldown
3. Logs a critical warning
4. Prevents further requests until usage drops

### 3.4 Automatic Request Queuing

If a request would exceed the limit:
1. Request is automatically queued
2. System waits for the rate limit window to reset
3. Request is retried after window expires
4. All of this happens transparently

### 3.5 Comprehensive Logging

Every API request is logged with:
- Timestamp
- Endpoint called
- Weight consumed
- Current usage percentage
- Rate limit violations

**Log Location:** `/opt/trading-bot/bot.log`

---

## 4. RATE LIMIT MONITORING

### 4.1 Real-Time Statistics

The system tracks:
- Current weight used in rolling 30s window
- Percentage of quota consumed
- Total requests made
- Number of rate limit violations
- Circuit breaker activations

### 4.2 Health Status Indicators

| Indicator | Usage | Status |
|-----------|-------|--------|
| 🟢 Green | < 70% | Healthy - No action needed |
| 🟡 Yellow | 70-85% | Caution - Monitor closely |
| 🔴 Red | > 85% | Critical - Circuit breaker active |

### 4.3 Accessing Statistics

```bash
# View rate limiter logs
grep "Rate Limit" /opt/trading-bot/bot.log | tail -50

# View circuit breaker activations
grep "CIRCUIT BREAKER" /opt/trading-bot/bot.log

# View rate limit violations
grep "RATE LIMIT" /opt/trading-bot/bot.log
```

---

## 5. CURRENT IMPLEMENTATION STATUS

### ✅ Implemented Safeguards

- [x] Hard-coded rate limits (800/30s)
- [x] Concurrent request limiting (20 max)
- [x] Circuit breaker system (90% threshold)
- [x] Automatic request queuing
- [x] Comprehensive audit logging
- [x] Real-time usage monitoring
- [x] Endpoint weight validation
- [x] Response header parsing (X-RateLimit headers)

### Implementation Files

| File | Purpose |
|------|---------|
| `src/api/rate_limiter.rs` | Core rate limiting logic |
| `src/api/kucoin.rs` | Integrated with all API calls |
| `src/scanner/market_scanner.rs` | Batch size reduced to 20 |

---

## 6. USAGE GUIDELINES FOR DEVELOPERS

### ⚠️ DO NOT:

1. ❌ **DO NOT** modify `KUCOIN_FUTURES_POOL_LIMIT` constant
2. ❌ **DO NOT** remove rate limiter checks from `request()` method
3. ❌ **DO NOT** bypass the `acquire()` permission system
4. ❌ **DO NOT** increase batch sizes beyond 20
5. ❌ **DO NOT** reduce `SAFETY_MARGIN` below 0.80
6. ❌ **DO NOT** disable circuit breaker logic
7. ❌ **DO NOT** remove or reduce wait times
8. ❌ **DO NOT** make parallel API calls outside the controlled system

### ✅ DO:

1. ✅ **ALWAYS** use the `KuCoinClient` for API calls (rate limiting is automatic)
2. ✅ **MONITOR** rate limit statistics regularly
3. ✅ **REPORT** any 429 errors immediately
4. ✅ **TEST** locally before deploying to production
5. ✅ **REVIEW** logs after any code changes
6. ✅ **RESPECT** the circuit breaker cooldowns
7. ✅ **DOCUMENT** any new endpoints and their weights
8. ✅ **COORDINATE** with team before making API-related changes

---

## 7. EMERGENCY PROCEDURES

### If You Receive 429 (Too Many Requests) Errors:

1. **STOP** making new requests immediately
2. **CHECK** the logs for circuit breaker status
3. **WAIT** for the 30-second window to fully reset
4. **REVIEW** recent code changes
5. **REPORT** the incident to the team

### If API Key is Suspended:

1. **CEASE** all bot operations immediately
2. **CONTACT** KuCoin support: support@kucoin.com
3. **DOCUMENT** the circumstances leading to suspension
4. **REVIEW** all logs from the past 24 hours
5. **IMPLEMENT** additional safeguards if needed

---

## 8. TESTING PROCEDURES

### Before Deploying Changes:

1. **Local Testing:**
   ```bash
   cargo test --test rate_limiter_tests
   ```

2. **Monitor Rate Limits:**
   ```bash
   # Run bot locally and watch logs
   cargo run | grep -E "(Rate Limit|CIRCUIT)"
   ```

3. **Verify Compliance:**
   - Ensure no requests exceed 800 weight/30s
   - Confirm circuit breaker triggers at 90%
   - Check that violations are logged

4. **Production Deployment:**
   - Deploy during low-activity hours
   - Monitor first 30 minutes closely
   - Have rollback plan ready

---

## 9. METRICS & KPIs

### Target Metrics

| Metric | Target | Alert Threshold |
|--------|--------|----------------|
| Average Usage | < 60% | > 75% |
| Peak Usage | < 80% | > 85% |
| Violations/Hour | 0 | > 1 |
| Circuit Breaker Activations/Day | 0 | > 5 |
| 429 Errors/Day | 0 | > 0 |

### Weekly Review Checklist

- [ ] Review rate limit violation logs
- [ ] Check circuit breaker activation frequency
- [ ] Analyze usage patterns
- [ ] Verify safety margins are adequate
- [ ] Update documentation if KuCoin changes limits

---

## 10. ACKNOWLEDGMENT REQUIRED

**I have read and understood the KuCoin API Compliance document. I acknowledge that:**

1. Violating rate limits can result in account suspension
2. I will not modify rate limit constants without authorization
3. I will monitor rate limit statistics when making API changes
4. I will report any 429 errors immediately
5. I understand the emergency procedures
6. I will follow all testing procedures before deployment

**Developer Name:** ________________________  
**Date:** ____________  
**Signature:** ________________________  

---

## 11. REVISION HISTORY

| Version | Date | Changes | Author |
|---------|------|---------|--------|
| 1.0.0 | 2025-11-16 | Initial compliance document | Trading Bot Team |

---

## 12. REFERENCES

- [KuCoin API Documentation](https://www.kucoin.com/docs)
- [KuCoin Rate Limiting Guide](https://www.kucoin.com/docs/beginners/rate-limiting-futures)
- [KuCoin Futures API](https://www.kucoin.com/docs-futures)
- Internal: `KUCOIN_API_FORENSIC_ANALYSIS.md`
- Internal: `src/api/rate_limiter.rs`

---

**⚠️  This is a living document. It will be updated as KuCoin changes their API policies.**

**Last Verified Against KuCoin Documentation:** 2025-11-16

