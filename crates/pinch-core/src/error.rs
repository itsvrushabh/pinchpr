use thiserror::Error;

#[derive(Error, Debug)]
pub enum ProviderError {
    #[error("API request failed: {0}")]
    Api(String),

    #[error("Authentication error: {0}")]
    Auth(String),

    #[error("Resource not found: {0}")]
    NotFound(String),

    #[error("Rate limit exceeded. Resets at: {0}")]
    RateLimited(String),

    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Provider internal error: {0}")]
    Other(String),
}

#[derive(Error, Debug)]
pub enum LlmError {
    #[error("Authentication failed: {0}")]
    Auth(String),

    #[error("API request failed (status {status:?}): {message}")]
    Api {
        status: Option<u16>,
        message: String,
    },

    #[error("Output deserialization failed: {message}. Raw output: {raw}")]
    Deserialization { message: String, raw: String },

    #[error("Context window exceeded ({requested} tokens, limit is {max})")]
    ContextLengthExceeded { requested: usize, max: usize },

    #[error("LLM client internal error: {0}")]
    Other(String),
}

#[derive(Error, Debug)]
pub enum DiffError {
    #[error("Failed to parse diff header: {0}")]
    HeaderParse(String),

    #[error("Failed to parse hunk line: {0}")]
    HunkParse(String),

    #[error("Unsupported diff format: {0}")]
    UnsupportedFormat(String),

    #[error("Token budget calculation error: {0}")]
    TokenBudget(String),

    #[error("Diff processing error: {0}")]
    Other(String),
}

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Configuration source error: {0}")]
    Config(#[from] config::ConfigError),

    #[error("Configuration build error: {0}")]
    Build(String),

    #[error("Missing required configuration value: {0}")]
    Missing(String),

    #[error("Invalid configuration value: {0}")]
    Invalid(String),
}
