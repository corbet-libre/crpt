use super::*;
use crate::facilitator::{Sdk, Transport};
use crate::test_support::FixturePayments;
use std::sync::{
    Mutex,
    atomic::{AtomicUsize, Ordering},
};
use x402_types::facilitator::Facilitator;

#[derive(Clone)]
struct Remote {
    calls: Arc<AtomicUsize>,
    requests: Arc<Mutex<Vec<Value>>>,
    reply: Value,
    valid: bool,
    supported: bool,
    fail: bool,
    fail_supported: bool,
    fail_verify: bool,
    kind: (u8, String, String),
    advance: Option<(Arc<std::sync::atomic::AtomicU64>, bool)>,
}
impl Default for Remote {
    fn default() -> Self {
        Self {
            calls: Arc::new(AtomicUsize::new(0)),
            requests: Arc::new(Mutex::new(vec![])),
            reply: serde_json::json!({"success":true,"network":"eip155:8453","transaction":"fixture-tx","payer":"PRIVATE-PAYER","KYC":"SECRET"}),
            valid: true,
            supported: true,
            fail: false,
            fail_supported: false,
            fail_verify: false,
            kind: (2, "exact".into(), "eip155:8453".into()),
            advance: None,
        }
    }
}
impl Facilitator for Remote {
    type Error = Error;
    async fn verify(&self, request: &proto::VerifyRequest) -> Result<proto::VerifyResponse> {
        tracing::info!(private_payer = "PRIVATE-PAYER", "fixture SDK response");
        if let Some((clock, false)) = &self.advance {
            clock.store(100, Ordering::SeqCst);
        }
        self.requests
            .lock()
            .unwrap()
            .push(serde_json::from_str(request.as_str()).unwrap());
        if self.fail_verify {
            return Err(Error::Rejected);
        }
        Ok(proto::VerifyResponse(
            serde_json::json!({"isValid":self.valid,"payer":"PRIVATE-PAYER"}),
        ))
    }
    async fn settle(&self, _: &proto::SettleRequest) -> Result<proto::SettleResponse> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if self.fail {
            Err(Error::Rejected)
        } else {
            Ok(proto::SettleResponse(self.reply.clone()))
        }
    }
    async fn supported(&self) -> Result<proto::SupportedResponse> {
        if let Some((clock, true)) = &self.advance {
            clock.store(100, Ordering::SeqCst);
        }
        if self.fail_supported {
            return Err(Error::Rejected);
        }
        Ok(proto::SupportedResponse {
            kinds: if self.supported {
                vec![proto::SupportedPaymentKind {
                    x402_version: self.kind.0,
                    scheme: self.kind.1.clone(),
                    network: self.kind.2.clone(),
                    extra: None,
                }]
            } else {
                vec![]
            },
            ..Default::default()
        })
    }
}
pub(super) fn terms() -> Value {
    serde_json::json!({"scheme":"exact","network":"eip155:8453","amount":"123456789012345678901234567890","asset":"USDC","payTo":"merchant","maxTimeoutSeconds":30,"extra":null})
}
pub(super) fn request(operation: &str) -> Instruction {
    let resource = serde_json::json!({"url":"https://merchant.example/resource"});
    Instruction { kind: Action::Execute, id:[1;32],parent:None,adapter:"crypto".into(),operation:operation.into(),binding:Binding{scope:"community".into(),subject:"member".into(),action:"resource".into()},expires_at:100,
        terms:Private::new(serde_json::to_vec(&serde_json::json!({"x402Version":2,"resource":resource,"accepts":[terms()]})).unwrap()),
        evidence:Private::new(serde_json::to_vec(&serde_json::json!({"x402Version":2,"accepted":terms(),"resource":resource,"payload":{"signature":"fixture-signature","authorization":{"from":"PRIVATE-PAYER","nonce":"fixture-nonce"}}})).unwrap()) }
}

#[tokio::test]
async fn settlement_uses_trusted_terms_and_reduces_every_provider_response() {
    let remote = Remote::default();
    let calls = remote.calls.clone();
    let requests = remote.requests.clone();
    let mut payments =
        Payments::new(MemoryJournal::default(), [9; 32], std::sync::Arc::new(|| 0)).unwrap();
    payments
        .register(Arc::new(
            configured("crypto".into(), remote, 16384).unwrap(),
        ))
        .unwrap();
    let request = request("x402.settle");
    let result = payments.execute(&request, 1).await.unwrap();
    assert_eq!(result.outcome.state(), State::Settled);
    let wire = serde_json::to_string(&result.outcome).unwrap();
    for private in [
        "PRIVATE-PAYER",
        "KYC",
        "SECRET",
        "fixture-tx",
        "fixture-signature",
    ] {
        assert!(!wire.contains(private));
    }
    let response = STANDARD
        .decode(result.member_action.unwrap().expose())
        .unwrap();
    let response: Value = serde_json::from_slice(&response).unwrap();
    assert_eq!(response["transaction"], "fixture-tx");
    assert!(response.get("payer").is_none());
    let proof = result
        .outcome
        .evidence(&request.binding, State::Settled, 1, 40)
        .unwrap();
    assert_eq!(
        proof.in_context(&request.binding, 39).unwrap(),
        ("crypto", 40)
    );
    payments.execute(&request, 2).await.unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(requests.lock().unwrap()[0]["paymentRequirements"], terms());
    // A second operation observing the same transaction has the same gate marker.
    let mut repeated = self::request("x402.settle");
    repeated.id = [2; 32];
    let result = payments.execute(&repeated, 1).await.unwrap();
    assert_eq!(
        result
            .outcome
            .evidence(&repeated.binding, State::Settled, 1, 40)
            .unwrap()
            .claim(),
        proof.claim()
    );
}

#[tokio::test]
async fn verification_is_not_collection_and_does_not_call_settlement() {
    let remote = Remote::default();
    let calls = remote.calls.clone();
    let adapter = configured("crypto".into(), remote, 16384).unwrap();
    let observation = adapter
        .execute(&request("x402.verify"), "key", None)
        .await
        .unwrap();
    assert_eq!(observation.state, State::Verified);
    assert_eq!(observation.valid_until, None);
    assert!(observation.claim.is_none());
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert!(observation.continuation.is_none());
}

#[tokio::test]
async fn client_cannot_change_amount_asset_network_recipient_or_resource() {
    let remote = Remote::default();
    let calls = remote.calls.clone();
    let requests = remote.requests.clone();
    let adapter = configured("crypto".into(), remote, 16384).unwrap();
    for field in ["amount", "asset", "network", "payTo", "scheme"] {
        let mut request = request("x402.settle");
        let mut evidence: Value = serde_json::from_slice(request.evidence.expose()).unwrap();
        evidence["accepted"][field] = Value::String(
            if field == "network" {
                "eip155:1"
            } else {
                "changed"
            }
            .into(),
        );
        request.evidence = Private::new(serde_json::to_vec(&evidence).unwrap());
        assert!(adapter.execute(&request, "key", None).await.is_err());
    }
    let mut request = request("x402.settle");
    let mut evidence: Value = serde_json::from_slice(request.evidence.expose()).unwrap();
    evidence["resource"]["url"] = Value::String("https://other.example".into());
    request.evidence = Private::new(serde_json::to_vec(&evidence).unwrap());
    assert!(adapter.execute(&request, "key", None).await.is_err());
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert!(requests.lock().unwrap().is_empty());
}

#[tokio::test]
async fn invalid_verification_and_unsupported_schemes_cannot_settle() {
    for remote in [
        Remote {
            valid: false,
            ..Default::default()
        },
        Remote {
            supported: false,
            ..Default::default()
        },
    ] {
        let calls = remote.calls.clone();
        let adapter = configured("crypto".into(), remote, 16384).unwrap();
        assert_eq!(
            adapter
                .execute(&request("x402.settle"), "key", None)
                .await
                .unwrap()
                .state,
            State::Failed
        );
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }
}

#[tokio::test]
async fn settlement_timeout_failure_and_wrong_network_never_prove_success() {
    for remote in [
        Remote {
            fail: true,
            ..Default::default()
        },
        Remote {
            reply: serde_json::json!({"success":false,"errorReason":"settlement_pending","transaction":"tx","network":"eip155:8453"}),
            ..Default::default()
        },
    ] {
        let adapter = configured("crypto".into(), remote, 16384).unwrap();
        assert_eq!(
            adapter
                .execute(&request("x402.settle"), "key", None)
                .await
                .unwrap()
                .state,
            State::Unknown
        );
    }
    for reply in [
        serde_json::json!({"success":true}),
        serde_json::json!({"success":true,"network":"eip155:1","transaction":"tx"}),
        serde_json::json!({"success":true,"network":"eip155:8453","transaction":""}),
    ] {
        let adapter = configured(
            "crypto".into(),
            Remote {
                reply,
                ..Default::default()
            },
            16384,
        )
        .unwrap();
        assert_eq!(
            adapter
                .execute(&request("x402.settle"), "key", None)
                .await
                .unwrap()
                .state,
            State::Unknown
        );
    }
}

struct Reader(bool);
#[async_trait]
impl ReceiptReader for Reader {
    async fn settled(&self, network: &str, transaction: &str) -> Result<ReceiptObservation> {
        assert_eq!(network, "eip155:8453");
        assert_eq!(transaction, "fixture-tx");
        Ok(ReceiptObservation {
            settled: self.0,
            upstream: Vec::new(),
            actual_cost: None,
        })
    }
}

#[tokio::test]
async fn reconciliation_is_read_only_and_requires_a_provider_observer() {
    let adapter = configured("crypto".into(), Remote::default(), 16384).unwrap();
    let observation = adapter
        .execute(&request("x402.settle"), "key", None)
        .await
        .unwrap();
    assert!(matches!(
        adapter
            .observe("x402.settle", observation.continuation.as_ref())
            .await,
        Err(Error::Unsupported)
    ));
    let adapter = adapter.with_reader(Arc::new(Reader(true)));
    assert_eq!(
        adapter
            .observe("x402.settle", observation.continuation.as_ref())
            .await
            .unwrap()
            .state,
        State::Settled
    );
    assert!(adapter.observe("x402.settle", None).await.is_err());
    assert!(
        adapter
            .observe("x402.settle", Some(&Private::new(vec![1])))
            .await
            .is_err()
    );
    let adapter = adapter.with_reader(Arc::new(Reader(false)));
    let result = adapter
        .observe("x402.settle", observation.continuation.as_ref())
        .await
        .unwrap();
    assert_eq!(result.state, State::Unknown);
    assert!(result.continuation.is_some());
}

#[tokio::test]
async fn malformed_inputs_limits_and_unknown_operations_fail_before_network() {
    for (id, limit) in [
        ("", 10),
        ("bad\n", 10),
        ("crypto", 0),
        (&"a".repeat(257), 10),
    ] {
        assert!(configured(id.into(), Remote::default(), limit).is_err());
    }
    let adapter = configured("crypto".into(), Remote::default(), 16384).unwrap();
    assert!(
        adapter
            .execute(&request("absent"), "key", None)
            .await
            .is_err()
    );
    assert!(
        adapter
            .execute(&request("x402.settle"), "key", Some(&Private::new(vec![])))
            .await
            .is_err()
    );
    let mut invalid = request("x402.settle");
    invalid.evidence = Private::new(b"{}".to_vec());
    assert!(adapter.execute(&invalid, "key", None).await.is_err());
    invalid = request("x402.settle");
    invalid.terms = Private::new(b"{}".to_vec());
    assert!(adapter.execute(&invalid, "key", None).await.is_err());
    let adapter = configured("crypto".into(), Remote::default(), 1).unwrap();
    assert!(
        adapter
            .execute(&request("x402.settle"), "key", None)
            .await
            .is_err()
    );
}

#[test]
fn http_headers_roundtrip_through_standard_v2_encoding() {
    let request = request("x402.settle");
    let mut offer: Offer = serde_json::from_slice(request.terms.expose()).unwrap();
    let header = payment_required(&offer).unwrap();
    let decoded: Offer = serde_json::from_slice(&STANDARD.decode(header).unwrap()).unwrap();
    assert_eq!(decoded.accepts, offer.accepts);
    let encoded = STANDARD.encode(request.evidence.expose());
    assert_eq!(
        payment_signature(&encoded, encoded.len()).unwrap().expose(),
        request.evidence.expose()
    );
    assert!(payment_signature(&encoded, 1).is_err());
    assert!(payment_signature("%%%", 10).is_err());
    offer.resource = None;
    assert!(payment_required(&offer).is_err());
    offer.accepts.clear();
    assert!(payment_required(&offer).is_err());
    assert_eq!(PAYMENT_REQUIRED, "PAYMENT-REQUIRED");
    assert_eq!(PAYMENT_SIGNATURE, "PAYMENT-SIGNATURE");
    assert_eq!(PAYMENT_RESPONSE, "PAYMENT-RESPONSE");
}

#[tokio::test]
async fn unavailable_or_mismatched_facilitators_cannot_settle() {
    for remote in [
        Remote {
            fail_supported: true,
            ..Default::default()
        },
        Remote {
            fail_verify: true,
            ..Default::default()
        },
        Remote {
            kind: (1, "exact".into(), "eip155:8453".into()),
            ..Default::default()
        },
        Remote {
            kind: (2, "other".into(), "eip155:8453".into()),
            ..Default::default()
        },
        Remote {
            kind: (2, "exact".into(), "eip155:1".into()),
            ..Default::default()
        },
    ] {
        let calls = remote.calls.clone();
        let adapter = configured("crypto".into(), remote, 16384).unwrap();
        assert!(matches!(
            adapter
                .execute(&request("x402.settle"), "key", None)
                .await
                .unwrap()
                .state,
            State::Unknown | State::Failed
        ));
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }
}

struct SchemeIdentity;
impl AuthorizationIdentity for SchemeIdentity {
    fn identify(&self, verified: &proto::VerifyRequest) -> Result<Authorization> {
        let wire: Value = serde_json::from_str(verified.as_str()).unwrap();
        let authorization = &wire["paymentPayload"]["payload"]["authorization"];
        Ok(Authorization {
            identity: Private::new(
                serde_json::to_vec(&serde_json::json!([
                    "fixture-signing-domain",
                    authorization["nonce"]
                ]))
                .unwrap(),
            ),
            valid_until: 10,
        })
    }
}
struct EmptyIdentity;
impl AuthorizationIdentity for EmptyIdentity {
    fn identify(&self, _: &proto::VerifyRequest) -> Result<Authorization> {
        Ok(Authorization {
            identity: Private::new(vec![]),
            valid_until: 10,
        })
    }
}

#[tokio::test]
async fn unsigned_metadata_cannot_multiply_verification_receipts_or_extend_authorization_expiry() {
    for canonical in [false, true] {
        let mut adapter = configured("crypto".into(), Remote::default(), 16384).unwrap();
        if canonical {
            adapter = adapter.with_authorization_identity(Arc::new(SchemeIdentity));
        }
        let mut payments =
            Payments::new(MemoryJournal::default(), [9; 32], std::sync::Arc::new(|| 0)).unwrap();
        payments.register(Arc::new(adapter)).unwrap();
        let mut original = None;
        for variant in 0..4 {
            let mut request = request("x402.verify");
            request.id = [variant + 1; 32];
            let mut payload: Value = serde_json::from_slice(request.evidence.expose()).unwrap();
            match variant {
                1 => {
                    payload.as_object_mut().unwrap().remove("resource");
                }
                2 => {
                    payload["resource"]["description"] = "changed unsigned description".into();
                }
                3 => {
                    payload["extensions"] = serde_json::json!({"unsigned":"changed"});
                }
                _ => {}
            }
            request.evidence = Private::new(serde_json::to_vec(&payload).unwrap());
            let outcome = payments.execute(&request, 1).await.unwrap().outcome;
            assert_eq!(outcome.state(), State::Verified);
            let evidence = outcome.evidence(&request.binding, State::Verified, 1, 100);
            if canonical {
                let evidence = evidence.unwrap();
                assert_eq!(evidence.in_context(&request.binding, 9).unwrap().1, 10);
                assert!(evidence.in_context(&request.binding, 10).is_err());
                if let Some(original) = original {
                    assert_eq!(original, *evidence.claim());
                } else {
                    original = Some(*evidence.claim());
                }
                assert!(
                    outcome
                        .evidence(&request.binding, State::Verified, 10, 100)
                        .is_err()
                );
            } else {
                assert!(matches!(evidence, Err(Error::Rejected)));
            }
        }
        if canonical {
            let mut foreign = request("x402.verify");
            foreign.id = [5; 32];
            foreign.binding.subject = "other-member".into();
            let conflict = payments.execute(&foreign, 1).await.unwrap();
            assert_eq!(conflict.commit_error, Some(Error::Conflict));
            assert_eq!(conflict.outcome.state(), State::Unknown);
        }
    }
    let adapter = configured("crypto".into(), Remote::default(), 16384)
        .unwrap()
        .with_authorization_identity(Arc::new(EmptyIdentity));
    assert_eq!(
        adapter
            .execute(&request("x402.verify"), "key", None)
            .await
            .unwrap()
            .state,
        State::Failed
    );
}

fn configured<F: Facilitator + Send + Sync>(
    id: String,
    facilitator: F,
    limit: usize,
) -> Result<X402<Sdk<F>>> {
    X402::new(id, Sdk(facilitator), limit, Mode::Production)
}
impl<F: Transport> X402<F> {
    async fn execute(
        &self,
        request: &Instruction,
        key: &str,
        previous: Option<&Private>,
    ) -> Result<Observation> {
        let auth = Dispatch::new(
            request,
            crate::test_support::permit(request, self.mode),
            0,
            std::sync::Arc::new(|| 0),
        )?;
        Adapter::execute(self, &auth, key, previous).await
    }
    async fn observe(&self, operation: &str, previous: Option<&Private>) -> Result<Observation> {
        let mut request = request(operation);
        request.kind = Action::Observe;
        let auth = Dispatch::new(
            &request,
            crate::test_support::permit(&request, self.mode),
            0,
            std::sync::Arc::new(|| 0),
        )?;
        Adapter::observe(self, &auth, previous).await
    }
}

#[tokio::test]
async fn complete_results_are_explicit_and_sandbox_cannot_grant_paid_access() {
    for mode in [Mode::Production, Mode::Sandbox] {
        let mut payments =
            Payments::new(MemoryJournal::default(), [9; 32], std::sync::Arc::new(|| 0)).unwrap();
        payments
            .register(Arc::new(
                X402::new("crypto".into(), Sdk(Remote::default()), 16384, mode).unwrap(),
            ))
            .unwrap();
        let request = request("x402.settle");
        let result = payments
            .dispatch_with_output(
                &request,
                crate::test_support::permit(&request, mode),
                1,
                crate::details::Disclosure::Complete,
            )
            .await
            .unwrap();
        assert_eq!(result.upstream.len(), 3);
        let full: Value = serde_json::from_slice(result.upstream[2].payload.expose()).unwrap();
        assert_eq!(full["KYC"], "SECRET");
        assert!(result.outcome.satisfies_terms());
        let access = payments.paid_access(
            &request,
            &result.outcome,
            crate::access::Request {
                level: "supporter".into(),
                period: crate::access::Period { start: 1, end: 50 },
                requires: State::Settled,
            },
            1,
        );
        assert_eq!(access.is_ok(), mode == Mode::Production);
        assert!(
            payments
                .dispatch_with_output(
                    &request,
                    crate::test_support::permit(&request, mode),
                    1,
                    crate::details::Disclosure::Complete
                )
                .await
                .unwrap()
                .upstream
                .is_empty()
        );
    }
    assert!(
        X402::new(
            "crypto".into(),
            Sdk(Remote::default()),
            16384,
            Mode::Simulation
        )
        .is_err()
    );
    let adapter = configured("crypto".into(), Remote::default(), 16384).unwrap();
    let request = request("x402.settle");
    let wrong = Dispatch::new(
        &request,
        crate::test_support::permit(&request, Mode::Sandbox),
        1,
        std::sync::Arc::new(|| 0),
    )
    .unwrap();
    assert!(
        Adapter::execute(&adapter, &wrong, "key", None)
            .await
            .is_err()
    );
    let mut read = self::request("x402.settle");
    read.kind = Action::Observe;
    let wrong = Dispatch::new(
        &read,
        crate::test_support::permit(&read, Mode::Production),
        1,
        std::sync::Arc::new(|| 0),
    )
    .unwrap();
    assert!(
        Adapter::execute(&adapter, &wrong, "key", None)
            .await
            .is_err()
    );
    let wrong = Dispatch::new(
        &request,
        crate::test_support::permit(&request, Mode::Production),
        1,
        std::sync::Arc::new(|| 0),
    )
    .unwrap();
    assert!(Adapter::observe(&adapter, &wrong, None).await.is_err());
}

#[tokio::test]
async fn delayed_facilitator_phases_cannot_use_an_expired_quote_for_the_next_call() {
    for before_verify in [true, false] {
        let now = Arc::new(std::sync::atomic::AtomicU64::new(1));
        let clock = now.clone();
        let remote = Remote {
            advance: Some((now, before_verify)),
            ..Default::default()
        };
        let calls = remote.calls.clone();
        let seen = remote.requests.clone();
        let mut payments = Payments::new(
            MemoryJournal::default(),
            [9; 32],
            Arc::new(move || clock.load(Ordering::SeqCst)),
        )
        .unwrap();
        payments
            .register(Arc::new(
                configured("crypto".into(), remote, 16384).unwrap(),
            ))
            .unwrap();
        let request = request("x402.settle");
        let result = payments
            .dispatch_with_output(
                &request,
                crate::test_support::permit(&request, Mode::Production),
                1,
                crate::details::Disclosure::Complete,
            )
            .await
            .unwrap();
        assert_eq!(result.outcome.state(), State::Unknown);
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        assert_eq!(seen.lock().unwrap().len(), usize::from(!before_verify));
        assert_eq!(result.upstream.len(), if before_verify { 1 } else { 2 });
    }
}
struct TraceCounter(Arc<AtomicUsize>);
impl tracing::Subscriber for TraceCounter {
    fn enabled(&self, _: &tracing::Metadata<'_>) -> bool {
        true
    }
    fn new_span(&self, _: &tracing::span::Attributes<'_>) -> tracing::span::Id {
        self.0.fetch_add(1, Ordering::SeqCst);
        tracing::span::Id::from_u64(1)
    }
    fn record(&self, _: &tracing::span::Id, _: &tracing::span::Record<'_>) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
    fn record_follows_from(&self, _: &tracing::span::Id, _: &tracing::span::Id) {}
    fn event(&self, _: &tracing::Event<'_>) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
    fn enter(&self, _: &tracing::span::Id) {}
    fn exit(&self, _: &tracing::span::Id) {}
}
#[tokio::test]
async fn configured_sdk_traces_stay_inside_the_private_payment_boundary() {
    let count = Arc::new(AtomicUsize::new(0));
    let seen = count.clone();
    async move {
        tracing::info!("outside boundary");
        let adapter = configured("crypto".into(), Remote::default(), 16384).unwrap();
        adapter
            .execute(&request("x402.settle"), "key", None)
            .await
            .unwrap();
        assert_eq!(seen.load(Ordering::SeqCst), 1);
    }
    .with_subscriber(TraceCounter(count))
    .await;
}
