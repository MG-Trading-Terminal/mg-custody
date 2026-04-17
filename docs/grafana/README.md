# Grafana dashboards

Drop-in dashboards for operators running mg-custody.

## readiness-dashboard.json

Feature-readiness view — the same data as the `/readiness` page in
the custody dashboard, but in a panel format that integrates with
your existing Grafana + Alertmanager stack.

### Import

1. Grafana → Dashboards → New → Import → Upload JSON file.
2. When prompted, pick your Prometheus datasource (the one scraping
   `/metrics` on the custody service).
3. Dashboard appears under the `mg-custody` tag.

### Panels

| Panel | Source metric | Alert-worthy when |
|---|---|---|
| Features not Ready | `custody_readiness_feature_state >= 2` | `> 0` |
| Readiness refresh age | `time() - custody_readiness_last_refresh_seconds` | `> 180s` |
| Probe sweep p99 | `custody_readiness_build_duration_seconds` histogram | `> 15s` |
| Ready features | `count(custody_readiness_feature_state == 1)` | drops sharply without a deploy |
| Feature state timeline | `custody_readiness_feature_state` | any line leaves "Ready" row |
| Current state per feature | same, snapshot | any bar is yellow/red |
| Probe sweep duration p50/p95/p99 | histogram | all three rising together |

### Suggested Alertmanager rules

```yaml
groups:
  - name: mg-custody-readiness
    rules:
      - alert: CustodyFeatureDegraded
        expr: custody_readiness_feature_state >= 2
        for: 2m
        labels: { severity: page }
        annotations:
          summary: "{{ $labels.feature }} is not Ready"
          description: "Handlers serving {{ $labels.feature }} are returning 503. See /readiness."

      - alert: CustodyReadinessStale
        expr: time() - custody_readiness_last_refresh_seconds > 180
        for: 1m
        labels: { severity: page }
        annotations:
          summary: "Readiness refresh task has stopped"
          description: "Live state is stale; chaos override and silence effects are frozen."

      - alert: CustodyProbeSweepSlow
        expr: histogram_quantile(0.99, sum(rate(custody_readiness_build_duration_seconds_bucket[15m])) by (le)) > 15
        for: 10m
        labels: { severity: warn }
        annotations:
          summary: "Probe sweep p99 > 15s"
          description: "Vendor latency creep — check which probe dominates the sweep."
```

The in-service webhook fires faster and with more context (the
exact transition reason) than a Prometheus-driven page, but these
rules are the safety net: if the refresh task itself hangs, its own
transition detection won't fire but `CustodyReadinessStale` will.

### Receiving webhook signatures

Outgoing alert webhooks include an `X-Custody-Signature` header when
`CUSTODY_READINESS__ALERT_WEBHOOK_SECRET` is set. Receiver code
should:

1. Read the header, split `t=...,v1=...`.
2. Reject if `|now − t|` exceeds your tolerance (30s is typical).
3. Recompute `hmac_sha256(secret, f"{t}.{raw_body}").hex()`.
4. Constant-time compare against `v1`.

Payload shape:

```json
{
  "service": "mg-custody",
  "timestamp": "2026-04-18T12:34:56.789Z",
  "transitions": [
    { "kind": "degradation",    "feature": "scanner.ethereum", "reason": "eth_chainId timeout" },
    { "kind": "misconfiguration","feature": "push.apns",       "missing": ["CUSTODY_APNS__KEY_P8"] },
    { "kind": "recovery",       "feature": "scanner.bitcoin",  "summary": "Bitcoin scanner + broadcaster" }
  ]
}
```

Multiple transitions bundled in one POST = one page for a whole-
region outage, not N individual pages.
