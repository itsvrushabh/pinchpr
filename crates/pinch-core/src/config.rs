use crate::error::ConfigError;
use config::{Config, Environment, File};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubSettings {
    pub token: Option<String>,
    pub api_url: Option<String>,
}

impl Default for GitHubSettings {
    fn default() -> Self {
        Self {
            token: None,
            api_url: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmSettings {
    pub model: String,
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    pub max_tokens: u32,
    pub temperature: f32,
}

impl Default for LlmSettings {
    fn default() -> Self {
        Self {
            model: "gpt-4o-mini".to_string(),
            api_key: None,
            base_url: None,
            max_tokens: 4096,
            temperature: 0.2,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewSettings {
    pub max_inline_comments: usize,
    pub enable_inline_comments: bool,
    pub token_budget: usize,
    pub ignore_patterns: Vec<String>,
}

impl Default for ReviewSettings {
    fn default() -> Self {
        Self {
            max_inline_comments: 10,
            enable_inline_comments: true,
            token_budget: 60000,
            ignore_patterns: vec![
                "Cargo.lock".into(),
                "package-lock.json".into(),
                "pnpm-lock.yaml".into(),
                "yarn.lock".into(),
                "uv.lock".into(),
                "poetry.lock".into(),
                "*.min.js".into(),
                "*.min.css".into(),
                "*.svg".into(),
                "*.png".into(),
                "*.jpg".into(),
            ],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Settings {
    pub github: GitHubSettings,
    pub llm: LlmSettings,
    pub review: ReviewSettings,
}

impl Settings {
    /// Build settings from defaults, optional config file, and environment variables
    pub fn load() -> Result<Self, ConfigError> {
        let mut builder = Config::builder();

        // 1. Base defaults
        let default_settings = Settings::default();
        builder = builder.set_default("github.token", default_settings.github.token)?
            .set_default("github.api_url", default_settings.github.api_url)?
            .set_default("llm.model", default_settings.llm.model)?
            .set_default("llm.api_key", default_settings.llm.api_key)?
            .set_default("llm.base_url", default_settings.llm.base_url)?
            .set_default("llm.max_tokens", default_settings.llm.max_tokens as i64)?
            .set_default("llm.temperature", default_settings.llm.temperature as f64)?
            .set_default("review.max_inline_comments", default_settings.review.max_inline_comments as i64)?
            .set_default("review.enable_inline_comments", default_settings.review.enable_inline_comments)?
            .set_default("review.token_budget", default_settings.review.token_budget as i64)?
            .set_default("review.ignore_patterns", default_settings.review.ignore_patterns)?;

        // 2. Check local or global config file
        if Path::new("pinch-pr.toml").exists() {
            builder = builder.add_source(File::with_name("pinch-pr.toml"));
        } else if let Ok(home) = std::env::var("HOME") {
            let global_config = format!("{home}/.config/pinch-pr/config.toml");
            if Path::new(&global_config).exists() {
                builder = builder.add_source(File::with_name(&global_config));
            }
        }

        // 3. Environment variables with prefix PINCH_
        builder = builder.add_source(Environment::with_prefix("PINCH").separator("__"));

        // 4. Standalone standard environment variables
        if let Ok(gh_token) = std::env::var("GITHUB_TOKEN") {
            builder = builder.set_override("github.token", gh_token)?;
        }
        if let Ok(openai_key) = std::env::var("OPENAI_API_KEY") {
            builder = builder.set_override("llm.api_key", openai_key)?;
        }
        if let Ok(openai_base) = std::env::var("OPENAI_BASE_URL") {
            builder = builder.set_override("llm.base_url", openai_base)?;
        }

        let cfg = builder.build().map_err(|e| ConfigError::Build(e.to_string()))?;
        cfg.try_deserialize::<Settings>()
            .map_err(|e| ConfigError::Build(e.to_string()))
    }
}
