# Integration Tests

Top-level E2E tests for meaningful protocol behavior across phase boundaries.

Current coverage:

- Token-2022 `transfer_checked` invokes `compliance_hook`
- blocked receiver prevents a real Token-2022 transfer
- daily transfer limit blocks the third real Token-2022 transfer
- paused hook prevents a real Token-2022 transfer
- fake source compliance account cannot authorize a real transfer

Run after building the hook program:

```bash
cd ../07-transfer-hook-compliance
anchor build --ignore-keys
cd ../integration-tests
cargo test
```
