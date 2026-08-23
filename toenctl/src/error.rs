use thiserror::Error;

#[derive(Debug, Error)]
pub enum ToenError {
    #[error("invalid arguments: {0}")]
    InvalidArguments(String),
    #[error("workspace error: {0}")]
    Workspace(String),
    #[error("{0}")]
    Operation(String),
}

impl From<String> for ToenError {
    fn from(value: String) -> Self {
        Self::Operation(value)
    }
}

impl From<&str> for ToenError {
    fn from(value: &str) -> Self {
        Self::Operation(value.to_owned())
    }
}

#[cfg(test)]
pub(crate) fn message(value: impl Into<String>) -> String {
    ToenError::Operation(value.into()).to_string()
}
