//! x402 v2 adapter. Maintained upstream types and facilitators own the protocol.
use crate::facilitator::Transport;
use crate::payments::*;
use crate::{
    cost::{Amount, Dispatch, Mode},
    details::ProviderReply,
};
use async_trait::async_trait;
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;
use tracing::instrument::WithSubscriber;
use x402_types::proto::{self, v2};

/// HTTP response header for a base64-encoded x402 offer.
pub const PAYMENT_REQUIRED: &str = "PAYMENT-REQUIRED";
/// HTTP request header for a base64-encoded signed payment payload.
pub const PAYMENT_SIGNATURE: &str = "PAYMENT-SIGNATURE";
/// HTTP response header for a base64-encoded settlement result.
pub const PAYMENT_RESPONSE: &str = "PAYMENT-RESPONSE";

/// Upstream x402 v2 offer. The caller chooses all payment terms.
pub type Offer = v2::PaymentRequired;

/// Optional read-only chain/provider reconciliation, implemented with its SDK.
/// Core x402 has no standard transaction-status endpoint; never retry `/settle`
/// to implement this interface.
#[async_trait]
pub trait ReceiptReader: Send + Sync {
    /// Confirm whether this previously returned settlement still holds.
    async fn settled(&self, network: &str, transaction: &str) -> Result<ReceiptObservation>;
}

/// Read-only provider result, with explicit transient details and fee reporting.
pub struct ReceiptObservation {
    /// Whether the previously returned settlement still holds.
    pub settled: bool,
    /// Complete available response data; it never becomes a gate proof by itself.
    pub upstream: Vec<ProviderReply>,
    /// Authenticated service charge, separately from payment principal.
    pub actual_cost: Option<Amount>,
}

/// Canonical scheme authorization facts, established using a maintained scheme SDK.
/// No unsigned metadata or alternate signature encoding may change this identity.
pub struct Authorization {
    /// Stable identity including signing domain, authorizer and authorization nonce.
    pub identity: Private,
    /// Exclusive authorization expiry derived from the verified scheme payload.
    pub valid_until: u64,
}

/// Trusted scheme-specific extraction after the facilitator verifies a payload.
/// Implementations must reject unsupported schemes and derive the identity and
/// expiry from authenticated fields. Resource metadata/extensions are not identity.
pub trait AuthorizationIdentity: Send + Sync {
    /// Extract canonical authenticated facts without retaining the authorization.
    fn identify(&self, verified: &proto::VerifyRequest) -> Result<Authorization>;
}

/// x402 capability over any maintained upstream facilitator implementation.
pub struct X402<F> {
    mode: Mode,
    id: String,
    facilitator: F,
    max_input_bytes: usize,
    reader: Option<Arc<dyn ReceiptReader>>,
    identity: Option<Arc<dyn AuthorizationIdentity>>,
}

#[derive(Serialize, Deserialize)]
struct Receipt {
    network: String,
    transaction: String,
    verified_terms: bool,
}

impl<F: Transport> X402<F> {
    /// Configure a trusted facilitator and an explicit payload bound.
    /// No currency, chain, scheme, wallet or provider is selected implicitly.
    pub fn new(id: String, facilitator: F, max_input_bytes: usize, mode: Mode) -> Result<Self> {
        if id.is_empty()
            || id.len() > 256
            || id.chars().any(char::is_control)
            || max_input_bytes == 0
            || mode == Mode::Simulation
        {
            return Err(Error::Invalid);
        }
        Ok(Self {
            mode,
            id,
            facilitator,
            max_input_bytes,
            reader: None,
            identity: None,
        })
    }

    /// Supply a trusted read-only settlement observer for this facilitator.
    pub fn with_reader(mut self, reader: Arc<dyn ReceiptReader>) -> Self {
        self.reader = Some(reader);
        self
    }

    /// Enable spendable verification evidence for schemes with canonical identity
    /// and authenticated expiry extraction. Without this, verify reports a fact
    /// but cannot mint gate evidence. Settlement receipts are unaffected.
    pub fn with_authorization_identity(mut self, identity: Arc<dyn AuthorizationIdentity>) -> Self {
        self.identity = Some(identity);
        self
    }

    fn wire(
        &self,
        request: &Instruction,
    ) -> Result<(proto::VerifyRequest, v2::PaymentRequirements)> {
        if request
            .terms
            .expose()
            .len()
            .saturating_add(request.evidence.expose().len())
            > self.max_input_bytes
        {
            return Err(Error::Invalid);
        }
        let offer: Offer =
            serde_json::from_slice(request.terms.expose()).map_err(|_| Error::Invalid)?;
        let payload: v2::PaymentPayload<v2::PaymentRequirements, Value> =
            serde_json::from_slice(request.evidence.expose()).map_err(|_| Error::Invalid)?;
        // Use the server's offer, never requirements supplied solely by the payer.
        let accepted = offer
            .accepts
            .into_iter()
            .find(|terms| terms == &payload.accepted)
            .ok_or(Error::Rejected)?;
        let offered_resource = offer.resource.ok_or(Error::Invalid)?;
        if payload
            .resource
            .as_ref()
            .is_some_and(|resource| resource.url != offered_resource.url)
        {
            return Err(Error::Rejected);
        }
        let wire = v2::VerifyRequest {
            x402_version: v2::X402Version2,
            payment_payload: payload,
            payment_requirements: accepted.clone(),
        };
        Ok((
            proto::VerifyRequest::try_from(&wire).map_err(|_| Error::Invalid)?,
            accepted,
        ))
    }
}

impl<F: Transport> Adapter for X402<F> {
    fn mode(&self) -> Mode {
        self.mode
    }
    fn max_input_bytes(&self) -> usize {
        self.max_input_bytes
    }
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            id: self.id.clone(),
            operations: vec!["x402.verify".into(), "x402.settle".into()],
        }
    }
    async fn execute(
        &self,
        request: &Dispatch<'_>,
        _key: &str,
        previous: Option<&Private>,
    ) -> Result<Observation> {
        request.check(&self.id, self.mode)?;
        if request.kind != Action::Execute {
            return Err(Error::Invalid);
        }
        if !self.capabilities().operations.contains(&request.operation) {
            return Err(Error::Unsupported);
        }
        if previous.is_some() {
            return Err(Error::Invalid);
        }
        let (wire, requirements) = self.wire(request)?;
        request.check(&self.id, self.mode)?;
        let supported = self
            .facilitator
            .supported_details()
            .with_subscriber(tracing::subscriber::NoSubscriber::default())
            .await;
        let mut upstream = supported.upstream;
        let supported = match supported.result {
            Ok(value) => value,
            Err(_) => {
                return Ok(Observation {
                    upstream,
                    ..Observation::fact(State::Unknown)
                });
            }
        };
        if !supported.kinds.iter().any(|kind| {
            kind.x402_version == 2
                && kind.scheme == requirements.scheme
                && kind.network == requirements.network.to_string()
        }) {
            return Ok(Observation {
                upstream,
                ..Observation::fact(State::Failed)
            });
        }
        if request.check(&self.id, self.mode).is_err() {
            return Ok(Observation {
                upstream,
                ..Observation::fact(State::Unknown)
            });
        }
        let verified = self
            .facilitator
            .verify_details(&wire)
            .with_subscriber(tracing::subscriber::NoSubscriber::default())
            .await;
        upstream.extend(verified.upstream);
        let verified = match verified.result {
            Ok(value) => value,
            Err(_) => {
                return Ok(Observation {
                    upstream,
                    ..Observation::fact(State::Unknown)
                });
            }
        };
        if verified.0.get("isValid").and_then(Value::as_bool) != Some(true) {
            return Ok(Observation {
                upstream,
                ..Observation::fact(State::Failed)
            });
        }
        if request.operation == "x402.verify" {
            // Verification is not a hold or settlement. The caller decides whether
            // this weaker fact permits its resource to execute before settlement.
            let mut observation = Observation::fact(State::Verified);
            observation.upstream = upstream;
            observation.satisfies_terms = true;
            if let Some(identity) = &self.identity {
                let authorization = match tracing::subscriber::with_default(
                    tracing::subscriber::NoSubscriber::default(),
                    || identity.identify(&wire),
                ) {
                    Ok(value) if !value.identity.expose().is_empty() => value,
                    _ => {
                        return Ok(Observation {
                            upstream: observation.upstream,
                            ..Observation::fact(State::Failed)
                        });
                    }
                };
                let claim = serde_json::to_vec(&(
                    "x402.authorization.v2",
                    &requirements.scheme,
                    requirements.network.to_string(),
                    authorization.identity.expose(),
                ))
                .expect("authorization identity fields serialize to JSON");
                observation.claim = Some(Private::new(claim));
                observation.valid_until = Some(authorization.valid_until.min(request.expires_at));
            }
            return Ok(observation);
        }
        if request.check(&self.id, self.mode).is_err() {
            return Ok(Observation {
                upstream,
                ..Observation::fact(State::Unknown)
            });
        }
        let exchange = self
            .facilitator
            .settle_details(&wire)
            .with_subscriber(tracing::subscriber::NoSubscriber::default())
            .await;
        upstream.extend(exchange.upstream);
        let reply = match exchange.result {
            Ok(reply) => reply.0,
            Err(_) => {
                return Ok(Observation {
                    upstream,
                    ..Observation::fact(State::Unknown)
                });
            }
        };
        // Even a negative settlement response may describe an in-flight transaction.
        // Only explicit success with a receipt in the requested network is evidence.
        let receipt = reply
            .get("network")
            .and_then(Value::as_str)
            .filter(|network| *network == requirements.network.to_string())
            .zip(
                reply
                    .get("transaction")
                    .and_then(Value::as_str)
                    .filter(|transaction| !transaction.is_empty()),
            )
            .map(|(network, transaction)| Receipt {
                network: network.into(),
                transaction: transaction.into(),
                verified_terms: reply.get("success").and_then(Value::as_bool) == Some(true),
            });
        let Some(receipt) = receipt else {
            return Ok(Observation {
                upstream,
                ..Observation::fact(State::Unknown)
            });
        };
        if reply.get("success").and_then(Value::as_bool) != Some(true) {
            return Ok(Observation {
                continuation: Some(Private::new(
                    serde_json::to_vec(&receipt).map_err(|_| Error::Invalid)?,
                )),
                upstream,
                ..Observation::fact(State::Unknown)
            });
        }
        let response = serde_json::json!({"success":true,"network":receipt.network,"transaction":receipt.transaction});
        let handover = Private::new(
            STANDARD
                .encode(serde_json::to_vec(&response).map_err(|_| Error::Invalid)?)
                .into_bytes(),
        );
        let mut observation = settled(receipt)?;
        observation.member_action = Some(handover);
        observation.upstream = upstream;
        Ok(observation)
    }
    async fn observe(
        &self,
        request: &Dispatch<'_>,
        continuation: Option<&Private>,
    ) -> Result<Observation> {
        request.check(&self.id, self.mode)?;
        if request.kind != Action::Observe {
            return Err(Error::Invalid);
        }
        let reader = self.reader.as_ref().ok_or(Error::Unsupported)?;
        let receipt: Receipt =
            serde_json::from_slice(continuation.ok_or(Error::Unsupported)?.expose())
                .map_err(|_| Error::Invalid)?;
        request.check(&self.id, self.mode)?;
        let read = reader
            .settled(&receipt.network, &receipt.transaction)
            .with_subscriber(tracing::subscriber::NoSubscriber::default())
            .await?;
        let mut result = if read.settled {
            settled(receipt)?
        } else {
            Observation {
                continuation: Some(Private::new(
                    serde_json::to_vec(&receipt).map_err(|_| Error::Invalid)?,
                )),
                ..Observation::fact(State::Unknown)
            }
        };
        result.upstream = read.upstream;
        result.actual_cost = read.actual_cost;
        Ok(result)
    }
}

fn settled(receipt: Receipt) -> Result<Observation> {
    Ok(Observation {
        satisfies_terms: receipt.verified_terms,
        claim: Some(Private::new(
            serde_json::to_vec(&("x402.settlement.v2", &receipt.network, &receipt.transaction))
                .expect("string tuples serialize to a byte vector"),
        )),
        continuation: Some(Private::new(
            serde_json::to_vec(&receipt).map_err(|_| Error::Invalid)?,
        )),
        ..Observation::fact(State::Settled)
    })
}

/// Encode the caller's offer for `PAYMENT-REQUIRED`. Contains no member data.
pub fn payment_required(offer: &Offer) -> Result<String> {
    if offer.accepts.is_empty() || offer.resource.is_none() {
        return Err(Error::Invalid);
    }
    Ok(STANDARD.encode(serde_json::to_vec(offer).map_err(|_| Error::Invalid)?))
}

/// Decode `PAYMENT-SIGNATURE` into bounded private evidence for an instruction.
/// Syntax and offered terms are checked again by the adapter before verification.
pub fn payment_signature(header: &str, max_encoded_bytes: usize) -> Result<Private> {
    if header.len() > max_encoded_bytes {
        return Err(Error::Invalid);
    }
    let bytes = STANDARD.decode(header).map_err(|_| Error::Invalid)?;
    Ok(Private::new(bytes))
}

#[cfg(all(test, not(target_arch = "wasm32")))]
#[path = "tests/x402.rs"]
mod tests;
