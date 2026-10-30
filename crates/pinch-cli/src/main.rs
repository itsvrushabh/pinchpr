use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use pinch_core::Settings;
use pinch_llm::OpenAiClient;
use pinch_providers::GitHubProvider;
use pinch_tools::ReviewTool;
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

    #[arg(long, help = "Override LLM API base URL for Ollama / vLLM / custom proxy")]
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
    }

    Ok(())
}

async fn run_review(args: ReviewArgs) -> Result<()> {
    // 1. Load settings with environment overrides
    let mut settings = Settings::load().context("Failed to load settings")?;

    // 2. Apply CLI overrides
    if let Some(m) = args.model {
        settings.llm.model = m;
    }
    if let Some(url) = args.base_url {
        settings.llm.base_url = Some(url);
    }

    // 3. Parse owner/repo
    let parts: Vec<&str> = args.repo.split('/').collect();
    if parts.len() != 2 || parts[0].is_empty() || parts[1].is_empty() {
        bail!("Invalid repository format '{}'. Expected 'owner/repo'", args.repo);
    }
    let owner = parts[0].to_string();
    let repo = parts[1].to_string();

    // 4. Initialize GitHub provider
    let provider = Arc::new(GitHubProvider::new(
        owner,
        repo,
        settings.github.token.clone(),
        settings.github.api_url.clone(),
    ).context("Failed to initialize GitHub provider")?);

    // 5. Initialize LLM client
    let llm = Arc::new(OpenAiClient::new(
        settings.llm.api_key.clone(),
        settings.llm.base_url.clone(),
        settings.llm.model.clone(),
        settings.llm.temperature,
        settings.llm.max_tokens,
    ));

    // 6. Execute review tool
    let tool = ReviewTool::new(provider, llm, settings);
    let result = tool.run(args.pr, args.dry_run).await?;

    // 7. Render output
    match args.format {
        OutputFormat::Markdown => {
            println!("{}", result.to_markdown_report());
        }
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
    }

    Ok(())
}
