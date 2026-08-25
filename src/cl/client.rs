use super::types::{
    BeaconBlockResponse, GenesisResponse, PendingConsolidationItem, PendingConsolidationsResponse,
    ValidatorData, ValidatorsResponse,
};
use crate::error::{AppError, Result};
use reqwest::{Client, Response, StatusCode};
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::collections::HashMap;
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct BeaconClient {
    base_url: String,
    http: Client,
}

impl BeaconClient {
    /// Creates a new Beacon API client with a configured HTTP connection pool and timeout.
    pub fn new(beacon_url: impl Into<String>) -> Self {
        let trimmed = beacon_url.into().trim_end_matches('/').to_string();
        let http = Client::builder()
            .timeout(Duration::from_secs(15))
            .pool_idle_timeout(Duration::from_secs(90))
            .build()
            .expect("failed to create reqwest client");
        Self {
            base_url: trimmed,
            http,
        }
    }

    /// Returns the base URL of the Beacon node.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Fetches beacon genesis information.
    pub async fn get_genesis(&self) -> Result<GenesisResponse> {
        self.get_json("/eth/v1/beacon/genesis").await
    }

    /// Queries validator indices for a batch of public keys.
    ///
    /// Attempts `POST /eth/v1/beacon/states/head/validators` with body payload first;
    /// falls back to `GET /eth/v1/beacon/states/head/validators?id=...` if POST is not supported.
    pub async fn get_validators_by_pubkeys(
        &self,
        pubkeys: &[String],
    ) -> Result<HashMap<String, u64>> {
        if pubkeys.is_empty() {
            return Ok(HashMap::new());
        }

        // Primary path: POST /eth/v1/beacon/states/head/validators
        let payload = serde_json::json!({ "ids": pubkeys });
        if let Ok(parsed) = self
            .post_json::<ValidatorsResponse, _>("/eth/v1/beacon/states/head/validators", &payload)
            .await
        {
            return Ok(parse_validator_indices(parsed.data));
        }

        // Fallback path: GET with comma-separated pubkeys
        let ids_param = pubkeys.join(",");
        let path = format!("/eth/v1/beacon/states/head/validators?id={}", ids_param);
        let parsed: ValidatorsResponse = self.get_json(&path).await?;
        Ok(parse_validator_indices(parsed.data))
    }

    /// Fetches a signed beacon block by slot number or block ID (e.g. "head", "finalized", or slot number).
    pub async fn get_beacon_block(&self, slot_or_id: &str) -> Result<Option<BeaconBlockResponse>> {
        let path = format!("/eth/v2/beacon/blocks/{}", slot_or_id);
        self.get_json_opt(&path).await
    }

    /// Fetches the pending consolidations queue from state (requires EIP-7251 / Electra support).
    pub async fn get_pending_consolidations(
        &self,
        state_id: &str,
    ) -> Result<Vec<PendingConsolidationItem>> {
        let path = format!("/eth/v1/beacon/states/{}/pending_consolidations", state_id);
        let resp = self.get_response(&path).await?;

        if resp.status() == StatusCode::NOT_FOUND {
            return Err(AppError::ClBeacon(format!(
                "Beacon endpoint '{}{}' returned 404 Not Found (endpoint requires EIP-7251/Electra/Pectra support)",
                self.base_url, path
            )));
        }

        let parsed: PendingConsolidationsResponse = self.parse_json_response(resp, &path).await?;
        Ok(parsed.data)
    }

    // -------------------------------------------------------------------------
    // Internal HTTP Helper Functions
    // -------------------------------------------------------------------------

    /// Sends a GET request and deserializes the JSON response body.
    async fn get_json<T: DeserializeOwned>(&self, relative_path: &str) -> Result<T> {
        let resp = self.get_response(relative_path).await?;
        self.parse_json_response(resp, relative_path).await
    }

    /// Sends a GET request, returning `Ok(None)` if the server returns 404 NOT_FOUND.
    async fn get_json_opt<T: DeserializeOwned>(&self, relative_path: &str) -> Result<Option<T>> {
        let resp = self.get_response(relative_path).await?;
        if resp.status() == StatusCode::NOT_FOUND {
            return Ok(None);
        }
        self.parse_json_response(resp, relative_path)
            .await
            .map(Some)
    }

    /// Sends a POST request with a JSON body and deserializes the JSON response.
    async fn post_json<T: DeserializeOwned, B: Serialize + ?Sized>(
        &self,
        relative_path: &str,
        body: &B,
    ) -> Result<T> {
        let url = format!("{}{}", self.base_url, relative_path);
        let resp = self.http.post(&url).json(body).send().await.map_err(|e| {
            AppError::ClBeacon(format!(
                "POST request failed for Beacon API '{}': {}",
                url, e
            ))
        })?;

        if !resp.status().is_success() {
            return Err(AppError::ClBeacon(format!(
                "Beacon API '{}' returned HTTP status {}",
                url,
                resp.status()
            )));
        }

        self.parse_json_response(resp, relative_path).await
    }

    /// Sends a GET request and validates network connectivity.
    async fn get_response(&self, relative_path: &str) -> Result<Response> {
        let url = format!("{}{}", self.base_url, relative_path);
        self.http.get(&url).send().await.map_err(|e| {
            AppError::ClBeacon(format!(
                "Failed to connect to Beacon API at '{}': {}",
                url, e
            ))
        })
    }

    /// Validates HTTP status code and parses the response into type `T`.
    async fn parse_json_response<T: DeserializeOwned>(
        &self,
        resp: Response,
        relative_path: &str,
    ) -> Result<T> {
        let url = format!("{}{}", self.base_url, relative_path);
        if !resp.status().is_success() {
            return Err(AppError::ClBeacon(format!(
                "Beacon API '{}' returned HTTP status {}",
                url,
                resp.status()
            )));
        }

        resp.json::<T>().await.map_err(|e| {
            AppError::ClBeacon(format!(
                "Failed to parse JSON response from Beacon API '{}': {}",
                url, e
            ))
        })
    }
}

/// Helper function to parse and map validator data into a pubkey -> index lookup map.
fn parse_validator_indices(data: Vec<ValidatorData>) -> HashMap<String, u64> {
    let mut map = HashMap::with_capacity(data.len());
    for item in data {
        if let Ok(idx) = item.index.parse::<u64>() {
            let pubkey_norm = item.validator.pubkey.to_lowercase();
            map.insert(pubkey_norm, idx);
        }
    }
    map
}
