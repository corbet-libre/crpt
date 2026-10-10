//! Response-bearing seam around maintained x402 facilitators and HTTP transports.
use crate::{
    details::ProviderReply,
    payments::{Error, Private, Result},
};
use std::future::Future;
use x402_types::{facilitator::Facilitator, proto::*};

/// A transient upstream exchange. Diagnostic data cannot construct a proof.
pub struct Exchange<T> {
    /// Parsed upstream protocol response, or a uniform failure.
    pub result: Result<T>,
    /// Complete available provider data for explicit private disclosure.
    pub upstream: Vec<ProviderReply>,
}
impl<T: serde::Serialize> Exchange<T> {
    fn typed(phase: &'static str, result: Result<T>) -> Self {
        let upstream = result
            .as_ref()
            .ok()
            .map(|value| ProviderReply {
                phase,
                headers: None,
                http_status: None,
                payload: Private::new(
                    serde_json::to_vec(value).expect("upstream response serializes"),
                ),
            })
            .into_iter()
            .collect();
        Self { result, upstream }
    }
}

/// Transport supplies typed maintained protocol values and explicit private data.
/// Implementations are trusted configuration, never selected by member input.
pub trait Transport: Send + Sync {
    /// Discover supported protocol capabilities; its charge must be in the quote.
    fn supported_details(&self) -> impl Future<Output = Exchange<SupportedResponse>> + Send;
    /// Verify authorization without settlement; include this phase in the quote.
    fn verify_details(
        &self,
        request: &VerifyRequest,
    ) -> impl Future<Output = Exchange<VerifyResponse>> + Send;
    /// Settle once. An error never establishes that no payment occurred.
    fn settle_details(
        &self,
        request: &SettleRequest,
    ) -> impl Future<Output = Exchange<SettleResponse>> + Send;
}

/// Reuse any maintained upstream facilitator implementation. Complete available
/// typed responses are returned; fields already discarded by that SDK cannot be
/// recovered. SDK transport/config errors stay redacted to protect credentials.
pub struct Sdk<F>(pub F);
impl<F: Facilitator + Send + Sync> Transport for Sdk<F> {
    async fn supported_details(&self) -> Exchange<SupportedResponse> {
        Exchange::typed(
            "supported",
            self.0.supported().await.map_err(|_| Error::Rejected),
        )
    }
    async fn verify_details(&self, request: &VerifyRequest) -> Exchange<VerifyResponse> {
        Exchange::typed(
            "verify",
            self.0.verify(request).await.map_err(|_| Error::Rejected),
        )
    }
    async fn settle_details(&self, request: &SettleRequest) -> Exchange<SettleResponse> {
        Exchange::typed(
            "settle",
            self.0.settle(request).await.map_err(|_| Error::Rejected),
        )
    }
}
