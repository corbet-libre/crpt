//! Crypto payment adapters sharing cpmt's execution and information boundary.
#![forbid(unsafe_code)]

pub use cpmt::payments;
/// Hosted BTCPay/CoinGate collection through the shared payment adapter boundary.
#[cfg(feature = "hosted")]
pub use cpmt::providers as hosted;
#[cfg(feature = "http")]
pub mod http;
pub mod x402;
