use super::types::{
    BeaconBlockResponse, GenesisResponse, PendingConsolidationItem, PendingConsolidationsResponse,
    ValidatorsResponse,
};
use crate::error::{AppError, Result};
use reqwest::Client;
use std::collections::HashMap;
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct BeaconClient {
    base_url: String,
    http: Client,
}

impl BeaconClient {
    pub fn new(beacon_url: impl Into<String>) -> Self {
        let trimmed = beacon_url.into().trim_end_matches('/').to_string();
        let http = Client::builder()
            .timeout(Duration::from_secs(15))
            .build()
            .expect("failed to create reqwest client");
        Self { base_url: trimmed, http }
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Fetches beacon genesis information.
    pub async fn get_genesis(&self) -> Result<GenesisResponse> {
        let url = format!("{}/eth/v1/beacon/genesis", self.base_url);
        let resp = self.http.get(&url).send().await.map_err(|e| {
            AppError::ClBeacon(format!("Failed to connect to Beacon API at {}: {}", url, e))
        })?;

        if !resp.status().is_success() {
            return Err(AppError::ClBeacon(format!(
                "Beacon API {} returned status {}",
                url,
                resp.status()
            )));
        }

        resp.json::<GenesisResponse>()
            .await
            .map_err(|e| AppError::ClBeacon(format!("Failed to parse Genesis JSON: {}", e)))
    }

    /// Queries validator indices for a batch of public keys.
    pub async fn get_validators_by_pubkeys(
        &self,
        pubkeys: &[String],
    ) -> Result<HashMap<String, u64>> {
        if pubkeys.is_empty() {
            return Ok(HashMap::new());
        }

        // Try POST /eth/v1/beacon/states/head/validators with {"ids": [...]}
        let post_url = format!("{}/eth/v1/beacon/states/head/validators", self.base_url);
        let payload = serde_json::json!({
            "ids": pubkeys,
        });

        let mut map = HashMap::new();

        let post_res = self.http.post(&post_url).json(&payload).send().await;
        if let Ok(resp) = post_res {
            if resp.status().is_success() {
                if let Ok(parsed) = resp.json::<ValidatorsResponse>().await {
                    for item in parsed.data {
                        if let Ok(idx) = item.index.parse::<u64>() {
                            let pubkey_norm = item.validator.pubkey.to_lowercase();
                            map.insert(pubkey_norm, idx);
                        }
                    }
                    return Ok(map);
                }
            }
        }

        // Fallback: Query GET with query parameter id=pubkey1,pubkey2...
        let ids_param = pubkeys.join(",");
        let get_url =
            format!("{}/eth/v1/beacon/states/head/validators?id={}", self.base_url, ids_param);
        let resp = self.http.get(&get_url).send().await.map_err(|e| {
            AppError::ClBeacon(format!("Failed to fetch validators from {}: {}", get_url, e))
        })?;

        if !resp.status().is_success() {
            return Err(AppError::ClBeacon(format!(
                "Beacon API {} returned status {}",
                get_url,
                resp.status()
            )));
        }

        let parsed = resp.json::<ValidatorsResponse>().await.map_err(|e| {
            AppError::ClBeacon(format!("Failed to parse ValidatorsResponse: {}", e))
        })?;

        for item in parsed.data {
            if let Ok(idx) = item.index.parse::<u64>() {
                let pubkey_norm = item.validator.pubkey.to_lowercase();
                map.insert(pubkey_norm, idx);
            }
        }

        Ok(map)
    }

    /// Fetches a signed beacon block by slot number or block ID (e.g. "head", "finalized", or slot).
    pub async fn get_beacon_block(&self, slot_or_id: &str) -> Result<Option<BeaconBlockResponse>> {
        let url = format!("{}/eth/v2/beacon/blocks/{}", self.base_url, slot_or_id);
        let resp = self.http.get(&url).send().await.map_err(|e| {
            AppError::ClBeacon(format!("Failed to fetch beacon block from {}: {}", url, e))
        })?;

        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }

        if !resp.status().is_success() {
            return Err(AppError::ClBeacon(format!(
                "Beacon block endpoint {} returned status {}",
                url,
                resp.status()
            )));
        }

        let parsed = resp.json::<BeaconBlockResponse>().await.map_err(|e| {
            AppError::ClBeacon(format!("Failed to parse BeaconBlockResponse: {}", e))
        })?;

        Ok(Some(parsed))
    }

    /// Fetches the pending consolidations queue from state.
    pub async fn get_pending_consolidations(
        &self,
        state_id: &str,
    ) -> Result<Vec<PendingConsolidationItem>> {
        let url =
            format!("{}/eth/v1/beacon/states/{}/pending_consolidations", self.base_url, state_id);
        let resp = self.http.get(&url).send().await.map_err(|e| {
            AppError::ClBeacon(format!(
                "Failed to fetch pending consolidations from {}: {}",
                url, e
            ))
        })?;

        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(AppError::ClBeacon(format!(
                "Beacon endpoint {} not supported or state not found (requires EIP-7251/Electra/Pectra support)",
                url
            )));
        }

        if !resp.status().is_success() {
            return Err(AppError::ClBeacon(format!(
                "Pending consolidations endpoint {} returned status {}",
                url,
                resp.status()
            )));
        }

        let parsed = resp.json::<PendingConsolidationsResponse>().await.map_err(|e| {
            AppError::ClBeacon(format!("Failed to parse PendingConsolidationsResponse: {}", e))
        })?;

        Ok(parsed.data)
    }
}
