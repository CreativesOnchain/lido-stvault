//! # `stvault-receipt`
//!
//! High-assurance, read-only verification library and CLI for tracing Lido stVault validator
//! consolidation requests across Ethereum's Execution Layer (EL) and Consensus Layer (CL).
//!
//! ## Core Verification Flow
//!
//! 1. **Parse Manifest**: [`parse_manifest_file`] / [`parse_manifest_str`] parses JSON or YAML Lido stVault manifests.
//! 2. **Execution Layer Verification**: [`el::verify_execution_layer`] traces transactions, validates predeploy interaction (`0x0000BBdDc7CE488642fb579F8B00f3a590007251`), and caches block headers.
//! 3. **Consensus Layer Verification**: [`cl::verify_consensus_layer`] queries validator indices, matches Beacon block bodies, and verifies the `pending_consolidations` queue.
//! 4. **Lido Role Inspection**: [`lido::LidoRoleInspector`] checks whether `NODE_OPERATOR_FEE_EXEMPT_ROLE` remains active.
//! 5. **Receipt Generation**: [`generate_and_save_receipts`] outputs Markdown reports, JSON receipts, and CSV tables.

pub mod cl;
pub mod cli;
pub mod el;
pub mod engine;
pub mod error;
pub mod lido;
pub mod manifest;
pub mod models;
pub mod receipt;
pub mod terminal;

pub use cl::BeaconClient;
pub use cli::{CliArgs, OutputFormat};
pub use el::ElClient;
pub use engine::VerificationEngine;
pub use error::{AppError, Result};
pub use manifest::{parse_manifest_file, parse_manifest_str};
pub use models::{
    ConsolidationPair, ConsolidationStatus, PairVerificationResult, VerificationReceipt,
    VerificationSummary,
};
pub use receipt::{
    EvidenceWriter, ReceiptArtifacts, generate_and_save_receipts, generate_csv_receipt,
    generate_json_receipt, generate_markdown_receipt,
};
