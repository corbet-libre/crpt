# crpt agent notes

Work in a lane (`~/agents/scratch/lane-new <lane>`); never edit `~/agents/scratch/ws`.
Run commands from the lane root (`<lane>/ws`); `<lane>` below is your lane name.

Build and test (quote the verdict line, never infer from a pipe):
- `~/agents/scratch/cbuild <lane> test -p crpt --all-features` — green, exit 0
  (21 lib tests; fixtures only, no live billing, chain, or provider calls).
- `cargo fmt -p crpt -- --check` and
  `clippy -p crpt --all-features --all-targets -- -D warnings` — clean/green.
- `check -p crpt --target wasm32-unknown-unknown` — green, exit 0.
- Full gate (locked deps, advisories via `cargo deny`, coverage) runs in CI
  only: `.ci/verify.sh`. Lane `--locked` needs CI's resolve refresh.

Land: `cargo fmt` in touched repos, rebase on main (never force-push),
`cbuild <lane> xtask land --dry`, then `xtask land`.

Invariants (rule -> enforcing test):
- Verification is not collection and never calls settlement
  -> `src/tests/x402.rs::verification_is_not_collection_and_does_not_call_settlement`
- Settlement needs trusted terms and matching amount/asset/network/recipient
  -> `settlement_uses_trusted_terms_and_reduces_every_provider_response`,
  `client_cannot_change_amount_asset_network_recipient_or_resource`
- Unknown/malformed settlement responses are `Unknown`, never success; timeouts
  and wrong networks never prove success
  -> `settlement_timeout_failure_and_wrong_network_never_prove_success`
- Reconciliation is read-only through an injected `ReceiptReader`; no reader
  means observation is unsupported
  -> `reconciliation_is_read_only_and_requires_a_provider_observer`
- Unsigned metadata cannot multiply receipts or extend expiry
  -> `unsigned_metadata_cannot_multiply_verification_receipts_or_extend_authorization_expiry`
- Expired quotes stop later phases; private replies stay private
  -> `delayed_facilitator_phases_cannot_use_an_expired_quote_for_the_next_call`,
  `detailed_transport_keeps_extensions_and_failed_bodies_private_per_call`
- Every dispatch and observation needs a cost permit; sandbox cannot mint
  production evidence
  -> `complete_results_are_explicit_and_sandbox_cannot_grant_paid_access`
- Fixtures only, loopback HTTP only
  -> `local_fixture_transport_is_restricted_to_loopback_http`;
  `forbid(unsafe_code)` (`src/lib.rs`).

Pointers: `src/x402.rs`, `src/facilitator.rs`, `docs/CONTRACT.md`,
`docs/CATALOGUE.md`.
Commits: explicit paths, imperative, author per `~/agents/AGENT.md`; no AI trailers.
`publish=false`; LGPL-3.0-only WITH linking exception; no registry publication.
