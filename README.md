# PinchPR (`pinch-pr`)

> High-performance, asynchronous Rust rewrite of [The-PR-Agent/pr-agent](https://github.com/The-PR-Agent/pr-agent).

PinchPR automates pull request reviews using Large Language Models with a focus on speed, correctness, zero-cost abstractions, and strict GitHub API resilience.

---

## Workspace Architecture

The project is structured as a modular Cargo workspace:

* [`crates/pinch-core`](crates/pinch-core): Shared domain data structures (`PullRequest`, `DiffFile`, `ReviewResult`), typed error definitions via `thiserror`, async traits (`GitProvider`, `LlmClient`), and hierarchical configuration loader.
* [`crates/pinch-diff`](crates/pinch-diff): Robust Git unified diff parser (with hunk and line indexing), file ignore filters, and dynamic BPE token budgeting via `tiktoken-rs`.
* [`crates/pinch-llm`](crates/pinch-llm): Async LLM orchestration powered by `async-openai`, with support for custom base URLs (Ollama, vLLM, Groq, DeepSeek) and JSON fence extraction fallbacks.
* [`crates/pinch-providers`](crates/pinch-providers): Git hosting provider implementations (`GitHubProvider` via `octocrab`), with commit SHA validation, inline diff position checks, and graceful degradation from GitHub 422 errors.
* [`crates/pinch-tools`](crates/pinch-tools): Orchestration pipeline for `/review` workflows, prompt formatting, and structured review generation.
* [`crates/pinch-cli`](crates/pinch-cli): User-facing binary CLI (`pinch-pr`).

For full architectural details, see [docs/plan.md](docs/plan.md).

---

## Installation & Build

Ensure you have Rust 2021 edition installed (via [rustup](https://rustup.rs/)):

```bash
# Build the entire workspace
cargo build

# Run all unit tests
cargo test
```

---

## Configuration

Set your environment variables or provide a `pinch-pr.toml` configuration file:

```bash
export GITHUB_TOKEN="ghp_your_github_token"
export OPENAI_API_KEY="sk-your_openai_key"

# Optional: custom OpenAI-compatible endpoint (Ollama, vLLM, Groq)
export OPENAI_BASE_URL="http://localhost:11434/v1"
```

---

## CLI Usage

### 1. Dry-run Review (Recommended for testing)

Run a complete pull request analysis without modifying GitHub:

```bash
cargo run --bin pinch-pr -- review \
  --repo tokio-rs/tokio \
  --pr 6500 \
  --dry-run
```

### 2. Live PR Review with Inline Comments

Publish a full structured review and inline comments directly to the pull request:

```bash
cargo run --bin pinch-pr -- review \
  --repo my-org/my-repo \
  --pr 42
```

### 3. Override LLM Model & Base URL

```bash
cargo run --bin pinch-pr -- review \
  --repo my-org/my-repo \
  --pr 42 \
  --model llama3.3:70b \
  --base-url http://localhost:11434/v1 \
  --dry-run
```

### 4. Output Raw JSON

```bash
cargo run --bin pinch-pr -- review \
  --repo my-org/my-repo \
  --pr 42 \
  --format json \
  --dry-run
```

---

## License

Licensed under either of:
- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)
