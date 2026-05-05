use std::fmt;

use serde::Serialize;
use serde_json::json;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub enum ErrorCode {
    NotFound,
    NotInitialized,
    InvalidParameter,
    RateLimited,
    Locked,
    Busy,
    Timeout,
    Internal,
}

impl ErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NotFound => "NOT_FOUND",
            Self::NotInitialized => "NOT_INITIALIZED",
            Self::InvalidParameter => "INVALID_PARAMETER",
            Self::RateLimited => "RATE_LIMITED",
            Self::Locked => "LOCKED",
            Self::Busy => "BUSY",
            Self::Timeout => "TIMEOUT",
            Self::Internal => "INTERNAL",
        }
    }
}

#[derive(Debug)]
pub struct McpError {
    code: ErrorCode,
    message: String,
}

impl McpError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub fn busy(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Busy, message)
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::NotFound, message)
    }

    pub fn not_initialized(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::NotInitialized, message)
    }

    pub fn invalid_parameter(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::InvalidParameter, message)
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Internal, message)
    }

    pub fn timeout(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Timeout, message)
    }

    pub fn code(&self) -> ErrorCode {
        self.code
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for McpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for McpError {}

pub fn classify_error(error: &anyhow::Error) -> ErrorCode {
    if let Some(mcp) = error.downcast_ref::<McpError>() {
        return mcp.code();
    }
    let message = error.to_string().to_ascii_lowercase();
    if message.contains("target not found")
        || message.contains("not found")
        || message.contains("unknown synrepo resource")
    {
        ErrorCode::NotFound
    } else if message.contains("run `synrepo init`")
        || message.contains("not initialized")
        || message.contains("failed to prepare")
    {
        ErrorCode::NotInitialized
    } else if message.contains("invalid")
        || message.contains("must ")
        || message.contains("unsupported")
        || message.contains("required")
    {
        ErrorCode::InvalidParameter
    } else if message.contains("rate limit") || message.contains("budget") {
        ErrorCode::RateLimited
    } else if message.contains("lock") || message.contains("writer") {
        ErrorCode::Locked
    } else if message.contains("busy") {
        ErrorCode::Busy
    } else if message.contains("timeout") || message.contains("timed out") {
        ErrorCode::Timeout
    } else {
        ErrorCode::Internal
    }
}

pub fn error_json(err: anyhow::Error) -> String {
    serde_json::to_string_pretty(&error_value(&err)).unwrap_or_else(|_| {
        r#"{"error":{"code":"INTERNAL","message":"serialization failure"},"error_message":"serialization failure"}"#.to_string()
    })
}

pub fn error_value(err: &anyhow::Error) -> serde_json::Value {
    let code = classify_error(err);
    let message = err
        .downcast_ref::<McpError>()
        .map(|mcp| mcp.message().to_string())
        .unwrap_or_else(|| err.to_string());
    let retryable = matches!(
        code,
        ErrorCode::RateLimited | ErrorCode::Locked | ErrorCode::Busy | ErrorCode::Timeout
    );
    let next_action = match code {
        ErrorCode::NotFound if message.contains("target not found") => {
            "Run synrepo_search with the exact symbol/path, then pass a suggested_card_targets entry or exact path to synrepo_card."
        }
        ErrorCode::NotFound => "Run synrepo_search or synrepo_overview to choose a valid target.",
        ErrorCode::NotInitialized => "Run synrepo init or synrepo project add for the repository.",
        ErrorCode::InvalidParameter => "Fix the tool parameters and retry.",
        ErrorCode::RateLimited => "Wait briefly or reduce request volume.",
        ErrorCode::Locked | ErrorCode::Busy => "Retry after the active writer or reader finishes.",
        ErrorCode::Timeout => "Retry with a narrower target or smaller budget.",
        ErrorCode::Internal => "Inspect synrepo status and retry with a narrower request.",
    };
    json!({
        "ok": false,
        "error": {
            "code": code.as_str(),
            "message": message,
            "retryable": retryable,
            "next_action": next_action,
        },
        "error_message": message,
    })
}
