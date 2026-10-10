//! Crypto payment adapters sharing cpmt's execution and information boundary.
#![forbid(unsafe_code)]

pub use cpmt::payments;
#[cfg(feature = "http")]
pub mod http;
pub mod x402;
