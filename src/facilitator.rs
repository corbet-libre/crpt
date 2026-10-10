//! Response-bearing seam around maintained x402 facilitators and HTTP transports.
use crate::{
    details::ProviderReply,
    payments::{Error, Private, Result},
};
use std::future::Future;
use x402_types::{facilitator::Facilitator, proto::*};

/// Borrowed authorization for one adapter phase. It cannot be detached from the
/// live dispatch. Recheck after any queue/await and immediately before I/O.
/// It validates the adapter's actual account/mode, bound quote and fresh clock.
pub struct DispatchGuard<'a> {
    check: &'a (dyn Fn() -> Result<()> + Send + Sync),
}
impl<'a> DispatchGuard<'a> {
    pub(crate) fn new(check: &'a (dyn Fn() -> Result<()> + Send + Sync)) -> Self {
        Self { check }
    }
    /// Refuse stale or mismatched authorization before a potentially billed call.
    pub fn check(&self) -> Result<()> {
        (self.check)()
    }
}

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
/// Every implementation must check the supplied guard at its actual I/O edge,
/// after queues or other waits. No detached work, hidden retries or extra calls.
pub trait Transport: Send + Sync {
    /// Discover supported protocol capabilities; its charge must be in the quote.
    fn supported_details(
        &self,
        authorization: &DispatchGuard<'_>,
    ) -> impl Future<Output = Exchange<SupportedResponse>> + Send;
    /// Verify authorization without settlement; include this phase in the quote.
    fn verify_details(
        &self,
        request: &VerifyRequest,
        authorization: &DispatchGuard<'_>,
    ) -> impl Future<Output = Exchange<VerifyResponse>> + Send;
    /// Settle once. An error never establishes that no payment occurred.
    fn settle_details(
        &self,
        request: &SettleRequest,
        authorization: &DispatchGuard<'_>,
    ) -> impl Future<Output = Exchange<SettleResponse>> + Send;
}

/// Reuse any maintained upstream facilitator implementation. Complete available
/// typed responses are returned; fields already discarded by that SDK cannot be
/// recovered. SDK transport/config errors stay redacted to protect credentials.
/// The host must configure the SDK with no internal queue, retries, detached work
/// or additional calls: its upstream trait cannot receive a deadline. A queued
/// SDK must instead implement Transport and use its guard at the actual I/O edge.
pub struct Sdk<F>(pub F);
impl<F: Facilitator + Send + Sync> Transport for Sdk<F> {
    async fn supported_details(
        &self,
        authorization: &DispatchGuard<'_>,
    ) -> Exchange<SupportedResponse> {
        let result = async {
            authorization.check()?;
            self.0.supported().await.map_err(|_| Error::Rejected)
        }
        .await;
        Exchange::typed("supported", result)
    }
    async fn verify_details(
        &self,
        request: &VerifyRequest,
        authorization: &DispatchGuard<'_>,
    ) -> Exchange<VerifyResponse> {
        let result = async {
            authorization.check()?;
            self.0.verify(request).await.map_err(|_| Error::Rejected)
        }
        .await;
        Exchange::typed("verify", result)
    }
    async fn settle_details(
        &self,
        request: &SettleRequest,
        authorization: &DispatchGuard<'_>,
    ) -> Exchange<SettleResponse> {
        let result = async {
            authorization.check()?;
            self.0.settle(request).await.map_err(|_| Error::Rejected)
        }
        .await;
        Exchange::typed("settle", result)
    }
}
