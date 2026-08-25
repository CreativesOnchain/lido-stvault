pub mod client;
pub mod types;
pub mod verifier;

pub use client::BeaconClient;
pub use verifier::{verify_consensus_layer, ClVerificationEvidence, ClVerifiedPairEvidence};
