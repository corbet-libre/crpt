//! Bounded HTTP transport implementing the upstream x402 facilitator interface.
//! No redirects, automatic retries, response logging or implicit credentials.
use crate::payments::{Error, Result};
use reqwest::{Client, Method, header::HeaderMap};
use serde::{Serialize, de::DeserializeOwned};
use std::time::Duration;
use url::Url;
use x402_types::{facilitator::Facilitator, proto::*};
use zeroize::Zeroizing;

/// Trusted HTTPS facilitator. Chain logic and signature verification stay remote.
/// Use provider-specific upstream implementations of `Facilitator` when dynamic
/// authentication or different transports are needed; `X402` accepts either.
pub struct HttpFacilitator {
    base: Url,
    client: Client,
    max_response_bytes: usize,
}

impl HttpFacilitator {
    /// Configure an explicit endpoint, authentication headers, timeout and bound.
    /// The endpoint and headers must never come from member evidence.
    pub fn new(
        base: Url,
        headers: HeaderMap,
        timeout: Duration,
        max_response_bytes: usize,
    ) -> Result<Self> {
        Self::build(base, headers, timeout, max_response_bytes, false)
    }
    fn build(
        mut base: Url,
        headers: HeaderMap,
        timeout: Duration,
        max_response_bytes: usize,
        local_fixture: bool,
    ) -> Result<Self> {
        let scheme_ok = base.scheme() == "https"
            || (local_fixture && base.scheme() == "http" && base.host_str() == Some("127.0.0.1"));
        if !scheme_ok
            || !base.username().is_empty()
            || base.password().is_some()
            || base.query().is_some()
            || base.fragment().is_some()
            || timeout.is_zero()
            || max_response_bytes == 0
        {
            return Err(Error::Invalid);
        }
        base.set_path(&format!("{}/", base.path().trim_end_matches('/')));
        let client = Client::builder()
            .default_headers(headers)
            .timeout(timeout)
            .https_only(!local_fixture)
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .no_proxy()
            .build()
            .map_err(transport_error)?;
        Ok(Self {
            base,
            client,
            max_response_bytes,
        })
    }
    async fn request<T: Serialize + Sync, R: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        body: Option<&T>,
    ) -> Result<R> {
        let url = self.base.join(path).map_err(|_| Error::Invalid)?;
        let mut request = self.client.request(method, url);
        if let Some(body) = body {
            request = request.json(body);
        }
        let mut response = request.send().await.map_err(transport_error)?;
        if !response.status().is_success()
            || response
                .content_length()
                .is_some_and(|length| length > self.max_response_bytes as u64)
        {
            return Err(Error::Rejected);
        }
        let mut bytes = Zeroizing::new(Vec::new());
        while let Some(chunk) = response.chunk().await.map_err(transport_error)? {
            if bytes.len().saturating_add(chunk.len()) > self.max_response_bytes {
                return Err(Error::Rejected);
            }
            bytes.extend_from_slice(&chunk);
        }
        serde_json::from_slice(&bytes).map_err(|_| Error::Rejected)
    }
}

fn transport_error(_: reqwest::Error) -> Error {
    Error::Rejected
}

impl Facilitator for HttpFacilitator {
    type Error = Error;
    async fn verify(&self, request: &VerifyRequest) -> Result<VerifyResponse> {
        self.request(Method::POST, "verify", Some(request)).await
    }
    async fn settle(&self, request: &SettleRequest) -> Result<SettleResponse> {
        self.request(Method::POST, "settle", Some(request)).await
    }
    async fn supported(&self) -> Result<SupportedResponse> {
        self.request::<(), _>(Method::GET, "supported", None).await
    }
}

#[cfg(test)]
#[path = "tests/http.rs"]
mod tests;
