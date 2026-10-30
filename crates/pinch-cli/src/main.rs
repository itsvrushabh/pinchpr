use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use pinch_core::Settings;
use pinch_llm::OpenAiClient;
use pinch_providers::GitHubProvider;
use pinch_tools::{AskTool, DescribeTool, ImproveTool, ReviewTool};
use std::sync::Arc;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

#[derive(Parser, Debug)]
#[command(
    name = "pinch-pr",
    version,
    about = "High-performance Rust rewrite of PR-Agent for automated AI pull request operations"
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

    #[command(about = "Generate 1-click GitHub code improvement suggestions")]
    Improve(ImproveArgs),

    #[command(about = "Auto-generate PR title, description, and walkthrough")]
    Describe(DescribeArgs),

    #[command(about = "Ask an interactive question about the pull request diff")]
    Ask(AskArgs),
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
        help = "GitHub repository in owner/repo format (auto-detected if omitted)"
    )]
    repo: Option<String>,

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
struct ImproveArgs {
    #[arg(
        long,
        help = "GitHub repository in owner/repo format (auto-detected if omitted)"
    )]
    repo: Option<String>,

    #[arg(long, help = "Pull request number")]
    pr: u64,

    #[arg(long, help = "Run locally without posting suggestions to GitHub")]
    dry_run: bool,

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

#[derive(Parser, Debug)]
struct DescribeArgs {
    #[arg(
        long,
        help = "GitHub repository in owner/repo format (auto-detected if omitted)"
    )]
    repo: Option<String>,

    #[arg(long, help = "Pull request number")]
    pr: u64,

    #[arg(long, help = "Run locally without updating GitHub")]
    dry_run: bool,

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

#[derive(Parser, Debug)]
struct AskArgs {
    #[arg(help = "Question to ask regarding the pull request diff")]
    question: String,

    #[arg(
        long,
        help = "GitHub repository in owner/repo format (auto-detected if omitted)"
    )]
    repo: Option<String>,

    #[arg(long, help = "Pull request number")]
    pr: u64,

    #[arg(long, help = "Run locally without posting answer to GitHub")]
    dry_run: bool,

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
        Commands::Improve(args) => run_improve(args).await?,
        Commands::Describe(args) => run_describe(args).await?,
        Commands::Ask(args) => run_ask(args).await?,
    }

    Ok(())
}

fn resolve_repo(repo_arg: Option<String>) -> Result<(String, String)> {
    if let Some(r) = repo_arg {
        let parts: Vec<&str> = r.split('/').collect();
        if parts.len() == 2 && !parts[0].is_empty() && !parts[1].is_empty() {
            return Ok((parts[0].to_string(), parts[1].to_string()));
        }
        bail!("Invalid repository format '{}'. Expected 'owner/repo'", r);
    }

    // Try auto-detecting from git remote origin
    if let Ok(output) = std::process::Command::new("git")
        .args(["config", "--get", "remote.origin.url"])
        .output()
    {
        if output.status.success() {
            let url = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if let Some(slug) = parse_git_remote_url(&url) {
                let parts: Vec<&str> = slug.split('/').collect();
                if parts.len() == 2 {
                    tracing::info!("Auto-detected repository from git remote: {}", slug);
                    return Ok((parts[0].to_string(), parts[1].to_string()));
                }
            }
        }
    }

    bail!("Repository not specified. Provide --repo <owner/repo> or run inside a git repository with an origin remote.");
}

fn parse_git_remote_url(url: &str) -> Option<String> {
    let trimmed = url.trim_end_matches(".git");
    if let Some(slug) = trimmed.strip_prefix("https://github.com/") {
        return Some(slug.to_string());
    }
    if let Some(slug) = trimmed.strip_prefix("http://github.com/") {
        return Some(slug.to_string());
    }
    if let Some(pos) = trimmed.find("github.com:") {
        return Some(trimmed[pos + "github.com:".len()..].to_string());
    }
    None
}

fn setup_clients(
    repo_arg: Option<String>,
    model_override: Option<String>,
    base_url_override: Option<String>,
) -> Result<(Arc<GitHubProvider>, Arc<OpenAiClient>, Settings)> {
    let mut settings = Settings::load().context("Failed to load settings")?;

    if let Some(m) = model_override {
        settings.llm.model = m;
    }
    if let Some(url) = base_url_override {
        settings.llm.base_url = Some(url);
    }

    let (owner, repo) = resolve_repo(repo_arg)?;

    let provider = Arc::new(
        GitHubProvider::new(
            owner,
            repo,
            settings.github.token.clone(),
            settings.github.api_url.clone(),
        )
        .context("Failed to initialize GitHub provider")?,
    );

    let llm = Arc::new(OpenAiClient::new(
        settings.llm.api_key.clone(),
        settings.llm.base_url.clone(),
        settings.llm.model.clone(),
        settings.llm.temperature,
        settings.llm.max_tokens,
    ));

    Ok((provider, llm, settings))
}

async fn run_review(args: ReviewArgs) -> Result<()> {
    let (provider, llm, settings) = setup_clients(args.repo, args.model, args.base_url)?;
    let tool = ReviewTool::new(provider, llm, settings);
    let result = tool.run(args.pr, args.dry_run).await?;

    match args.format {
        OutputFormat::Markdown => println!("{}", result.to_markdown_report()),
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&result)?),
    }

    Ok(())
}

async fn run_improve(args: ImproveArgs) -> Result<()> {
    let (provider, llm, settings) = setup_clients(args.repo, args.model, args.base_url)?;
    let tool = ImproveTool::new(provider, llm, settings);
    let result = tool.run(args.pr, args.dry_run).await?;

    match args.format {
        OutputFormat::Markdown => println!("{}", result.to_markdown_report()),
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&result)?),
    }

    Ok(())
}

async fn run_describe(args: DescribeArgs) -> Result<()> {
    let (provider, llm, settings) = setup_clients(args.repo, args.model, args.base_url)?;
    let tool = DescribeTool::new(provider, llm, settings);
    let result = tool.run(args.pr, args.dry_run).await?;

    match args.format {
        OutputFormat::Markdown => println!("{}", result.to_markdown_report()),
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&result)?),
    }

    Ok(())
}

async fn run_ask(args: AskArgs) -> Result<()> {
    let (provider, llm, settings) = setup_clients(args.repo, args.model, args.base_url)?;
    let tool = AskTool::new(provider, llm, settings);
    let result = tool.run(args.pr, &args.question, args.dry_run).await?;

    match args.format {
        OutputFormat::Markdown => println!("{}", result.to_markdown_report()),
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&result)?),
    }

    Ok(())
}
