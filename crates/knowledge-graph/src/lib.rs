//! Versioned, evidence-backed local research graph. No engine or game dependency.
pub mod bundle;
pub mod error;
pub mod export;
pub mod extract;
pub mod ids;
pub mod material;
#[cfg(feature = "mcp")]
pub mod mcp;
pub mod model;
pub mod observations;
pub mod proposals;
pub mod query;
pub mod source;
pub mod store;
