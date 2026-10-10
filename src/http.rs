//! Bounded HTTP transport implementing the upstream x402 facilitator interface.
//! No redirects, automatic retries, response logging or implicit credentials.
use crate::payments::{Error, Private, Result};
use crate::{
    details::ProviderReply,
    facilitator::{DispatchGuard, Exchange, Transport},
};
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
        mut headers: HeaderMap,
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
        for value in headers.values_mut() {
            value.set_sensitive(true);
        }
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
    async fn send<T: Serialize + Sync>(
        &self,
        method: Method,
        path: &'static str,
        body: Option<&T>,
        authorization: Option<&DispatchGuard<'_>>,
    ) -> Result<ProviderReply> {
        let url = self.base.join(path).map_err(|_| Error::Invalid)?;
        let mut request = self.client.request(method, url);
        if let Some(body) = body {
            request = request.json(body);
        }
        if let Some(authorization) = authorization {
            authorization.check()?;
        }
        let mut response = request.send().await.map_err(transport_error)?;
        if response
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
        Ok(ProviderReply {
            phase: path,
            headers: Some(Private::new(
                serde_json::to_vec(
                    &response
                        .headers()
                        .iter()
                        .map(|(name, value)| (name.as_str(), value.as_bytes()))
                        .collect::<Vec<_>>(),
                )
                .expect("header fields serialize"),
            )),
            http_status: Some(response.status().as_u16()),
            payload: Private::new(std::mem::take(&mut *bytes)),
        })
    }
    async fn exchange<T: Serialize + Sync, R: DeserializeOwned>(
        &self,
        method: Method,
        path: &'static str,
        body: Option<&T>,
        authorization: Option<&DispatchGuard<'_>>,
    ) -> Exchange<R> {
        match self.send(method, path, body, authorization).await {
            Ok(reply) => {
                let result = if reply.http_status.is_some_and(|s| (200..300).contains(&s)) {
                    serde_json::from_slice(reply.payload.expose()).map_err(|_| Error::Rejected)
                } else {
                    Err(Error::Rejected)
                };
                Exchange {
                    result,
                    upstream: vec![reply],
                }
            }
            Err(error) => Exchange {
                result: Err(error),
                upstream: Vec::new(),
            },
        }
    }
}

fn transport_error(_: reqwest::Error) -> Error {
    Error::Rejected
}

impl Facilitator for HttpFacilitator {
    type Error = Error;
    async fn verify(&self, request: &VerifyRequest) -> Result<VerifyResponse> {
        self.exchange(Method::POST, "verify", Some(request), None)
            .await
            .result
    }
    async fn settle(&self, request: &SettleRequest) -> Result<SettleResponse> {
        self.exchange(Method::POST, "settle", Some(request), None)
            .await
            .result
    }
    async fn supported(&self) -> Result<SupportedResponse> {
        self.exchange::<(), _>(Method::GET, "supported", None, None)
            .await
            .result
    }
}

#[cfg(test)]
#[path = "tests/http.rs"]
mod tests;

impl Transport for HttpFacilitator {
    async fn supported_details(
        &self,
        authorization: &DispatchGuard<'_>,
    ) -> Exchange<SupportedResponse> {
        self.exchange::<(), _>(Method::GET, "supported", None, Some(authorization))
            .await
    }
    async fn verify_details(
        &self,
        request: &VerifyRequest,
        authorization: &DispatchGuard<'_>,
    ) -> Exchange<VerifyResponse> {
        self.exchange(Method::POST, "verify", Some(request), Some(authorization))
            .await
    }
    async fn settle_details(
        &self,
        request: &SettleRequest,
        authorization: &DispatchGuard<'_>,
    ) -> Exchange<SettleResponse> {
        self.exchange(Method::POST, "settle", Some(request), Some(authorization))
            .await
    }
}
