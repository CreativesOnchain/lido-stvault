pub mod cl;
pub mod cli;
pub mod el;
pub mod engine;
pub mod error;
pub mod lido;
pub mod manifest;
pub mod models;
pub mod receipt;

pub use cl::BeaconClient;
pub use cli::{CliArgs, OutputFormat};
pub use el::ElClient;
pub use engine::VerificationEngine;
pub use error::{AppError, Result};
pub use manifest::{parse_manifest_file, parse_manifest_str};
pub use models::{
    ConsolidationPair, ConsolidationStatus, PairVerificationResult, VerificationReceipt,
};
pub use receipt::{
    generate_csv_receipt, generate_json_receipt, generate_markdown_receipt, EvidenceWriter,
};
