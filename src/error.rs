use thiserror::Error;

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

pub type Result<T> = std::result::Result<T, AppError>;
