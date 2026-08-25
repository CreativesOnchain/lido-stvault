pub mod client;
pub mod predeploy;
mod types;
pub mod verifier;

pub use client::ElClient;
pub use predeploy::ConsolidationPredeploy;
pub use types::{ElVerificationEvidence, ElVerifiedTx};
pub use verifier::verify_execution_layer;
