//! Module Overview
//! Service-layer error type definitions and conversions.
//! Normalizes error codes/messages crossing service boundaries.

use thiserror::Error;

pub type AppResult<T> = Result<T, AppError>;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("{message}")]
    Validation { message: String },
    #[error("{message}")]
    NotFound { message: String },
    #[error("{message}")]
    External { message: String },
    #[error("{message}")]
    Internal { message: String },
}

impl AppError {
    pub fn validation(message: impl Into<String>) -> Self {
        Self::Validation {
            message: message.into(),
        }
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::NotFound {
            message: message.into(),
        }
    }

    pub fn external(message: impl Into<String>) -> Self {
        Self::External {
            message: message.into(),
        }
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::Internal {
            message: message.into(),
        }
    }

    pub fn code(&self) -> &'static str {
        match self {
            Self::Validation { .. } => "validation_error",
            Self::NotFound { .. } => "not_found",
            Self::External { .. } => "external_error",
            Self::Internal { .. } => "internal_error",
        }
    }
}

impl From<String> for AppError {
    fn from(message: String) -> Self {
        Self::internal(message)
    }
}

impl From<&str> for AppError {
    fn from(message: &str) -> Self {
        Self::internal(message)
    }
}

#[cfg(test)]
mod tests {
    use super::AppError;

    #[test]
    fn app_error_code_matches_variant() {
        assert_eq!(AppError::validation("x").code(), "validation_error");
        assert_eq!(AppError::not_found("x").code(), "not_found");
        assert_eq!(AppError::external("x").code(), "external_error");
        assert_eq!(AppError::internal("x").code(), "internal_error");
    }

    #[test]
    fn app_error_display_keeps_message() {
        let err = AppError::validation("bad input");
        assert_eq!(err.to_string(), "bad input");
    }
}
