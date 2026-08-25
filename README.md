# Lido stVault Consolidation Request Receipt CLI (`stvault-receipt`)

[![Crates.io](https://img.shields.io/crates/v/stvault-receipt.svg)](https://crates.io/crates/stvault-receipt)
[![Rust](https://img.shields.io/badge/rust-1.80%2B-orange.svg)](https://www.rust-lang.org)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE-MIT)
[![CI](https://github.com/CreativesOnchain/lido-stvault/actions/workflows/ci.yml/badge.svg)](https://github.com/CreativesOnchain/lido-stvault/actions/workflows/ci.yml)

A high-assurance, read-only CLI tool and Rust library that verifies the post-submission status of Lido stVault validator consolidation requests across Ethereum's **Execution Layer (EL)** and **Consensus Layer (CL)** without relying on proprietary indexers, private endpoints, or commercial APIs.

---

## The Problem

Under Ethereum's **EIP-7251 (MaxEB)** and **EIP-7002** consolidation mechanisms, validator consolidation requests are initiated on the Execution Layer via the consolidation predeploy contract (`0x0000BBdDc7CE488642fb579F8B00f3a590007251`).

1. **The Cross-Layer Disconnect:** An EL transaction can succeed (`status == 1`) and consume gas, while the Consensus Layer may silently reject or drop the consolidation request (e.g. invalid withdrawal credentials, queue full, inactive validator).
2. **Partial Batch Failures:** In multi-validator consolidation batches, some pairs may succeed while others fail silently or remain queued.
3. **Hardware Decommissioning Danger:** If node operators assume EL transaction success equals consolidation completion, prematurely shutting down source validator keys can lead to offline penalties or slashing.
4. **Dangling Fee-Exemption Permissions:** The temporary `NODE_OPERATOR_FEE_EXEMPT_ROLE` in Lido contracts may remain unrevoked after batch consolidation workflows, creating accounting and governance risks.

---

## The Solution

`stvault-receipt` traces every `source -> target` validator pair from the official Lido stVault consolidation manifest across both layers:

```
[Manifest & EL Tx Hashes]
        │
        ├──► 1. Execution Layer (EL RPC)
        │      • Confirms tx receipt status == 1
        │      • Extracts EIP-7251 Predeploy calls and event evidence
        │      • Maps EL block number & timestamp
        │
        ├──► 2. Consensus Layer (Beacon API)
        │      • Resolves 48-byte BLS pubkeys to CL Validator Indices
        │      • Confirms request inclusion in Beacon Block body
        │      • Verifies exact pair in `pending_consolidations` state queue
        │
        ├──► 3. Lido stVault / ACL Contract
        │      • Inspects `NODE_OPERATOR_FEE_EXEMPT_ROLE` on-chain state
        │      • Warns if elevated fee privileges remain unrevoked
        │
        └──► 4. Generates Audit Receipts
               • Markdown Summary (`receipt_summary.md`)
               • Canonical Machine-Readable JSON (`receipt.json`)
               • Pair-by-Pair CSV (`consolidations.csv`)
               • Raw Evidence Dumps (`evidence/`)
```

---

## Status Classification

Every validator pair is deterministically classified into one of four statuses:

| Status | Meaning |
| :--- | :--- |
| **`ACCEPTED`** | Request verified on EL, included in Beacon block, and confirmed in CL `pending_consolidations` queue. |
| **`QUEUED`** | Request verified on EL and/or Beacon block, awaiting consensus epoch processing. |
| **`NOT_ACCEPTED`** | Request failed consensus rules or was dropped from the pending queue. |
| **`INDETERMINATE`** | Evidence is missing, pruned, reorged, or RPC endpoints returned errors. |

> [!IMPORTANT]
> Missing or conflicting evidence returns **`INDETERMINATE`** rather than a false positive.

---

## Installation & Building

### From Crates.io

```bash
cargo install stvault-receipt
```

### From Source

```bash
# Clone the repository
git clone https://github.com/CreativesOnchain/lido-stvault.git
cd lido-stvault

# Build in release mode
cargo build --release

# The compiled binary will be at ./target/release/stvault-receipt
```

---

## CLI Usage

```bash
stvault-receipt [OPTIONS] --manifest <PATH> --el-tx <TX_HASH> --el-rpc <URL> --cl-beacon-api <URL>
```

### Options Reference

| Flag | Env Var | Default | Description |
| :--- | :--- | :--- | :--- |
| `-m, --manifest <PATH>` | - | *Required* | Path to Lido stVault manifest (JSON or YAML) |
| `-t, --el-tx <TX_HASH>` | - | *Required* | EL transaction hash (comma-separated or repeated) |
| `--el-rpc <URL>` | `EL_RPC_URL` | `http://127.0.0.1:8545` | Execution Layer JSON-RPC endpoint |
| `--cl-beacon-api <URL>` | `CL_BEACON_API_URL` | `http://127.0.0.1:5052` | Consensus Layer Beacon API endpoint |
| `--st-vault-dashboard <ADDR>` | `ST_VAULT_DASHBOARD` | - | Lido stVault Dashboard / ACL contract address |
| `--timeout <SECONDS>` | - | `30` | HTTP request timeout for RPC and Beacon API queries |
| `-o, --output-dir <DIR>` | - | `./stvault_receipt_output` | Directory to write receipts and evidence |
| `--format <FORMAT>` | - | `all` | Output format to print to stdout (`all`, `markdown`, `json`, `csv`) |
| `--generate-completions <SHELL>` | - | - | Generate autocompletions (`bash`, `zsh`, `fish`, `powershell`, `elvish`) |
| `-q, --quiet` | - | `false` | Suppress interactive banners and informative logs |

### Example Run

```bash
stvault-receipt \
  --manifest ./manifest.json \
  --el-tx 0x4a2a11b0c9535359a34a86b5da49b4c0bc06716035f29d20c7fdc6e9d72dc26d \
  --el-rpc http://127.0.0.1:8545 \
  --cl-beacon-api http://127.0.0.1:5052 \
  --st-vault-dashboard 0xB9D7934878B5FB9610B3fE8A5e441e8fad7E293f \
  --timeout 45 \
  --output-dir ./output
```

---

## Performance & High-Assurance Architecture

- **Stack-Allocated Hex Validation:** Fixed-size stack array decoding (`[0u8; 48]`) eliminating heap churn on thousands of validator public keys.
- **Zero-Copy CSV Streaming:** `CsvRow<'a>` serializes directly into byte buffers without per-row dynamic heap allocations.
- **In-Place Markdown Generation:** Direct formatting via `write!` streaming macros.
- **Beacon Block Request Deduplication:** Prefetches and caches Beacon blocks once per block timestamp across batch transactions.
- **Network-Agnostic Dynamic Genesis:** Queries Genesis directly from Beacon API to compute slot timings accurately on any network (Mainnet, Holesky, Sepolia, Ephemery, Hoodi).

---

## Manifest Format Support

Supports both JSON and YAML official Lido stVault manifest formats:

```json
[
  {
    "source_pubkey": "0x8a9233f81e69b07ef94dd6d9dfd7ab6c7e112d7c07dd5aa9e8a83d3e8e2e92c48858e37ab7b3117562ad846ef3294ee1",
    "target_pubkey": "0x96b6e41b9d1bb8bb4be6fb98f6d7ab7b1a206a445e9bb5f5c1d683777d13e3db85be12aa219e27c73ffbb7be2e92c488"
  }
]
```

Or nested object wrappers (`{"pairs": [...]}` or `{"consolidations": [...]}`).

---

## Generated Output Files

Running the tool produces four primary audit artifacts in the output directory:

1. **`receipt_summary.md`**: Human-readable report with status badges and metrics.
2. **`receipt.json`**: Full machine-readable receipt for automated CI/CD pipelines.
3. **`consolidations.csv`**: Tabular CSV breakdown of all source/target indices, tx hashes, and statuses.
4. **`evidence/verification_metadata.json`**: Raw configuration, block headers, and execution metadata.

---

## Shell Autocompletions

Generate autocompletions for your shell:

```bash
# Bash
stvault-receipt --generate-completions bash > ~/.local/share/bash-completion/completions/stvault-receipt

# Zsh
stvault-receipt --generate-completions zsh > ~/.zfunc/_stvault-receipt

# Fish
stvault-receipt --generate-completions fish > ~/.config/fish/completions/stvault-receipt.fish

# PowerShell
stvault-receipt --generate-completions powershell > stvault-receipt.ps1
```

---

## Containerization (Docker)

Run `stvault-receipt` via Docker without needing a local Rust toolchain:

```bash
# Build Docker image
docker build -t stvault-receipt:latest .

# Run container
docker run --rm \
  -v $(pwd)/manifest.json:/data/manifest.json:ro \
  -v $(pwd)/output:/data/output \
  --network host \
  stvault-receipt:latest \
  --manifest /data/manifest.json \
  --el-tx 0x4a2a...26d \
  --el-rpc http://localhost:8545 \
  --cl-beacon-api http://localhost:5052 \
  --output-dir /data/output
```

Or use Docker Compose:
```bash
docker compose run --rm stvault-receipt --manifest /data/manifests/hoodi_manifest_sample.json --el-tx 0x...
```

---

## CI/CD Pipeline

The project includes GitHub Actions workflows:
- **`ci.yml`**: Automated `cargo fmt --check`, `cargo clippy`, and test matrix runs across Ubuntu, macOS, and Windows.
- **`release.yml`**: Automated multi-platform binary compilation and asset publishing on `v*` git release tags.

---

## Running Tests

```bash
# Run all unit, mock integration, and wiremock tests (39 tests)
cargo test

# Run tests with live stdout output
cargo test -- --nocapture
```

---

## Safety & Scope Boundaries

- **Read-Only:** Does not broadcast transactions or submit consolidation requests.
- **Non-Invasive:** Does not automatically revoke roles or alter validator configurations.
- **Zero-Trust:** Requires direct proof from Beacon state; never relies on third-party scrapers.
- **Fail-Safe:** Any missing, ambiguous, or pruned state data returns `INDETERMINATE`.

---

## License

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE).
