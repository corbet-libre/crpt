# Agent instructions

- Implement crypto payment adapters using cpmt's shared execution contract.
  Callers own commercial policy. Keep platform, network and currency support in
  adapters; x402 is a protocol adapter, not a preferred cryptocurrency.
- Preserve the information boundary documented in README.md. No custody, private
  wallet keys, raw gate evidence retention or member-to-member transfers.
- Reuse maintained upstream SDKs/types; no own cryptographic primitives.
  Use semver ranges and first-party `branch = "main"`, not exact manifest pins.
- LGPL-3.0-only WITH LGPL-3.0-linking-exception; no GPL/AGPL-only dependencies.
- Fixtures only. No live billing, chain calls, spending or provider registration.
- Workstations edit/format; Crow compiles and tests committed candidates.
  Do not claim completion before required checks pass. Forgejo is primary.
- All comments/docs are English. Preserve source coverage and privacy checks.

- Follow docs/CONTRACT.md: all domain data stays in RAM; no direct database or
  file persistence. Caller state receives only minimum operational records.
- Complete private results are explicit; arbitrary provider JSON is not proof.
- Cost permits precede every dispatch, including reads. Sandbox and simulation
  cannot construct production evidence. Owned code forbids unsafe Rust.
