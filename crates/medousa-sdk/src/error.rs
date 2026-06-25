use thiserror::Error;

#[derive(Debug, Error)]
pub enum SdkError {
    #[error("http error: {0}")]
    Http(String),
    #[error("serialization error: {0}")]
    Serde(String),
    #[error("transport error: {0}")]
    Transport(String),
}

impl From<reqwest::Error> for SdkError {
    fn from(value: reqwest::Error) -> Self {
        Self::Http(value.to_string())
    }
}

impl From<serde_json::Error> for SdkError {
    fn from(value: serde_json::Error) -> Self {
        Self::Serde(value.to_string())
    }
}
