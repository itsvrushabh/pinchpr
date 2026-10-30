use crate::error::{LlmError, ProviderError};
use crate::models::{PullRequest, ReviewResult};
use async_trait::async_trait;

/// Abstraction over Git hosting platforms (GitHub, GitLab, local git, etc.)
#[async_trait]
pub trait GitProvider: Send + Sync {
    /// Fetch pull request metadata (title, body, branches, head SHA)
    async fn get_pr(&self, pr_number: u64) -> Result<PullRequest, ProviderError>;

    /// Fetch unified git diff string for the pull request
    async fn get_pr_diff(&self, pr_number: u64) -> Result<String, ProviderError>;

    /// Post a top-level conversation comment to the pull request
    async fn publish_comment(&self, pr_number: u64, body: &str) -> Result<(), ProviderError>;

    /// Publish a structured pull request review with inline comments on the head commit
    async fn publish_review(&self, pr_number: u64, review: &ReviewResult) -> Result<(), ProviderError>;
}

/// Abstraction over LLM completions and structured outputs
#[async_trait]
pub trait LlmClient: Send + Sync {
    /// Generate a structured JSON response deserialized into `T`
    async fn generate_structured<T: serde::de::DeserializeOwned + Send>(
        &self,
        system: &str,
        user: &str,
    ) -> Result<T, LlmError>;

    /// Generate an unstructured plain text completion
    async fn generate_text(&self, system: &str, user: &str) -> Result<String, LlmError>;
}
