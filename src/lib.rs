//! Crypto payment adapters sharing cpmt's execution and information boundary.
#![forbid(unsafe_code)]

pub use cpmt::payments;
/// Hosted BTCPay/CoinGate collection through the shared payment adapter boundary.
#[cfg(feature = "hosted")]
pub use cpmt::providers as hosted;
pub use cpmt::{access, cost, details, run, simulation};
pub mod facilitator;
#[cfg(feature = "http")]
pub mod http;
#[cfg(all(test, not(target_arch = "wasm32")))]
#[path = "tests/support.rs"]
mod test_support;
pub mod x402;
