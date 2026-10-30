use crate::prompts::{format_ask_user_prompt, DEFAULT_ASK_SYSTEM_PROMPT};
use anyhow::{Context, Result};
use pinch_core::{AskResult, GitProvider, LlmClient, Settings};
use pinch_diff::{parse_git_diff, DiffFilter, TokenBudgetManager};
use std::sync::Arc;
use tracing::info;

pub struct AskTool<L: LlmClient = pinch_llm::OpenAiClient> {
    provider: Arc<dyn GitProvider>,
    llm: Arc<L>,
    settings: Settings,
}

impl<L: LlmClient> AskTool<L> {
    pub fn new(provider: Arc<dyn GitProvider>, llm: Arc<L>, settings: Settings) -> Self {
        Self {
            provider,
            llm,
            settings,
        }
    }

    /// Execute the PR Q&A workflow
    pub async fn run(&self, pr_number: u64, question: &str, dry_run: bool) -> Result<AskResult> {
        info!(
            "Starting Q&A pipeline for PR #{}: '{}'",
            pr_number, question
        );

        let pr = self
            .provider
            .get_pr(pr_number)
            .await
            .context("Failed to fetch PR metadata")?;

        let raw_diff = self
            .provider
            .get_pr_diff(pr_number)
            .await
            .context("Failed to fetch PR diff")?;

        let parsed_files = parse_git_diff(&raw_diff).context("Failed to parse git diff")?;
        let filter = DiffFilter::new(self.settings.review.ignore_patterns.clone());
        let filtered_files = filter.filter_files(parsed_files);

        let budget_manager = TokenBudgetManager::new(self.settings.review.token_budget);
        let budgeted_diff = budget_manager
            .format_and_prune(&filtered_files)
            .context("Failed to budget and format diff")?;

        let user_prompt =
            format_ask_user_prompt(question, &pr.title, pr.body.as_deref(), &budgeted_diff);

        info!("Querying LLM with question...");
        let result: AskResult = self
            .llm
            .generate_structured(DEFAULT_ASK_SYSTEM_PROMPT, &user_prompt)
            .await
            .context("Failed to answer question via LLM")?;

        if dry_run {
            info!("Dry-run mode active. Answer will not be posted to GitHub.");
        } else {
            info!("Posting answer to PR #{}...", pr_number);
            let report = result.to_markdown_report();
            self.provider
                .publish_comment(pr_number, &report)
                .await
                .context("Failed to publish answer to PR")?;
            info!("Answer posted successfully!");
        }

        Ok(result)
    }
}
