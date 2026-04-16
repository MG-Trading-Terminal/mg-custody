# Incident Response Plan

## Severity Levels

| Level | Definition | Response Time | Example |
|-------|-----------|---------------|---------|
| **SEV1** | Active fund loss or key compromise | Immediate (< 15 min) | Unauthorized withdrawal, leaked private key |
| **SEV2** | Service disruption, potential fund risk | < 1 hour | Circuit breaker tripped, signer unreachable |
| **SEV3** | Degraded service, no fund risk | < 4 hours | Scanner lag > 200 blocks, RPC node down |
| **SEV4** | Minor issue, no user impact | Next business day | Dashboard UI bug, metric missing |

## Detection

### Automated Alerts (Prometheus)
- `CircuitBreakerTripped` → SEV2 (immediate page)
- `SigningFailureRate > 10%` → SEV2
- `ChainScanLagCritical > 200 blocks` → SEV3
- `ChainRpcDown` → SEV3
- `PermissionDenialSpike` → SEV3 (potential attack)
- `DbPoolExhaustion > 80%` → SEV3
- `ApiErrorRate > 5%` → SEV3

### Manual Detection
- User reports missing deposit → check scanner logs + chain explorer
- Exchange reports webhook failure → check delivery queue
- Anomalous withdrawal patterns → check audit log

## Response Procedures

### SEV1: Fund Loss / Key Compromise

1. **IMMEDIATELY**: Emergency freeze
   ```bash
   custody-admin freeze --reason "SEV1: [description]"
   # or via API:
   curl -X POST /v1/admin/emergency/freeze -d '{"reason":"..."}'
   ```

2. **Assess scope** (< 30 min):
   - Which keys affected? Check audit log for signing operations
   - Which addresses compromised? Check withdrawal history
   - Is the attacker still active? Check real-time logs

3. **Contain**:
   - Rotate compromised keys (if MPC: initiate key reshare)
   - Revoke all sessions: truncate sessions table
   - Block attacker IP if known: add to firewall

4. **Recover**:
   - Generate new deposit addresses for affected users
   - Sweep remaining funds to new cold wallet
   - Notify affected users

5. **Post-mortem** (within 48 hours):
   - Root cause analysis
   - Timeline of events
   - Remediation actions
   - Process improvements

### SEV2: Service Disruption

1. **Diagnose** (< 15 min):
   - Check `/health/detailed` for component status
   - Check Grafana dashboard for anomalies
   - Check `docker-compose logs custody` for errors

2. **Circuit breaker tripped**:
   - Verify legitimate (check outflow amounts)
   - If false positive: reset via DB (with owner approval)
   - If legitimate: investigate why limits were hit

3. **Signer unreachable**:
   - Check IPC socket: `ls -la /run/custody/signer.sock`
   - Check signer process: `docker-compose logs signer`
   - Restart signer: `docker-compose restart signer`

### SEV3: Degraded Service

1. **Scanner lag**:
   - Check RPC node: `curl $RPC_URL` health
   - Switch to fallback RPC if primary is down
   - Force rescan: `custody-admin rescan --chain ethereum --from-block N`

2. **RPC node down**:
   - Failover to backup endpoint (config/default.toml fallback_rpc_urls)
   - If all endpoints down: chain is likely having issues, monitor

3. **DB pool exhaustion**:
   - Check active queries: `SELECT * FROM pg_stat_activity`
   - Kill long-running queries if stuck
   - Increase max_connections in config (requires restart)

## Communication

| Audience | Channel | When |
|----------|---------|------|
| Engineering team | Slack #custody-incidents | All SEVs |
| Management | Email + Slack #leadership | SEV1-2 |
| Affected users | Email / in-app notification | SEV1 (fund loss) |
| Regulators | Formal report | SEV1 (if required by jurisdiction) |

## Post-Incident

- [ ] Incident timeline documented
- [ ] Root cause identified
- [ ] Fix deployed and verified
- [ ] Monitoring gap closed (new alert rule if needed)
- [ ] Retrospective meeting held
- [ ] Action items tracked to completion
