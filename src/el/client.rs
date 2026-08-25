use super::types::{BlockDetails, TxDetails, TxLog, TxReceipt};
use crate::error::{AppError, Result};
use reqwest::Client;
use serde_json::{Value, json};
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct ElClient {
    rpc_url: String,
    http: Client,
}

impl ElClient {
    /// Creates a new Execution Layer JSON-RPC client with default 30s timeout.
    pub fn new(rpc_url: impl Into<String>) -> Self {
        Self::with_timeout(rpc_url, Duration::from_secs(30))
    }

    /// Creates a new Execution Layer JSON-RPC client with a custom timeout.
    pub fn with_timeout(rpc_url: impl Into<String>, timeout: Duration) -> Self {
        let http = Client::builder()
            .timeout(timeout)
            .pool_idle_timeout(Duration::from_secs(90))
            .build()
            .expect("failed to create reqwest client");
        Self {
            rpc_url: rpc_url.into(),
            http,
        }
    }

    /// Returns the RPC endpoint URL.
    pub fn rpc_url(&self) -> &str {
        &self.rpc_url
    }

    /// Dispatches a JSON-RPC 2.0 request and returns the parsed `result` payload.
    async fn call_rpc(&self, method: &str, params: Value) -> Result<Value> {
        let payload = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": method,
            "params": params,
        });

        let resp = self
            .http
            .post(&self.rpc_url)
            .json(&payload)
            .send()
            .await
            .map_err(|e| AppError::ElRpc(format!("HTTP request failed for {}: {}", method, e)))?;

        if !resp.status().is_success() {
            return Err(AppError::ElRpc(format!(
                "RPC endpoint returned HTTP status {}: {}",
                resp.status(),
                resp.text().await.unwrap_or_default()
            )));
        }

        let json_resp: Value = resp.json().await.map_err(|e| {
            AppError::ElRpc(format!(
                "Failed to parse RPC JSON response for {}: {}",
                method, e
            ))
        })?;

        if let Some(error) = json_resp.get("error") {
            return Err(AppError::ElRpc(format!(
                "RPC error for {}: {}",
                method, error
            )));
        }

        Ok(json_resp.get("result").cloned().unwrap_or(Value::Null))
    }

    /// Fetches transaction receipt by hash (`eth_getTransactionReceipt`).
    pub async fn get_transaction_receipt(&self, tx_hash: &str) -> Result<Option<TxReceipt>> {
        let result = self
            .call_rpc("eth_getTransactionReceipt", json!([tx_hash]))
            .await?;
        if result.is_null() {
            return Ok(None);
        }

        let status_hex = extract_str(&result, "status");
        let status = status_hex == "0x1" || status_hex == "1";
        let block_number = extract_hex_u64(&result, "blockNumber")?;
        let gas_used = extract_hex_u64(&result, "gasUsed").unwrap_or(0);
        let block_hash = extract_str(&result, "blockHash");
        let transaction_hash = extract_str_or(&result, "transactionHash", tx_hash);
        let from = extract_str(&result, "from");
        let to = extract_opt_str(&result, "to");
        let logs = parse_logs(result.get("logs"));

        Ok(Some(TxReceipt {
            transaction_hash,
            block_number,
            block_hash,
            status,
            gas_used,
            from,
            to,
            logs,
            raw: result,
        }))
    }

    /// Fetches transaction details by hash (`eth_getTransactionByHash`).
    pub async fn get_transaction_by_hash(&self, tx_hash: &str) -> Result<Option<TxDetails>> {
        let result = self
            .call_rpc("eth_getTransactionByHash", json!([tx_hash]))
            .await?;
        if result.is_null() {
            return Ok(None);
        }

        let block_number = extract_hex_u64(&result, "blockNumber").ok();
        let hash = extract_str_or(&result, "hash", tx_hash);
        let from = extract_str(&result, "from");
        let to = extract_opt_str(&result, "to");
        let input = extract_str_or(&result, "input", "0x");
        let value = extract_str_or(&result, "value", "0x0");

        Ok(Some(TxDetails {
            hash,
            block_number,
            from,
            to,
            input,
            value,
            raw: result,
        }))
    }

    /// Fetches block details by number (`eth_getBlockByNumber`).
    pub async fn get_block_by_number(&self, block_number: u64) -> Result<Option<BlockDetails>> {
        let block_num_hex = format!("0x{:x}", block_number);
        let result = self
            .call_rpc("eth_getBlockByNumber", json!([block_num_hex, false]))
            .await?;
        if result.is_null() {
            return Ok(None);
        }

        let hash = extract_str(&result, "hash");
        let timestamp = extract_hex_u64(&result, "timestamp").unwrap_or(0);

        Ok(Some(BlockDetails {
            number: block_number,
            hash,
            timestamp,
            raw: result,
        }))
    }

    /// Executes a read-only contract call (`eth_call`).
    pub async fn eth_call(&self, to: &str, data: &str) -> Result<String> {
        let tx_obj = json!({
            "to": to,
            "data": data,
        });
        let result = self.call_rpc("eth_call", json!([tx_obj, "latest"])).await?;
        Ok(result.as_str().unwrap_or("0x").to_string())
    }
}

// -----------------------------------------------------------------------------
// JSON Field Extraction Helpers
// -----------------------------------------------------------------------------

/// Extracts a string property from a JSON `Value`.
fn extract_str(val: &Value, field: &str) -> String {
    val.get(field)
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string()
}

/// Extracts a string property with a fallback default.
fn extract_str_or(val: &Value, field: &str, default: &str) -> String {
    val.get(field)
        .and_then(|v| v.as_str())
        .unwrap_or(default)
        .to_string()
}

/// Extracts an optional string property from a JSON `Value`.
fn extract_opt_str(val: &Value, field: &str) -> Option<String> {
    val.get(field)
        .and_then(|v| v.as_str())
        .map(ToString::to_string)
}

/// Extracts and parses a hex-encoded `u64` from a JSON `Value`.
fn extract_hex_u64(val: &Value, field: &str) -> Result<u64> {
    let hex_str = val.get(field).and_then(|v| v.as_str()).unwrap_or("0x0");
    parse_hex_u64(hex_str)
}

/// Parses transaction logs array from an RPC receipt.
fn parse_logs(logs_val: Option<&Value>) -> Vec<TxLog> {
    match logs_val.and_then(|v| v.as_array()) {
        Some(logs_arr) => logs_arr
            .iter()
            .filter_map(|l| {
                Some(TxLog {
                    address: extract_str(l, "address"),
                    topics: l
                        .get("topics")?
                        .as_array()?
                        .iter()
                        .filter_map(|t| t.as_str().map(ToString::to_string))
                        .collect(),
                    data: extract_str(l, "data"),
                })
            })
            .collect(),
        None => Vec::new(),
    }
}

/// Parses a hex string (`0x...` or clean hex) into a `u64`.
pub fn parse_hex_u64(hex_str: &str) -> Result<u64> {
    let clean = hex_str
        .trim()
        .trim_start_matches("0x")
        .trim_start_matches("0X");
    if clean.is_empty() {
        return Ok(0);
    }
    u64::from_str_radix(clean, 16)
        .map_err(|e| AppError::ElRpc(format!("Failed to parse hex u64 '{}': {}", hex_str, e)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_hex_u64() {
        assert_eq!(parse_hex_u64("0x0").unwrap(), 0);
        assert_eq!(parse_hex_u64("0x10").unwrap(), 16);
        assert_eq!(parse_hex_u64("0xff").unwrap(), 255);
        assert_eq!(parse_hex_u64("100").unwrap(), 256);
        assert_eq!(parse_hex_u64("").unwrap(), 0);
    }
}
