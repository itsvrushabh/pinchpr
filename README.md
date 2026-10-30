# PinchPR (`pinch-pr`)

> High-performance, asynchronous Rust rewrite of [The-PR-Agent/pr-agent](https://github.com/The-PR-Agent/pr-agent).

PinchPR automates pull request reviews, 1-click code improvements, PR descriptions, and interactive Q&A using Large Language Models with a focus on speed, correctness, zero-cost abstractions, and strict GitHub API resilience.

---

## Capabilities

* **🔍 `/review`:** Automated code review covering correctness, potential bugs, security vulnerabilities, performance regressions, and quality scores with diff-validated inline comments.
* **💡 `/improve`:** Generates actionable code suggestions formatted with GitHub's native ```` ```suggestion ```` markdown syntax for **1-click commit application**.
* **📝 `/describe`:** Scans PR changes to auto-generate pull request titles, type-of-change badges, overviews, and bulleted walkthroughs.
* **❓ `/ask`:** Interactive technical Q&A answering targeted questions directly about the PR diff and architectural choices.
* **🪄 Auto-Repo Detection:** Run inside any cloned git repository without specifying `--repo`; PinchPR automatically resolves `owner/repo` from your git origin remote.
* **🦙 Local & Offline LLMs:** Fully compatible with local providers via Ollama, vLLM, and Groq through custom base URLs without OpenAI fees.

---

## Workspace Architecture

PinchPR is built as a clean, acyclic Cargo workspace:

* [`crates/pinch-core`](crates/pinch-core): Domain data structures (`PullRequest`, `DiffFile`, `ReviewResult`, `CodeSuggestion`), typed error definitions via `thiserror`, async traits (`GitProvider`, `LlmClient`), and hierarchical configuration loader.
* [`crates/pinch-diff`](crates/pinch-diff): Robust Git unified diff parser (with hunk and line indexing), file ignore filters, and dynamic BPE token budgeting via `tiktoken-rs`.
* [`crates/pinch-llm`](crates/pinch-llm): Async LLM orchestration powered by `async-openai`, supporting custom base URLs (Ollama, vLLM, Groq, DeepSeek) and JSON fence extraction fallbacks.
* [`crates/pinch-providers`](crates/pinch-providers): Git hosting provider implementations (`GitHubProvider` via `octocrab`), with commit SHA validation, inline diff position checks, and graceful degradation from GitHub 422 errors.
* [`crates/pinch-tools`](crates/pinch-tools): Pipelines for `/review`, `/improve`, `/describe`, and `/ask` workflows with embedded prompt templates.
* [`crates/pinch-cli`](crates/pinch-cli): User-facing binary CLI (`pinch-pr`).

For in-depth architectural details, see [docs/plan.md](docs/plan.md).

---

## Installation & Build

Ensure you have Rust (2021 edition) installed:

```bash
# Build binary
cargo build --release

# Run unit tests across workspace
cargo test --workspace

# Lint with Clippy
cargo clippy --workspace --all-targets -- -D warnings
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

When run inside a cloned Git repository, `--repo` is **automatically detected**. You can also pass `--repo owner/repo` explicitly.

### 1. Review a Pull Request

```bash
# Local dry-run review (prints output without posting to GitHub)
pinch-pr review --pr 42 --dry-run

# Publish live review and inline comments to GitHub
pinch-pr review --pr 42
```

### 2. Generate 1-Click Code Improvements (`/improve`)

Generates replacement blocks that can be committed directly from the GitHub PR view:

```bash
pinch-pr improve --pr 42 --dry-run
```

### 3. Generate PR Description (`/describe`)

```bash
pinch-pr describe --pr 42 --dry-run
```

### 4. Interactive Q&A (`/ask`)

```bash
pinch-pr ask "Why was Mutex replaced with an RwLock in this PR?" --pr 42 --dry-run
```

---

## Using with Local LLMs (Ollama / vLLM)

Run reviews 100% offline and free using Ollama:

```bash
# 1. Start Ollama with a coding model
ollama run qwen2.5-coder:7b

# 2. Run PinchPR pointing to Ollama
pinch-pr review \
  --pr 42 \
  --base-url http://localhost:11434/v1 \
  --model qwen2.5-coder:7b \
  --dry-run
```

---

## GitHub Actions CI Integration

You can integrate PinchPR directly into your repository workflows using `action.yml`:

```yaml
name: PinchPR Automated Review

on:
  pull_request:
    types: [opened, synchronize]

jobs:
  review:
    runs-on: ubuntu-latest
    permissions:
      contents: read
      pull-requests: write
      issues: write
    steps:
      - uses: actions/checkout@v4

      - name: Run PinchPR Review
        uses: itsvrushabh/pinchpr@master
        with:
          github-token: ${{ secrets.GITHUB_TOKEN }}
          openai-api-key: ${{ secrets.OPENAI_API_KEY }}
          command: review
```

---

## License

Licensed under either of:
- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)
