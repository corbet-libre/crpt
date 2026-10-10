# crpt

Crypto payment adapters for cvld, using the same execution contract as `cpmt`.
The caller chooses what to collect and what a successful fact permits. There is
no wallet custody, monetary account ledger, member-to-member transfer, preferred
chain, currency list or automatic provider fallback.

The bundled x402 v2 adapter supports the HTTP payment-required/signature/response
exchange, facilitator capability discovery, verification and settlement. Any
network/scheme advertised by the configured facilitator can be selected in the
caller's offer. An accepted payment must match that offer's amount, asset,
recipient, network, scheme and extra parameters. Crypto verification and chain
execution belong to the maintained facilitator implementation.

| Operation | Result | Side effect |
|---|---|---|
| `x402.verify` | `Verified` | Facilitator verifies authorization; no settlement |
| `x402.settle` | `Settled` after explicit success and a matching network receipt | Facilitator executes the requested settlement step |
| `observe` | Current settlement fact through an injected `ReceiptReader` | Read-only; never repeats settlement |

`Settled` means that the selected scheme completed its settlement step. The
scheme determines whether that step collects money, creates an escrow hold or
performs another effect. A verified authorization is neither collected money nor
a guaranteed hold. The caller owns resource execution order and eligibility.
Verification alone emits no spendable gate receipt by default. To use a verified
but unsettled authorization as gate evidence, inject `AuthorizationIdentity`
backed by the selected scheme's maintained SDK. It must derive a stable identity
from the signed domain/authorizer/nonce and an authenticated expiry; unsigned metadata and
signature encodings cannot create new receipts or extend that expiry.
This keeps authorization, upfront and escrow flows outside a hardcoded core policy.

Register `x402::X402` with `payments::Payments`, which is re-exported from `cpmt`.
Use a durable shared payment journal and stable secret binding key. `terms` is a
server-created `x402::Offer`; `evidence` is decoded `PAYMENT-SIGNATURE` input.
Header helpers use standard base64 encoding. A successful settlement's
`member_action` contains the `PAYMENT-RESPONSE` header value for that authenticated
member only. Gate consumers receive only `payments::Evidence`.

## Dependencies and transport

`x402-types` supplies the upstream v2 types and `Facilitator` interface. Any
compatible SDK facilitator can be injected. The optional `http` feature provides
a small `reqwest` transport with explicit HTTPS endpoint/authentication, timeout,
response limits, disabled redirects and disabled automatic retries. No endpoints,
credentials or environment variables are discovered. We use upstream libraries
for protocol types, HTTP, encoding and cryptographic primitives.

Surveyed [x402-rs](https://github.com/x402-rs/x402-rs) and
[r402](https://github.com/qntx/r402). x402-rs's transport-independent interface fits
the existing gate model without importing a resource server, wallet or payment
policy. Its bundled Axum client does not expose the HTTP-client configuration
needed for our redirect/retry/size boundary; the transport here implements that
upstream interface without copying chain or signature code. Dependencies use
current semver ranges and first-party `main`, never exact manifest pins.

## Privacy, retries and reconciliation

The payment boundary discards payer addresses, signatures, KYC fields and raw
provider responses. It retains only the transaction/network reference needed for
configured reconciliation; the shared executor stores keyed receipt/context
bindings. `forget` removes continuation while preserving replay protection.
A failed, missing or malformed settlement response is `Unknown`, never success.
A pending transaction reference is retained only to allow read-only reconciliation.
The bundled adapter uses network plus transaction as the settlement receipt
identity. Schemes that put multiple independently spendable payments into one
transaction need a scheme adapter with a canonical allocation identity; the
bundled adapter conservatively treats that transaction as one receipt.

Core x402 specifies no transaction-status endpoint. Supply a `ReceiptReader`
backed by a maintained provider/chain SDK when reconciliation is needed; without
one, observation is explicitly unsupported. If no transaction reference survived
an uncertain response, provider-side reconciliation is required. Never issue a
new operation ID simply to retry a payment. Verification-only flows must carry
the authorization in memory through resource execution or obtain it again from
the payer for settlement; this library does not persist raw authorizations.

`cgts::payments::PaymentGate` consumes the shared evidence with an atomic spent
receipt. `cgbl::payments::PaymentGate` accepts an in-process fact and does not
claim person uniqueness. Further crypto platforms implement `payments::Adapter`;
additional currencies do not require changing the core.

Tests use local HTTP fixtures and external-provider fixtures only. No live chain,
wallet keys or payment credentials are used.
