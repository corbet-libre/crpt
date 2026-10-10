# crpt

Reusable crypto payment adapters, using the same execution contract as `cpmt`.
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
The host supplies an atomic `Journal` port and stable secret binding key; the library has no database implementation. `terms` is a
server-created `x402::Offer`; `evidence` is decoded `PAYMENT-SIGNATURE` input.
Header helpers use standard base64 encoding. A successful settlement's
`member_action` contains the `PAYMENT-RESPONSE` header value for that authenticated
member only. Gate consumers receive requested minimal facts. `Payments::paid_access` projects explicitly configured financial terms into an access level and period.

## Dependencies and transport

`x402-types` supplies the upstream v2 types and `Facilitator` interface. Any
compatible SDK facilitator can be injected as `facilitator::Sdk(upstream)`. The optional `http` feature provides
a small `reqwest` transport with explicit HTTPS endpoint/authentication, timeout,
response limits, disabled redirects and disabled automatic retries. No endpoints,
credentials or environment variables are discovered. We use upstream libraries
for protocol types, HTTP, encoding and cryptographic primitives.

The [shared credential contract](https://git.corbet.ch/corbet-foss/cpmt/src/branch/main/docs/CREDENTIALS.md)
describes host injection, dynamic authentication, credential replacement and the
separate stable payment-journal key.

Surveyed [x402-rs](https://github.com/x402-rs/x402-rs) and
[r402](https://github.com/qntx/r402). x402-rs's transport-independent interface fits
the existing gate model without importing a resource server, wallet or payment
policy. Its bundled Axum client does not expose the HTTP-client configuration
needed for our redirect/retry/size boundary; the transport here implements that
upstream interface without copying chain or signature code. Dependencies use
current semver ranges and first-party `main`, never exact manifest pins.

## Privacy, retries and reconciliation

The default path discards payer addresses, signatures, KYC fields and raw
provider responses. Explicit `Disclosure::Complete` callers receive available
results in private transient buffers, including unknown HTTP extensions. The
caller-owned atomic state port receives only the transaction/network reference needed for
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

The advisory check denies known vulnerabilities and reports maintenance notices
as warnings. The current x402-types → alloy-primitives graph still uses `paste`
for compile-time token generation; RustSec marks that macro crate unmaintained
([RUSTSEC-2024-0436](https://rustsec.org/advisories/RUSTSEC-2024-0436.html)).
This remains visible in CI rather than introducing a private upstream fork.

Enable `hosted` for BTCPay Server Greenfield invoices and CoinGate hosted orders:
`crpt::hosted::{Rest, Provider, Limits}` reuses cpmt's implementation and its
journal/gate contract. Select `Provider::Btcpay` or `Provider::CoinGate`; inject
the operator's credential and explicit instance/store or sandbox configuration.
[Supported operations and limits](https://git.corbet.ch/corbet-foss/cpmt/src/branch/main/docs/PROVIDERS.md)
include read-only reconciliation, private checkout-link handoff, amount checks,
and refusal to treat manual BTCPay marking as settlement. The dependency also
exposes the fiat profiles; availability does not decide what the host offers.

## Cost authorization and modes

Read the [domain contract](docs/CONTRACT.md) and [integration catalogue](docs/CATALOGUE.md) before integration. Configure each
adapter with `Mode::Production` or `Mode::Sandbox`; the host owns endpoint/account
selection. Build an inert preview with `Payments::preview`, a trusted tariff and
its provenance/expiry, then use `cfbk` (re-exported as `cost`) to obtain a move-only
permit. `dispatch` and fresh `observation` instructions both require that permit.
The host's atomic journal reserves operation identity and maximum fee together.
There is no uncosted execution facade and no implicit free facilitator call.

`run` uses the shared planner for an explicit caller-selected cascade, checking
all alternatives before dispatch and stopping on success, pending or uncertainty.
`simulation::simulate` uses dummy outcomes and never contacts a provider. Sandbox
is separate and cannot produce production paid-access evidence. Hosted adapters
through the `hosted` feature use exactly the same `cpmt` implementation.

The legacy upstream `HttpFacilitator: Facilitator` methods are transport primitives;
use it directly as `X402`'s response-bearing transport to preserve complete HTTP
results through the costed payment boundary. Wrapping it in `Sdk` intentionally
has the upstream trait's typed-data limits. No live provider/chain/sandbox call is
part of the test suite; fixture success does not qualify a production account.

The host supplies a live `cost::Clock` to `Payments::new`. The adapter checks fresh
time before discovery, verification, settlement and receipt observation. If an
earlier phase exhausts the deadline, later phases are not called; explicit private
results already received remain available with an Unknown outcome. The quote must
cover the entire operation. SDK futures run with tracing suppressed, and injected
HTTP headers are marked sensitive.

The shared executor preserves unexpected authenticated fees in `cost.violation`
and cascade `charge_violation`, keeping them distinct from the original maximum.
Failed final caller-state writes return `commit_error` and Unknown with explicit
private reconciliation data; they cannot produce proof or automatic fallback.

Rich `facilitator::Transport` and `ReceiptReader` implementations receive a
borrowed `facilitator::DispatchGuard`. They must call `check()` after any queue or
await and immediately before each network dispatch. It retains the actual adapter
account/mode and live quote deadline; it cannot be detached from the dispatch.
The concrete HTTP transport performs this check after request construction.

`facilitator::Sdk` checks at the upstream invocation. Its host must configure an
SDK with no internal queue, automatic retries, detached work or hidden extra
calls, because the upstream Facilitator trait cannot accept a deadline. An SDK
that needs such a queue must implement the rich Transport seam and carry the
borrowed guard to its actual I/O edge. The compatibility Facilitator methods on
HttpFacilitator remain low-level protocol primitives; use its rich Transport
implementation directly inside X402 for enforced phase authorization.
