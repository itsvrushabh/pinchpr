use async_trait::async_trait;
use octocrab::Octocrab;
use pinch_core::{
    CommentSide, GitProvider, InlineComment, ProviderError, PullRequest, ReviewResult,
};
use pinch_diff::parse_git_diff;
use tracing::{info, warn};

pub struct GitHubProvider {
    octocrab: Octocrab,
    owner: String,
    repo: String,
}

impl GitHubProvider {
    pub fn new(
        owner: String,
        repo: String,
        token: Option<String>,
        api_url: Option<String>,
    ) -> Result<Self, ProviderError> {
        let mut builder = Octocrab::builder();

        if let Some(t) = token {
            builder = builder.personal_token(t);
        }
        if let Some(url) = api_url {
            builder = builder
                .base_uri(url)
                .map_err(|e| ProviderError::Validation(e.to_string()))?;
        }

        let octocrab = builder
            .build()
            .map_err(|e| ProviderError::Auth(e.to_string()))?;

        Ok(Self {
            octocrab,
            owner,
            repo,
        })
    }
}

#[async_trait]
impl GitProvider for GitHubProvider {
    async fn get_pr(&self, pr_number: u64) -> Result<PullRequest, ProviderError> {
        let pr = self
            .octocrab
            .pulls(&self.owner, &self.repo)
            .get(pr_number)
            .await
            .map_err(|e| ProviderError::Api(e.to_string()))?;

        Ok(PullRequest {
            number: pr_number,
            title: pr.title.unwrap_or_default(),
            body: pr.body,
            head_sha: pr.head.sha,
            base_branch: pr.base.ref_field,
            head_branch: pr.head.ref_field,
            author: pr.user.map_or("unknown".to_string(), |u| u.login),
        })
    }

    async fn get_pr_diff(&self, pr_number: u64) -> Result<String, ProviderError> {
        self.octocrab
            .pulls(&self.owner, &self.repo)
            .get_diff(pr_number)
            .await
            .map_err(|e| ProviderError::Api(e.to_string()))
    }

    async fn publish_comment(&self, pr_number: u64, body: &str) -> Result<(), ProviderError> {
        self.octocrab
            .issues(&self.owner, &self.repo)
            .create_comment(pr_number, body)
            .await
            .map_err(|e| ProviderError::Api(e.to_string()))?;

        info!("Published general comment to PR #{}", pr_number);
        Ok(())
    }

    async fn publish_review(
        &self,
        pr_number: u64,
        review: &ReviewResult,
    ) -> Result<(), ProviderError> {
        let pr_meta = self.get_pr(pr_number).await?;
        let raw_diff = self.get_pr_diff(pr_number).await?;
        let parsed_files = parse_git_diff(&raw_diff).unwrap_or_default();

        // Validate inline comments to ensure they reference existing lines in diff hunks
        let mut valid_comments: Vec<&InlineComment> = Vec::new();
        for comment in &review.inline_comments {
            let matched_file = parsed_files.iter().find(|f| f.path() == comment.path);
            if let Some(f) = matched_file {
                let is_valid = match comment.side {
                    CommentSide::Right => f.contains_new_line(comment.line),
                    CommentSide::Left => f.hunks.iter().any(|h| h.contains_old_line(comment.line)),
                };
                if is_valid {
                    valid_comments.push(comment);
                } else {
                    warn!(
                        "Skipping inline comment on {}:{} - line not in PR diff hunk",
                        comment.path, comment.line
                    );
                }
            } else {
                warn!(
                    "Skipping inline comment on {} - file not found in PR diff",
                    comment.path
                );
            }
        }

        // Format body for review
        let body = review.to_markdown_report();

        // Attempt publishing review via Octocrab
        // If inline comments fail or GitHub returns 422, gracefully fall back to publishing comment
        let post_result = async {
            let endpoint = format!(
                "/repos/{}/{}/pulls/{}/reviews",
                self.owner, self.repo, pr_number
            );

            // Construct review payload
            let comments_json: Vec<serde_json::Value> = valid_comments
                .iter()
                .map(|c| {
                    let side_str = match c.side {
                        CommentSide::Right => "RIGHT",
                        CommentSide::Left => "LEFT",
                    };
                    let mut obj = serde_json::json!({
                        "path": c.path,
                        "body": c.body,
                        "line": c.line,
                        "side": side_str,
                    });
                    if let Some(start) = c.start_line {
                        if start < c.line {
                            obj["start_line"] = serde_json::json!(start);
                            obj["start_side"] = serde_json::json!(side_str);
                        }
                    }
                    obj
                })
                .collect();

            let payload = serde_json::json!({
                "commit_id": pr_meta.head_sha,
                "body": body,
                "event": "COMMENT",
                "comments": comments_json,
            });

            self.octocrab
                .post::<_, serde_json::Value>(&endpoint, Some(&payload))
                .await
        }
        .await;

        match post_result {
            Ok(_) => {
                info!("Published full structured review to PR #{}", pr_number);
                Ok(())
            }
            Err(e) => {
                warn!(
                    "Failed to post batch review with inline comments ({}). Falling back to discussion comment.",
                    e
                );
                self.publish_comment(pr_number, &body).await
            }
        }
    }
}
