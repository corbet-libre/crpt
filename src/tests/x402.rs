use super::*;
use std::sync::{
    Mutex,
    atomic::{AtomicUsize, Ordering},
};

#[derive(Clone)]
struct Remote {
    calls: Arc<AtomicUsize>,
    requests: Arc<Mutex<Vec<Value>>>,
    reply: Value,
    valid: bool,
    supported: bool,
    fail: bool,
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
        }
    }
}
impl Facilitator for Remote {
    type Error = Error;
    async fn verify(&self, request: &proto::VerifyRequest) -> Result<proto::VerifyResponse> {
        self.requests
            .lock()
            .unwrap()
            .push(serde_json::from_str(request.as_str()).unwrap());
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
        Ok(proto::SupportedResponse {
            kinds: if self.supported {
                vec![proto::SupportedPaymentKind {
                    x402_version: 2,
                    scheme: "exact".into(),
                    network: "eip155:8453".into(),
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
    Instruction { id:[1;32],parent:None,adapter:"crypto".into(),operation:operation.into(),binding:Binding{scope:"community".into(),subject:"member".into(),action:"resource".into()},expires_at:100,
        terms:Private::new(serde_json::to_vec(&serde_json::json!({"x402Version":2,"resource":resource,"accepts":[terms()]})).unwrap()),
        evidence:Private::new(serde_json::to_vec(&serde_json::json!({"x402Version":2,"accepted":terms(),"resource":resource,"payload":{"signature":"fixture-signature","authorization":{"from":"PRIVATE-PAYER","nonce":"fixture-nonce"}}})).unwrap()) }
}

#[tokio::test]
async fn settlement_uses_trusted_terms_and_reduces_every_provider_response() {
    let remote = Remote::default();
    let calls = remote.calls.clone();
    let requests = remote.requests.clone();
    let mut payments = Payments::new(MemoryJournal::default(), [9; 32]).unwrap();
    payments
        .register(Arc::new(X402::new("crypto".into(), remote, 16384).unwrap()))
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
    let adapter = X402::new("crypto".into(), remote, 16384).unwrap();
    let observation = adapter
        .execute(&request("x402.verify"), "key", None)
        .await
        .unwrap();
    assert_eq!(observation.state, State::Verified);
    assert_eq!(observation.valid_until, Some(100));
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert!(observation.continuation.is_none());
}

#[tokio::test]
async fn client_cannot_change_amount_asset_network_recipient_or_resource() {
    let remote = Remote::default();
    let calls = remote.calls.clone();
    let requests = remote.requests.clone();
    let adapter = X402::new("crypto".into(), remote, 16384).unwrap();
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
        let adapter = X402::new("crypto".into(), remote, 16384).unwrap();
        assert!(
            adapter
                .execute(&request("x402.settle"), "key", None)
                .await
                .is_err()
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
        let adapter = X402::new("crypto".into(), remote, 16384).unwrap();
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
        let adapter = X402::new(
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
    async fn settled(&self, network: &str, transaction: &str) -> Result<bool> {
        assert_eq!(network, "eip155:8453");
        assert_eq!(transaction, "fixture-tx");
        Ok(self.0)
    }
}

#[tokio::test]
async fn reconciliation_is_read_only_and_requires_a_provider_observer() {
    let adapter = X402::new("crypto".into(), Remote::default(), 16384).unwrap();
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
        assert!(X402::new(id.into(), Remote::default(), limit).is_err());
    }
    let adapter = X402::new("crypto".into(), Remote::default(), 16384).unwrap();
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
    let adapter = X402::new("crypto".into(), Remote::default(), 1).unwrap();
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
    offer.accepts.clear();
    assert!(payment_required(&offer).is_err());
    assert_eq!(PAYMENT_REQUIRED, "PAYMENT-REQUIRED");
    assert_eq!(PAYMENT_SIGNATURE, "PAYMENT-SIGNATURE");
    assert_eq!(PAYMENT_RESPONSE, "PAYMENT-RESPONSE");
}
