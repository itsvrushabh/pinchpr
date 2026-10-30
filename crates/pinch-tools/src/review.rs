use crate::prompts::{format_review_user_prompt, DEFAULT_REVIEW_SYSTEM_PROMPT};
use anyhow::{Context, Result};
use pinch_core::{CommentSide, GitProvider, LlmClient, ReviewResult, Settings};
use pinch_diff::{parse_git_diff, DiffFilter, TokenBudgetManager};
use std::sync::Arc;
use tracing::{info, warn};

pub struct ReviewTool<L: LlmClient = pinch_llm::OpenAiClient> {
    provider: Arc<dyn GitProvider>,
    analyzer: ReviewAnalyzer<L>,
}

pub struct ReviewAnalyzer<L: LlmClient = pinch_llm::OpenAiClient> {
    llm: Arc<L>,
    settings: Settings,
}

impl<L: LlmClient> ReviewTool<L> {
    pub fn new(provider: Arc<dyn GitProvider>, llm: Arc<L>, settings: Settings) -> Self {
        Self {
            provider,
            analyzer: ReviewAnalyzer::new(llm, settings),
        }
    }

    /// Execute the full review workflow
    pub async fn run(&self, pr_number: u64, dry_run: bool) -> Result<ReviewResult> {
        info!("Starting review pipeline for PR #{}", pr_number);

        // 1. Fetch PR metadata
        let pr = self
            .provider
            .get_pr(pr_number)
            .await
            .context("Failed to fetch PR metadata")?;

        info!(
            "Fetched metadata for PR #{}: '{}' by {}",
            pr.number, pr.title, pr.author
        );

        // 2. Fetch raw PR diff
        let raw_diff = self
            .provider
            .get_pr_diff(pr_number)
            .await
            .context("Failed to fetch PR diff")?;

        let review = self
            .analyzer
            .analyze(
                &pr.title,
                pr.body.as_deref(),
                &pr.author,
                &pr.base_branch,
                &pr.head_branch,
                &raw_diff,
            )
            .await?;

        if dry_run {
            info!("Dry-run mode active. Review will not be published to GitHub.");
        } else {
            info!("Publishing review to PR #{}...", pr_number);
            self.provider
                .publish_review(pr_number, &review)
                .await
                .context("Failed to publish review to Git provider")?;
            info!("Review published successfully!");
        }

        Ok(review)
    }
}

impl<L: LlmClient> ReviewAnalyzer<L> {
    pub fn new(llm: Arc<L>, settings: Settings) -> Self {
        Self { llm, settings }
    }

    pub async fn analyze(
        &self,
        title: &str,
        body: Option<&str>,
        author: &str,
        base_branch: &str,
        head_branch: &str,
        raw_diff: &str,
    ) -> Result<ReviewResult> {
        let parsed_files = parse_git_diff(raw_diff).context("Failed to parse git diff")?;
        info!("Parsed {} files from diff", parsed_files.len());

        // Filter ignored/binary files
        let filter = DiffFilter::new(self.settings.review.ignore_patterns.clone());
        let filtered_files = filter.filter_files(parsed_files);
        info!(
            "Retained {} relevant files after filtering",
            filtered_files.len()
        );

        let budget_manager = TokenBudgetManager::new(self.settings.review.token_budget);
        let budgeted_diff = budget_manager
            .format_and_prune(&filtered_files)
            .context("Failed to budget and format diff")?;

        let user_prompt = format_review_user_prompt(
            title,
            body,
            author,
            base_branch,
            head_branch,
            &budgeted_diff,
        );

        info!(
            "Sending review request to LLM model: {}",
            self.settings.llm.model
        );
        let mut review: ReviewResult = self
            .llm
            .generate_structured(DEFAULT_REVIEW_SYSTEM_PROMPT, &user_prompt)
            .await
            .context("Failed to generate structured review from LLM")?;

        if !self.settings.review.enable_inline_comments {
            review.inline_comments.clear();
        } else {
            review.inline_comments.retain(|c| {
                if let Some(f) = filtered_files.iter().find(|f| f.path() == c.path) {
                    match c.side {
                        CommentSide::Right => f.contains_new_line(c.line),
                        CommentSide::Left => f.hunks.iter().any(|h| h.contains_old_line(c.line)),
                    }
                } else {
                    false
                }
            });

            if review.inline_comments.len() > self.settings.review.max_inline_comments {
                warn!(
                    "Limiting inline comments from {} to {}",
                    review.inline_comments.len(),
                    self.settings.review.max_inline_comments
                );
                review
                    .inline_comments
                    .truncate(self.settings.review.max_inline_comments);
            }
        }

        Ok(review)
    }
}
