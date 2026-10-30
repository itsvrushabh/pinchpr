use pinch_core::{DiffError, DiffFile, DiffLineKind};
use tiktoken_rs::{cl100k_base, CoreBPE};
use tracing::warn;

pub struct TokenBudgetManager {
    bpe: Option<CoreBPE>,
    max_budget: usize,
}

impl TokenBudgetManager {
    pub fn new(max_budget: usize) -> Self {
        let bpe = cl100k_base().ok();
        if bpe.is_none() {
            warn!("Failed to initialize cl100k_base tokenizer. Using fallback estimation.");
        }
        Self { bpe, max_budget }
    }

    /// Approximate token count of a given string
    pub fn count_tokens(&self, text: &str) -> usize {
        if let Some(bpe) = &self.bpe {
            bpe.encode_with_special_tokens(text).len()
        } else {
            // Standard fallback heuristic: ~4 characters per token
            (text.len() + 3) / 4
        }
    }

    /// Format a list of diff files into a unified prompt string, pruning files if budget is exceeded
    pub fn format_and_prune(&self, files: &[DiffFile]) -> Result<String, DiffError> {
        let mut output = String::new();
        let mut current_tokens = 0;

        // Sort files by priority: core source code first (.rs, .py, .ts, .go, .c, .cpp), then others
        let mut prioritized: Vec<&DiffFile> = files.iter().collect();
        prioritized.sort_by_key(|f| {
            let path = f.path();
            if path.ends_with(".rs") || path.ends_with(".py") || path.ends_with(".ts") || path.ends_with(".go") {
                0
            } else if path.ends_with(".toml") || path.ends_with(".json") || path.ends_with(".yaml") || path.ends_with(".yml") {
                1
            } else if path.ends_with(".md") || path.ends_with(".txt") {
                2
            } else {
                3
            }
        });

        for file in prioritized {
            let file_header = format!("\n--- File: {} ---\n", file.path());
            let header_tokens = self.count_tokens(&file_header);

            if current_tokens + header_tokens >= self.max_budget {
                output.push_str("\n[Remaining diffs omitted: reached token budget]\n");
                break;
            }

            let mut file_content = file_header.clone();
            let mut file_omitted = false;

            for hunk in &file.hunks {
                let mut hunk_str = format!("{}\n", hunk.header);
                for line in &hunk.lines {
                    let prefix = match line.kind {
                        DiffLineKind::Add => "+",
                        DiffLineKind::Delete => "-",
                        DiffLineKind::Context => " ",
                    };
                    hunk_str.push_str(&format!("{}{}\n", prefix, line.content));
                }

                let hunk_tokens = self.count_tokens(&hunk_str);
                if current_tokens + self.count_tokens(&file_content) + hunk_tokens > self.max_budget {
                    file_content.push_str("[Remaining hunks in this file omitted: exceeded token budget]\n");
                    file_omitted = true;
                    break;
                } else {
                    file_content.push_str(&hunk_str);
                }
            }

            current_tokens += self.count_tokens(&file_content);
            output.push_str(&file_content);

            if file_omitted && current_tokens >= self.max_budget {
                break;
            }
        }

        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pinch_core::{DiffHunk, DiffLine, FileStatus};

    #[test]
    fn test_budget_pruning() {
        let manager = TokenBudgetManager::new(50); // small budget

        let file = DiffFile {
            old_path: Some("src/main.rs".into()),
            new_path: Some("src/main.rs".into()),
            status: FileStatus::Modified,
            hunks: vec![DiffHunk {
                header: "@@ -1,5 +1,5 @@".into(),
                old_start: 1,
                old_lines: 5,
                new_start: 1,
                new_lines: 5,
                lines: vec![
                    DiffLine {
                        kind: DiffLineKind::Add,
                        content: "let very_long_variable_string_123456789 = true;".into(),
                        old_lineno: None,
                        new_lineno: Some(1),
                    },
                    DiffLine {
                        kind: DiffLineKind::Add,
                        content: "let another_long_variable_string_987654321 = false;".into(),
                        old_lineno: None,
                        new_lineno: Some(2),
                    },
                ],
            }],
            is_binary: false,
        };

        let formatted = manager.format_and_prune(&[file]).unwrap();
        assert!(formatted.contains("src/main.rs"));
    }
}
