## Summary

<!-- 1-3 sentences: what changed and why -->

## Changes

- [ ] New feature
- [ ] Bug fix
- [ ] Security fix
- [ ] Refactor
- [ ] Documentation
- [ ] Dependencies

## Security Checklist

<!-- Required for changes to: signing, key management, withdrawal pipeline, auth -->

- [ ] No key material in logs (`tracing::` calls reviewed)
- [ ] No f64 for monetary amounts (Decimal only)
- [ ] Circuit breaker limits not bypassed
- [ ] RBAC guards on all new endpoints
- [ ] Input validation at system boundary
- [ ] No new `unwrap()` in production paths

## Testing

- [ ] `cargo test` passes
- [ ] New tests added for changed behavior
- [ ] Tested on testnet (if chain-specific)

## Changelog

<!-- Add entry to CHANGELOG.md [Unreleased] section -->
