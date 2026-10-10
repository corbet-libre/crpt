# Crypto payment domain contract

`crpt` executes caller-authorized crypto payment tasks through maintained upstream
protocol implementations. It reuses `cpmt` for operation identity, atomic caller
state, cost permits, rich/minimal output, paid access, simulation and fallback.
There is no custody, wallet key management, transfer between members, database
connection, SQL implementation, file cache or hidden durable SDK state.

The caller chooses financial terms, desired fact, access level and period.
An adapter establishes only its protocol's actual guarantee. x402 verification
is not collection; settlement has the configured scheme's meaning. The
`paid_access` transformation requires the exact authenticated instruction's terms
to be satisfied and caps the requested interval by authenticated validity.
Amounts, assets, networks and provider IDs stay out of that projection.

Every execution and observation consumes an explicit shared cost permit. Preview
is local and takes an explicit bounded tariff/quote with provenance and validity;
unknown cost cannot authorize dispatch. The quote must include every facilitator
phase and any read charge. Principal is not the operator service fee. Built-in
x402 replies do not establish fees, so actual cost remains unknown; an injected
receipt reader may report an authenticated fee. Uncertainty keeps maximum cost
reserved and cannot trigger a second settlement.

The host implements `cpmt::payments::Journal` and atomically reserves operation
identity and fee budget across processes/restarts. Only minimal operational
references, keyed receipt/context identities and reservations reach it. Raw
requests, KYC, complete results and member actions never enter that port. Library
buffers stay in RAM; host swap, dumps and SDK memory copies remain host concerns.

Minimal output is the default. `Disclosure::Complete` returns available upstream
responses through redacted, transient `ProviderReply` buffers. Arbitrary JSON
cannot construct a proof. `facilitator::Sdk` reuses any upstream `Facilitator`
and relays complete available typed responses; fields already discarded by an
SDK cannot be recovered. `HttpFacilitator` implements the richer transport seam
and preserves available response headers, unknown fields and non-success bodies within its explicit
size bound. Local transport/configuration diagnostics stay redacted.

Simulation uses the same `cfbk` planning, bound terms and fallback classification
with explicit dummy results, no credentials and no I/O. It has no production
proof type. Sandbox is explicit, still cost-authorized, and cannot mint production
evidence. No provider is enabled by inclusion in the library.

Owned code forbids unsafe Rust. Dependencies use current compatible ranges and
first-party main branches; lockfiles record the tested graph. Integration tests
use maintained protocol types and inert loopback fixtures. Live facilitators,
chain transactions and sandbox sessions are not run or qualified by those tests.

The injected trusted `cost::Clock` must advance across awaited work. Each actual
facilitator/read phase rechecks the effective deadline. If expiry occurs after a
phase, return available private replies as Unknown and retain fee exposure;
never begin a later phase with the old quote. SDK tracing is suppressed within
these calls; injected HTTP authentication headers are sensitive.

Unexpected authenticated fees survive in the shared Reservation violation field.
A failed final state commit returns `Execution::commit_error`, Unknown and
available explicit private details without proof or member handover. Host
reconciliation must preserve that charge knowledge before more spending in the
affected budget scope. See the [shared payment contract](https://git.corbet.ch/corbet-foss/cpmt/src/branch/main/docs/CONTRACT.md)
and [candidate catalogue](CATALOGUE.md).
