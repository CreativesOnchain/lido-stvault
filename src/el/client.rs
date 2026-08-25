use crate::error::{AppError, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct ElClient {
    rpc_url: String,
    http: Client,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxReceipt {
    pub transaction_hash: String,
    pub block_number: u64,
    pub block_hash: String,
    pub status: bool,
    pub gas_used: u64,
    pub from: String,
    pub to: Option<String>,
    pub logs: Vec<TxLog>,
    pub raw: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxLog {
    pub address: String,
    pub topics: Vec<String>,
    pub data: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxDetails {
    pub hash: String,
    pub block_number: Option<u64>,
    pub from: String,
    pub to: Option<String>,
    pub input: String,
    pub value: String,
    pub raw: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockDetails {
    pub number: u64,
    pub hash: String,
    pub timestamp: u64,
    pub raw: Value,
}

impl ElClient {
    pub fn new(rpc_url: impl Into<String>) -> Self {
        let http = Client::builder()
            .timeout(Duration::from_secs(15))
            .build()
            .expect("failed to create reqwest client");
        Self {
            rpc_url: rpc_url.into(),
            http,
        }
    }

    pub fn rpc_url(&self) -> &str {
        &self.rpc_url
    }

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

    /// Fetches transaction receipt.
    pub async fn get_transaction_receipt(&self, tx_hash: &str) -> Result<Option<TxReceipt>> {
        let result = self
            .call_rpc("eth_getTransactionReceipt", json!([tx_hash]))
            .await?;
        if result.is_null() {
            return Ok(None);
        }

        let status_hex = result
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("0x0");
        let status = status_hex == "0x1" || status_hex == "1";

        let block_number_hex = result
            .get("blockNumber")
            .and_then(|v| v.as_str())
            .unwrap_or("0x0");
        let block_number = parse_hex_u64(block_number_hex)?;

        let block_hash = result
            .get("blockHash")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let transaction_hash = result
            .get("transactionHash")
            .and_then(|v| v.as_str())
            .unwrap_or(tx_hash)
            .to_string();
        let from = result
            .get("from")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let to = result
            .get("to")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let gas_used_hex = result
            .get("gasUsed")
            .and_then(|v| v.as_str())
            .unwrap_or("0x0");
        let gas_used = parse_hex_u64(gas_used_hex).unwrap_or(0);

        let logs = if let Some(logs_arr) = result.get("logs").and_then(|v| v.as_array()) {
            logs_arr
                .iter()
                .filter_map(|l| {
                    Some(TxLog {
                        address: l.get("address")?.as_str()?.to_string(),
                        topics: l
                            .get("topics")?
                            .as_array()?
                            .iter()
                            .filter_map(|t| t.as_str().map(|s| s.to_string()))
                            .collect(),
                        data: l.get("data")?.as_str()?.to_string(),
                    })
                })
                .collect()
        } else {
            Vec::new()
        };

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

    /// Fetches transaction details.
    pub async fn get_transaction_by_hash(&self, tx_hash: &str) -> Result<Option<TxDetails>> {
        let result = self
            .call_rpc("eth_getTransactionByHash", json!([tx_hash]))
            .await?;
        if result.is_null() {
            return Ok(None);
        }

        let block_number = result
            .get("blockNumber")
            .and_then(|v| v.as_str())
            .and_then(|s| parse_hex_u64(s).ok());

        let hash = result
            .get("hash")
            .and_then(|v| v.as_str())
            .unwrap_or(tx_hash)
            .to_string();
        let from = result
            .get("from")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let to = result
            .get("to")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let input = result
            .get("input")
            .and_then(|v| v.as_str())
            .unwrap_or("0x")
            .to_string();
        let value = result
            .get("value")
            .and_then(|v| v.as_str())
            .unwrap_or("0x0")
            .to_string();

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

    /// Fetches block details by number.
    pub async fn get_block_by_number(&self, block_number: u64) -> Result<Option<BlockDetails>> {
        let block_num_hex = format!("0x{:x}", block_number);
        let result = self
            .call_rpc("eth_getBlockByNumber", json!([block_num_hex, false]))
            .await?;
        if result.is_null() {
            return Ok(None);
        }

        let hash = result
            .get("hash")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let timestamp_hex = result
            .get("timestamp")
            .and_then(|v| v.as_str())
            .unwrap_or("0x0");
        let timestamp = parse_hex_u64(timestamp_hex).unwrap_or(0);

        Ok(Some(BlockDetails {
            number: block_number,
            hash,
            timestamp,
            raw: result,
        }))
    }

    /// Executes read-only eth_call.
    pub async fn eth_call(&self, to: &str, data: &str) -> Result<String> {
        let tx_obj = json!({
            "to": to,
            "data": data,
        });
        let result = self.call_rpc("eth_call", json!([tx_obj, "latest"])).await?;
        Ok(result.as_str().unwrap_or("0x").to_string())
    }
}

fn parse_hex_u64(hex_str: &str) -> Result<u64> {
    let clean = hex_str
        .trim()
        .strip_prefix("0x")
        .or_else(|| hex_str.trim().strip_prefix("0X"))
        .unwrap_or(hex_str.trim());
    if clean.is_empty() {
        return Ok(0);
    }
    u64::from_str_radix(clean, 16)
        .map_err(|e| AppError::ElRpc(format!("Failed to parse hex u64 '{}': {}", hex_str, e)))
}
