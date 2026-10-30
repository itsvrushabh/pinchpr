# Contributing

## Git workflow

1. Start from an up-to-date `master` and create a short-lived branch named for its intent, such as `feat/local-review`, `fix/diff-parser`, or `docs/contributing`.
2. Keep commits focused and explain their motivation. No specific commit-message format is required.
3. Open a pull request against `master`. Include a summary, linked issue when applicable, test commands and results, and any user-visible or breaking changes.
4. Ensure CI passes before merge. Squash merging is preferred to keep the mainline history readable.
5. Never commit tokens, provider credentials, or generated build output.

Run the project's checks locally before opening a pull request:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo build --workspace --locked
```

Repository maintainers should configure branch protection for `master`, require the CI check, and restrict force pushes and branch deletion where appropriate. These are repository settings and cannot be enforced by workflow files alone.
