pub mod client;
pub mod predeploy;
pub mod types;
pub mod verifier;

pub use client::ElClient;
pub use predeploy::ConsolidationPredeploy;
pub use types::{BlockDetails, ElVerificationEvidence, ElVerifiedTx, TxDetails, TxLog, TxReceipt};
pub use verifier::verify_execution_layer;
