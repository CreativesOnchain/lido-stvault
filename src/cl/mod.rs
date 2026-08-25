pub mod client;
pub mod types;
pub mod verifier;

pub use client::BeaconClient;
pub use types::{ClVerificationEvidence, ClVerifiedPairEvidence};
pub use verifier::verify_consensus_layer;
