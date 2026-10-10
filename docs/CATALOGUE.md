# Crypto payment integration catalogue

Primary references checked 2026-10-10. No catalogue entry enables an account,
network, asset or commercial offer. The application supplies those choices.
There is no wallet custody, private spending key, member balance or transfer
service in this crate. Candidate availability is separate from implementation.

## Implemented and fixture-tested

| Integration | Present capability | Limit |
|---|---|---|
| x402 v2 | Offer/header handling, configured facilitator discovery, verification and settlement using upstream x402-types | The facilitator establishes the scheme's actual effect. Verification alone is not collected payment. Canonical authorization identity needs an injected scheme SDK strategy. |
| x402 HTTP transport | Explicit HTTPS facilitator/auth headers, bounded private response bytes/headers, no redirects or retries | Discovery, verification and settlement all require an encompassing bounded tariff. Standard x402 does not expose a final service fee or generic transaction-status API. |
| ReceiptReader extension | Injected authenticated read of a known network/transaction and optional fee, with available private upstream details | No built-in chain reader. A boolean cannot retroactively prove financial terms absent from the original settlement. One transaction is conservatively one spendable receipt in the bundled adapter. |
| BTCPay Server / CoinGate through hosted | Invoice/order creation and authenticated observation, reused directly from cpmt | No refund/payout/hold API. Partial, manual or ambiguous records cannot establish full payment. [Exact operations and credentials](https://git.corbet.ch/corbet-foss/cpmt/src/branch/main/docs/PROVIDERS.md). |

See [the contract](CONTRACT.md) and the
[shared credential contract](https://git.corbet.ch/corbet-foss/cpmt/src/branch/main/docs/CREDENTIALS.md).
Only real upstream code with inert fixtures was exercised. No production or
provider-sandbox account is qualified by these tests.

## Candidates, not implemented

| Candidate | Concrete integration and upstream route | Credentials, cost and evidence limits |
|---|---|---|
| Additional x402 schemes / facilitators | Reuse [x402-rs](https://github.com/x402-rs/x402-rs) scheme implementations behind the existing transport, AuthorizationIdentity and ReceiptReader seams. The current generic adapter already negotiates advertised v2 schemes; new scheme-specific claims need qualification. | Explicit endpoint/account headers, network/asset/recipient and canonical signed domain/authorizer/nonce. Bound all discovery/verification/settlement costs. Multi-payment transactions require allocation identity beyond transaction hash. Never reconstruct cryptographic verification locally. |
| Core Lightning | Official [cln-rpc Rust client](https://github.com/ElementsProject/lightning/blob/master/cln-rpc/README.md) for invoice creation and listinvoices reconciliation. A stable label enables recovery after a lost creation reply. | Host injects a restricted node transport capability; no automatic socket/key-file discovery or node installation. Scope authority to incoming invoice operations. Verify exact amount_received_msat, payment hash and authenticated node/network; invoice text alone is not payment. Node hosting/liquidity/service costs need an explicit tariff. [Invoice](https://docs.corelightning.org/reference/invoice), [status/amount](https://docs.corelightning.org/reference/listinvoices). |
| LND incoming invoices | Official protobuf/gRPC or REST contracts with maintained generated transport; AddInvoice and LookupInvoice form a candidate collection/read pair. Hold-invoice operations require a separate lifecycle integration. | Inject endpoint, server trust and narrowly scoped macaroon in memory; no admin credentials or wallet startup. Read invoice state and actual amount, retaining only its necessary payment reference. Node/service costs remain explicit; no assumption of free RPC or indefinite hold. [LookupInvoice](https://lightning.engineering/api-docs/api/lnd/lightning/lookup-invoice/), [macaroon authority](https://docs.lightning.engineering/lightning-network-tools/lnd/macaroons). |
| NOWPayments | Official payment create/read API via maintained reqwest/serde. Keep payment ID and distinguish confirming, partially_paid and finished; read with the same configured account key. | Inject x-api-key; callback IPN secret is a separate credential if authenticated webhooks are later added. Qualification must establish final amount/asset and lost-create recovery; order_id is not claimed as idempotency. Conversion, network and processing fees need current bounded quotes. Custody/balance/payout products are outside this proposed collection adapter. [Current provider integration guide](https://nowpayments.io/blog/nowpayments-api-explained-customize-your-payment-gateway). |
| Coinbase Payment Acceptance | Official authorize/capture APIs expose payment and operation IDs, with x-idempotency-key. Potential provider-managed conditional-payment integration; it is distinct from legacy Commerce and general wallet APIs. | Host-supplied authorized merchant access and request JWT through maintained signing libraries. Qualify asynchronous operation settlement, exact assets/amounts, expiry and feeBps/feeReceiver before any proof mapping. No account/wallet creation or reward/distribution implementation. Commercial availability and fees remain unverified. [Authorization](https://docs.cdp.coinbase.com/api-reference/payment-acceptance/payments/authorize-payment), [capture](https://docs.cdp.coinbase.com/api-reference/payment-acceptance/payments/capture-payment). |
| EVM receipt observation via Alloy | [Alloy](https://alloy.rs/) provides maintained Rust Ethereum RPC and ABI types. A receipt reader can verify an explicitly supported transfer/contract event against caller-selected chain/asset/recipient/amount. | Explicit read-only RPC endpoint/authentication and finality policy. Transaction success alone does not prove the requested transfer; identify its event/allocation and handle reorgs. RPC usage can be billed. No signing, broadcasting, wallet or invented chain consensus. This is a proposed reader, not implemented payment support. |
| Solana receipt observation | Maintained Solana/Agave client types and the official [getTransaction contract](https://solana.com/docs/rpc/http/gettransaction) are candidates for a bounded reader. | Inject RPC service credentials and explicit commitment/version support. Validate transaction error, exact instruction/account/asset effects and stable allocation identity; a signature alone is insufficient. RPC fees and observation calls require an encompassing quote. No transaction signing, key management or sending. |

## Extension and qualification requirements

The public payment Adapter contract remains open. Prefer current maintained
upstream types, transport, signing and validation code. A Rust API around a node
or TLS backend is not a claim that every transitive/native component is safe
Rust; our crate forbids unsafe code and every adopted graph must pass the normal
license/advisory checks. No private SDK forks, exact manifest pins or copied
cryptographic primitives.

Every candidate needs actual SDK/transport fixtures for authenticated request
construction, amount/recipient/network integrity, canonical identity, unexpected
fields, bounded private complete results, redaction, expiry at each dispatch,
replay/lost replies, reconciliation, fee reports and sandbox separation. Until
then it remains a candidate. No callback, checkout redirect or arbitrary JSON
constructs a production fact.

An upstream service may manage funds, nodes or durable records; this library does
not. Only host-authorized collection/read capabilities belong in these adapters.
All request/session/result data here stays in RAM. Minimal operational references
and atomic cost reservations cross the caller-owned state port; raw signatures,
KYC and complete responses do not. Explicit callers can inspect available private
upstream results without making them gate evidence.

No price is asserted by this catalogue. Every potentially billed action needs
an exact or defensibly bounded quote/tariff with source, denomination and expiry.
Principal, protocol/network fees and operator service costs are distinct.
Simulation performs no calls; sandbox is explicit and is not presumed free.
Unexpected charges and failed caller-state commits must remain visible and stop
proof/fallback. A new operation ID is never a safe response to uncertain payment.
