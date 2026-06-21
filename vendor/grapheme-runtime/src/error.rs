use thiserror::Error;

#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error("runtime error: {0}")]
    RuntimeError(String),

    #[error("artifact compatibility error: {0}")]
    ArtifactCompatibilityError(String),

    #[error("artifact integrity error: {0}")]
    ArtifactIntegrityError(String),
}
