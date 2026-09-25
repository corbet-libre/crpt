# crpt

**Crypto-payment and blockchain admission gate for cvld.**

`crpt` will prove blockchain facts for `cvld`: ownership of a wallet, an on-chain payment, or a stake, wrapping established chain libraries. Fiat payments are `cpmt`'s job.

Status: name reserved, no implementation yet.

## Boundaries

What it does:

- Verifies wallet ownership, on-chain payments and stakes and returns an opaque gate proof with its trust anchor.

What it never does:

- Never holds keys or custody of funds.
- Wallet addresses never reach `cvld` beyond the opaque proof.
- No own security primitives: a thin wrapper around the established best-practice library for the factor.

## License

Copyright 2026 Julian Y. Richard Corbet. Licensed under
[LGPL-3.0-only](LICENSES/LGPL-3.0-only.txt)
[WITH LGPL-3.0-linking-exception](LICENSES/LGPL-3.0-linking-exception.txt).
See [LICENSE.md](LICENSE.md).
