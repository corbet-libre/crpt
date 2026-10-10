use super::*;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};

async fn server(replies: Vec<String>) -> (Url, tokio::task::JoinHandle<Vec<String>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = Url::parse(&format!("http://{}/api/", listener.local_addr().unwrap())).unwrap();
    let task = tokio::spawn(async move {
        let mut requests = Vec::new();
        for reply in replies {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut bytes = Vec::new();
            let mut buffer = [0; 4096];
            loop {
                let n = stream.read(&mut buffer).await.unwrap();
                if n == 0 {
                    break;
                }
                bytes.extend_from_slice(&buffer[..n]);
                if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&bytes[..end]).to_ascii_lowercase();
                    let length = headers
                        .lines()
                        .find_map(|l| l.strip_prefix("content-length: "))
                        .map_or(0, |n| n.parse::<usize>().unwrap());
                    if bytes.len() >= end + 4 + length {
                        break;
                    }
                }
            }
            requests.push(String::from_utf8(bytes).unwrap());
            stream.write_all(reply.as_bytes()).await.unwrap();
        }
        requests
    });
    (url, task)
}
fn response(body: &str) -> String {
    format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    )
}

#[tokio::test]
async fn upstream_interface_uses_exact_configured_endpoint_and_authentication() {
    let (url, calls) = server(vec![
        response(r#"{"kinds":[],"extensions":[],"signers":{}}"#),
        response(r#"{"isValid":true,"payer":"secret"}"#),
        response(r#"{"success":true,"network":"eip155:8453","transaction":"tx"}"#),
    ])
    .await;
    let mut headers = HeaderMap::new();
    headers.insert("authorization", "Bearer fixture-token".parse().unwrap());
    let client = HttpFacilitator::build(url, headers, Duration::from_secs(2), 4096, true).unwrap();
    assert!(client.supported().await.unwrap().kinds.is_empty());
    let request: VerifyRequest =
        serde_json::from_str(r#"{"x402Version":2,"paymentPayload":{},"paymentRequirements":{}}"#)
            .unwrap();
    assert_eq!(client.verify(&request).await.unwrap().0["isValid"], true);
    assert_eq!(
        client.settle(&request).await.unwrap().0["transaction"],
        "tx"
    );
    let calls = calls.await.unwrap();
    assert!(calls[0].starts_with("GET /api/supported "));
    assert!(calls[1].starts_with("POST /api/verify "));
    assert!(calls[2].starts_with("POST /api/settle "));
    assert!(
        calls
            .iter()
            .all(|call| call.contains("Bearer fixture-token"))
    );
    assert!(calls[1].contains(request.as_str()));
}

#[tokio::test]
async fn redirects_errors_oversize_and_malformed_bodies_are_uniform_rejections() {
    for reply in ["HTTP/1.1 307 Temporary Redirect\r\nLocation: https://other.example\r\nContent-Length: 0\r\n\r\n".into(),response("secret-invalid-json"),response(&"x".repeat(100)),"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n64\r\n".to_owned()+&"x".repeat(100)+"\r\n0\r\n\r\n",String::new(),"HTTP/1.1 200 OK\r\nContent-Length: 20\r\n\r\nshort".into()] {
        let (url,calls)=server(vec![reply]).await;
        let client=HttpFacilitator::build(url,HeaderMap::new(),Duration::from_secs(2),64,true).unwrap();
        let error=client.supported().await.unwrap_err();assert_eq!(error,Error::Rejected);assert!(!error.to_string().contains("secret"));
        assert_eq!(calls.await.unwrap().len(),1);
    }
}

#[test]
fn production_transport_rejects_untrusted_url_shapes_and_implicit_limits() {
    for url in [
        "http://example.test",
        "https://user:pass@example.test",
        "https://:pass@example.test",
        "https://example.test?key=value",
        "https://example.test#fragment",
        "file:///tmp/example",
    ] {
        assert!(
            HttpFacilitator::new(
                Url::parse(url).unwrap(),
                HeaderMap::new(),
                Duration::from_secs(1),
                1024
            )
            .is_err()
        );
    }
    assert!(
        HttpFacilitator::new(
            Url::parse("https://example.test").unwrap(),
            HeaderMap::new(),
            Duration::ZERO,
            1024
        )
        .is_err()
    );
    assert!(
        HttpFacilitator::new(
            Url::parse("https://example.test").unwrap(),
            HeaderMap::new(),
            Duration::from_secs(1),
            0
        )
        .is_err()
    );
    assert!(
        HttpFacilitator::new(
            Url::parse("https://example.test").unwrap(),
            HeaderMap::new(),
            Duration::from_secs(1),
            1024
        )
        .is_ok()
    );
}

#[test]
fn local_fixture_transport_is_restricted_to_loopback_http() {
    for url in ["http://localhost", "file:///tmp/example"] {
        assert!(
            HttpFacilitator::build(
                Url::parse(url).unwrap(),
                HeaderMap::new(),
                Duration::from_secs(1),
                1024,
                true
            )
            .is_err()
        );
    }
}

#[tokio::test]
async fn detailed_transport_keeps_extensions_and_failed_bodies_private_per_call() {
    let (url,calls)=server(vec![
        response(r#"{"kinds":[],"extensions":[],"signers":{},"unknown_extension":{"secret":"retained only on request"}}"#),
        response(r#"{"isValid":true,"unknown_verify":"full"}"#),
        response(r#"{"success":true,"unknown_settle":"full"}"#),
        response("private-invalid-json"),
        "HTTP/1.1 403 Forbidden\r\nContent-Length: 13\r\nConnection: close\r\n\r\nprivate-error".into(),
    ]).await;
    let client =
        HttpFacilitator::build(url, HeaderMap::new(), Duration::from_secs(2), 4096, true).unwrap();
    let supported = client.supported_details().await;
    assert!(supported.result.is_ok());
    assert!(
        String::from_utf8_lossy(supported.upstream[0].payload.expose())
            .contains("unknown_extension")
    );
    let request: VerifyRequest =
        serde_json::from_str(r#"{"x402Version":2,"paymentPayload":{},"paymentRequirements":{}}"#)
            .unwrap();
    assert_eq!(
        client.verify_details(&request).await.result.unwrap().0["unknown_verify"],
        "full"
    );
    assert_eq!(
        client.settle_details(&request).await.result.unwrap().0["unknown_settle"],
        "full"
    );
    for expected in [b"private-invalid-json".as_slice(), b"private-error"] {
        let exchange = client.supported_details().await;
        assert_eq!(exchange.result.unwrap_err(), Error::Rejected);
        assert_eq!(exchange.upstream[0].payload.expose(), expected);
        assert!(!format!("{:?}", exchange.upstream[0]).contains("private"));
    }
    assert_eq!(calls.await.unwrap().len(), 5);
}
