use thiserror::Error;

/// Unified error enum for all stvault-receipt operations.
#[derive(Error, Debug)]
pub enum AppError {
    #[error("Manifest error: {0}")]
    Manifest(String),

    #[error("Execution layer RPC error: {0}")]
    ElRpc(String),

    #[error("Consensus layer Beacon API error: {0}")]
    ClBeacon(String),

    #[error("Lido contract inspection error: {0}")]
    LidoContract(String),

    #[error("Verification evaluation error: {0}")]
    Evaluation(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON serialization/deserialization error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("YAML serialization/deserialization error: {0}")]
    Yaml(#[from] serde_yaml::Error),

    #[error("CSV error: {0}")]
    Csv(#[from] csv::Error),

    #[error("Hex decoding error: {0}")]
    Hex(#[from] hex::FromHexError),

    #[error("HTTP client error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("URL parse error: {0}")]
    Url(#[from] url::ParseError),
}

impl AppError {
    /// Returns the recommended CLI exit code for this error:
    /// - `2`: Manifest parsing or configuration error
    /// - `1`: Network, RPC, consensus, or verification failure
    pub fn exit_code(&self) -> u8 {
        match self {
            Self::Manifest(_) | Self::Yaml(_) => 2,
            _ => 1,
        }
    }

    /// Returns `true` if this is a manifest or input validation error.
    pub fn is_manifest_error(&self) -> bool {
        matches!(self, Self::Manifest(_) | Self::Yaml(_))
    }

    /// Returns `true` if this error occurred during network communication.
    pub fn is_network_error(&self) -> bool {
        matches!(self, Self::Http(_) | Self::ElRpc(_) | Self::ClBeacon(_))
    }
}

pub type Result<T> = std::result::Result<T, AppError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_exit_codes() {
        let manifest_err = AppError::Manifest("bad format".to_string());
        assert_eq!(manifest_err.exit_code(), 2);
        assert!(manifest_err.is_manifest_error());
        assert!(!manifest_err.is_network_error());

        let rpc_err = AppError::ElRpc("connection refused".to_string());
        assert_eq!(rpc_err.exit_code(), 1);
        assert!(!rpc_err.is_manifest_error());
        assert!(rpc_err.is_network_error());
    }

    #[test]
    fn test_error_display() {
        let err = AppError::Evaluation("test error".to_string());
        assert_eq!(
            format!("{}", err),
            "Verification evaluation error: test error"
        );
    }
}
