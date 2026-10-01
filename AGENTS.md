# Agent instructions

Write all code comments and documentation in English.

## Product boundary

- Crypto-payment and blockchain gate for cvld: wallet ownership, on-chain payments and staking.
- Verifies wallet ownership, on-chain payments and stakes and returns an opaque gate proof bound to its configured provider.
- Never holds keys or custody of funds.
- Wallet addresses never reach `cvld` beyond the opaque proof.
- No own security primitives: a thin wrapper around the established best-practice library for the factor.
- This crate is LGPL-3.0-only WITH LGPL-3.0-linking-exception. Do not add implementation code available only under the full GPL or AGPL.
- Tests use fixtures only; no live chain or node calls.

## Quality boundary

- `cargo fmt --check`, `cargo clippy --all-targets` (no warnings),
  `cargo test` — all green before every commit. No local workstation
  builds; use GHA, Crow as fallback.
