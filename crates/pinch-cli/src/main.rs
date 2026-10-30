use anyhow::{bail, ensure, Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use pinch_core::Settings;
use pinch_llm::OpenAiClient;
use pinch_providers::GitHubProvider;
use pinch_tools::{ReviewAnalyzer, ReviewTool};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::Arc;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

#[derive(Parser, Debug)]
#[command(
    name = "pinch-pr",
    version,
    about = "High-performance Rust rewrite of PR-Agent for automated AI pull request reviews"
)]
struct Cli {
    #[arg(short, long, global = true, help = "Enable verbose / debug output")]
    verbose: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    #[command(about = "Analyze and review a pull request")]
    Review(ReviewArgs),
    #[command(about = "Review committed changes on a local branch")]
    ReviewLocal(ReviewLocalArgs),
}

#[derive(Clone, Copy, Debug, ValueEnum, Default)]
enum OutputFormat {
    #[default]
    Markdown,
    Json,
}

#[derive(Parser, Debug)]
struct ReviewArgs {
    #[arg(
        long,
        help = "GitHub repository in owner/repo format (e.g. 'tokio-rs/tokio')"
    )]
    repo: String,

    #[arg(long, help = "Pull request number")]
    pr: u64,

    #[arg(
        long,
        help = "Run the review locally without posting comments to GitHub"
    )]
    dry_run: bool,

    #[arg(long, help = "Override LLM model name (e.g. 'gpt-4o', 'gpt-4o-mini')")]
    model: Option<String>,

    #[arg(
        long,
        help = "Override LLM API base URL for Ollama / vLLM / custom proxy"
    )]
    base_url: Option<String>,

    #[arg(
        long,
        value_enum,
        default_value_t = OutputFormat::Markdown,
        help = "Output display format"
    )]
    format: OutputFormat,
}

#[derive(Parser, Debug)]
struct ReviewLocalArgs {
    #[arg(long, help = "Path to a local Git repository")]
    repo_path: PathBuf,

    #[arg(long, help = "Base branch or commit to compare against")]
    base: String,

    #[arg(long, help = "Override LLM model name")]
    model: Option<String>,

    #[arg(long, help = "Override LLM API base URL")]
    base_url: Option<String>,

    #[arg(
        long,
        value_enum,
        default_value_t = OutputFormat::Markdown,
        help = "Output display format"
    )]
    format: OutputFormat,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Setup logging
    let filter = if cli.verbose {
        EnvFilter::new("debug")
    } else {
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"))
    };

    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer())
        .init();

    match cli.command {
        Commands::Review(args) => run_review(args).await?,
        Commands::ReviewLocal(args) => run_local_review(args).await?,
    }

    Ok(())
}

async fn run_review(args: ReviewArgs) -> Result<()> {
    let settings = load_settings(args.model, args.base_url)?;

    // Parse owner/repo
    let parts: Vec<&str> = args.repo.split('/').collect();
    if parts.len() != 2 || parts[0].is_empty() || parts[1].is_empty() {
        bail!(
            "Invalid repository format '{}'. Expected 'owner/repo'",
            args.repo
        );
    }
    let owner = parts[0].to_string();
    let repo = parts[1].to_string();

    // Initialize GitHub provider
    let provider = Arc::new(
        GitHubProvider::new(
            owner,
            repo,
            settings.github.token.clone(),
            settings.github.api_url.clone(),
        )
        .context("Failed to initialize GitHub provider")?,
    );

    // Initialize LLM client
    let llm = create_llm(&settings);

    // Execute review tool
    let tool = ReviewTool::new(provider, llm, settings);
    let result = tool.run(args.pr, args.dry_run).await?;

    print_result(&result, args.format)?;
    Ok(())
}

async fn run_local_review(args: ReviewLocalArgs) -> Result<()> {
    let settings = load_settings(args.model, args.base_url)?;
    let repo_path = validate_repository(&args.repo_path)?;
    let base_commit = resolve_commit(&repo_path, &args.base)?;
    let merge_base = git_output(
        &repo_path,
        ["merge-base", base_commit.as_str(), "HEAD"].as_slice(),
    )
    .context("Failed to find a merge base between the base ref and HEAD")?;
    let merge_base = String::from_utf8(merge_base.stdout)
        .context("Git returned a non-UTF-8 merge-base hash")?
        .trim()
        .to_string();
    ensure!(
        !merge_base.is_empty(),
        "Git returned an empty merge-base hash"
    );
    let head_ref = git_output(&repo_path, ["rev-parse", "--abbrev-ref", "HEAD"].as_slice())
        .context("Failed to determine the checked-out branch")?;
    let head_ref = String::from_utf8(head_ref.stdout)
        .context("Git returned a non-UTF-8 branch name")?
        .trim()
        .to_string();

    let diff = git_output(
        &repo_path,
        [
            "diff",
            "--no-ext-diff",
            "--no-textconv",
            "--find-renames",
            merge_base.as_str(),
            "HEAD",
            "--",
        ]
        .as_slice(),
    )
    .context("Failed to generate the local branch diff")?;
    let diff = String::from_utf8(diff.stdout).context("Git diff is not valid UTF-8")?;
    if diff.trim().is_empty() {
        return print_result(
            &pinch_core::ReviewResult {
                summary: format!(
                    "No committed changes were found between '{}' and '{}'.",
                    args.base, head_ref
                ),
                score: None,
                key_changes: Vec::new(),
                issues: Vec::new(),
                inline_comments: Vec::new(),
            },
            args.format,
        );
    }

    let analyzer = ReviewAnalyzer::new(create_llm(&settings), settings);
    let result = analyzer
        .analyze(
            &format!("Local branch review: {head_ref}"),
            None,
            "local",
            &args.base,
            &head_ref,
            &diff,
        )
        .await
        .context("Failed to analyze local branch changes")?;

    print_result(&result, args.format)
}

fn load_settings(model: Option<String>, base_url: Option<String>) -> Result<Settings> {
    let mut settings = Settings::load().context("Failed to load settings")?;
    if let Some(model) = model {
        settings.llm.model = model;
    }
    if let Some(base_url) = base_url {
        settings.llm.base_url = Some(base_url);
    }
    Ok(settings)
}

fn create_llm(settings: &Settings) -> Arc<OpenAiClient> {
    Arc::new(OpenAiClient::new(
        settings.llm.api_key.clone(),
        settings.llm.base_url.clone(),
        settings.llm.model.clone(),
        settings.llm.temperature,
        settings.llm.max_tokens,
    ))
}

fn print_result(result: &pinch_core::ReviewResult, format: OutputFormat) -> Result<()> {
    match format {
        OutputFormat::Markdown => {
            println!("{}", result.to_markdown_report());
        }
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
    }
    Ok(())
}

fn validate_repository(path: &Path) -> Result<PathBuf> {
    let path = path
        .canonicalize()
        .with_context(|| format!("Repository path does not exist: {}", path.display()))?;
    ensure!(
        path.is_dir(),
        "Repository path is not a directory: {}",
        path.display()
    );
    git_output(&path, ["rev-parse", "--show-toplevel"].as_slice())
        .with_context(|| format!("Not a Git repository: {}", path.display()))?;
    Ok(path)
}

fn resolve_commit(repo_path: &Path, reference: &str) -> Result<String> {
    ensure!(!reference.trim().is_empty(), "Base ref must not be empty");
    let expression = format!("{reference}^{{commit}}");
    let output = git_output(
        repo_path,
        [
            "rev-parse",
            "--verify",
            "--quiet",
            "--end-of-options",
            expression.as_str(),
        ]
        .as_slice(),
    )
    .with_context(|| format!("Base ref '{reference}' does not resolve to a commit"))?;
    let commit = String::from_utf8(output.stdout)
        .context("Git returned a non-UTF-8 commit hash")?
        .trim()
        .to_string();
    ensure!(
        !commit.is_empty(),
        "Git returned an empty commit hash for '{reference}'"
    );
    Ok(commit)
}

fn git_output(repo_path: &Path, args: &[&str]) -> Result<Output> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo_path)
        .args(args)
        .env("GIT_NO_LAZY_FETCH", "1")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .output()
        .context("Failed to run Git; ensure Git is installed and available on PATH")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("Git command failed: {}", stderr.trim());
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static NEXT_REPO_ID: AtomicU64 = AtomicU64::new(0);

    struct TestRepo(PathBuf);

    impl TestRepo {
        fn new() -> Self {
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock should be after UNIX epoch")
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "pinch-pr-test-{}-{nonce}-{}",
                std::process::id(),
                NEXT_REPO_ID.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&path).expect("temporary repository directory should be created");
            let repo = Self(path);
            repo.git(&["init", "-b", "main"]);
            repo.git(&["config", "user.name", "PinchPR Test"]);
            repo.git(&["config", "user.email", "pinchpr-test@example.invalid"]);
            repo.git(&["config", "commit.gpgsign", "false"]);
            fs::write(repo.0.join("example.txt"), "base\n")
                .expect("fixture file should be written");
            repo.git(&["add", "example.txt"]);
            repo.git(&["commit", "-m", "base"]);
            repo
        }

        fn git(&self, args: &[&str]) -> Output {
            git_output(&self.0, args).unwrap_or_else(|error| panic!("{error:#}"))
        }
    }

    impl Drop for TestRepo {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).expect("temporary repository should be removed");
        }
    }

    #[test]
    fn local_branch_diff_uses_merge_base_without_changing_repository_state() {
        let repo = TestRepo::new();
        repo.git(&["checkout", "-b", "feature"]);
        fs::write(repo.0.join("example.txt"), "base\nfeature change\n")
            .expect("fixture file should be updated");
        repo.git(&["commit", "-am", "feature change"]);

        let base = resolve_commit(&repo.0, "main").expect("main should resolve");
        let merge_base = repo.git(&["merge-base", &base, "HEAD"]);
        let merge_base = String::from_utf8(merge_base.stdout)
            .expect("merge-base should be UTF-8")
            .trim()
            .to_string();
        let before = repo.git(&["status", "--porcelain"]).stdout;
        let diff = repo.git(&["diff", &merge_base, "HEAD", "--"]).stdout;
        let after = repo.git(&["status", "--porcelain"]).stdout;

        assert!(String::from_utf8_lossy(&diff).contains("feature change"));
        assert_eq!(before, after);
    }

    #[test]
    fn local_review_rejects_an_invalid_base_ref() {
        let repo = TestRepo::new();
        let error =
            resolve_commit(&repo.0, "missing-branch").expect_err("missing base ref should fail");
        assert!(error.to_string().contains("does not resolve to a commit"));
    }
}
