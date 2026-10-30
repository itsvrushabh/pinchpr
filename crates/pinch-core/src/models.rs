use serde::{Deserialize, Serialize};

/// Pull Request metadata required for review orchestration and publishing
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PullRequest {
    pub number: u64,
    pub title: String,
    pub body: Option<String>,
    pub head_sha: String,
    pub base_branch: String,
    pub head_branch: String,
    pub author: String,
}

/// Status of a file within a git diff
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum FileStatus {
    Added,
    Modified,
    Deleted,
    Renamed,
    Copied,
    TypeChanged,
    Binary,
    Unknown,
}

/// Kind of diff line
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum DiffLineKind {
    Context,
    Add,
    Delete,
}

/// Single line within a diff hunk
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiffLine {
    pub kind: DiffLineKind,
    pub content: String,
    pub old_lineno: Option<u32>,
    pub new_lineno: Option<u32>,
}

/// A contiguous block of differences within a file
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiffHunk {
    pub header: String,
    pub old_start: u32,
    pub old_lines: u32,
    pub new_start: u32,
    pub new_lines: u32,
    pub lines: Vec<DiffLine>,
}

impl DiffHunk {
    /// Returns true if the hunk touches the given line in the new file (Right side)
    pub fn contains_new_line(&self, line: u32) -> bool {
        self.lines.iter().any(|l| l.new_lineno == Some(line))
    }

    /// Returns true if the hunk touches the given line in the old file (Left side)
    pub fn contains_old_line(&self, line: u32) -> bool {
        self.lines.iter().any(|l| l.old_lineno == Some(line))
    }
}

/// Represents the changes applied to a single file in a pull request
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiffFile {
    pub old_path: Option<String>,
    pub new_path: Option<String>,
    pub status: FileStatus,
    pub hunks: Vec<DiffHunk>,
    pub is_binary: bool,
}

impl DiffFile {
    /// Returns the canonical path of the file (preferring the new path)
    pub fn path(&self) -> &str {
        self.new_path
            .as_deref()
            .or(self.old_path.as_deref())
            .unwrap_or("unknown")
    }

    /// Checks if this diff includes the given line on the new (Right) side
    pub fn contains_new_line(&self, line: u32) -> bool {
        self.hunks.iter().any(|h| h.contains_new_line(line))
    }

    /// Total count of added lines
    pub fn additions_count(&self) -> usize {
        self.hunks
            .iter()
            .flat_map(|h| &h.lines)
            .filter(|l| l.kind == DiffLineKind::Add)
            .count()
    }

    /// Total count of deleted lines
    pub fn deletions_count(&self) -> usize {
        self.hunks
            .iter()
            .flat_map(|h| &h.lines)
            .filter(|l| l.kind == DiffLineKind::Delete)
            .count()
    }
}

/// Side of the diff for an inline comment (matching GitHub PR Review API)
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum CommentSide {
    #[default]
    Right,
    Left,
}

/// Inline review comment attached to a specific line/range in a file
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InlineComment {
    pub path: String,
    pub line: u32,
    pub start_line: Option<u32>,
    pub side: CommentSide,
    pub body: String,
}

/// Severity classification for review findings
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum IssueSeverity {
    Info,
    Low,
    Medium,
    High,
    Critical,
}

impl std::fmt::Display for IssueSeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IssueSeverity::Info => write!(f, "Info"),
            IssueSeverity::Low => write!(f, "Low"),
            IssueSeverity::Medium => write!(f, "Medium"),
            IssueSeverity::High => write!(f, "High"),
            IssueSeverity::Critical => write!(f, "Critical"),
        }
    }
}

/// An identified code issue, bug, or security risk
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReviewIssue {
    pub category: String,
    pub severity: IssueSeverity,
    pub description: String,
    pub file: Option<String>,
    pub line: Option<u32>,
    pub suggestion: Option<String>,
}

/// Structured outcome of a pull request review
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReviewResult {
    pub summary: String,
    pub score: Option<u8>,
    pub key_changes: Vec<String>,
    pub issues: Vec<ReviewIssue>,
    pub inline_comments: Vec<InlineComment>,
}

impl ReviewResult {
    /// Formats the review result into a clean, GitHub-flavored Markdown document
    pub fn to_markdown_report(&self) -> String {
        let mut md = String::new();

        md.push_str("## 🤖 PinchPR Automated Review\n\n");

        md.push_str("### 📋 Summary\n");
        md.push_str(&self.summary);
        md.push_str("\n\n");

        if let Some(score) = self.score {
            md.push_str(&format!("**Quality / Complexity Score:** `{score}/10`\n\n"));
        }

        if !self.key_changes.is_empty() {
            md.push_str("### 🔍 Key Changes\n");
            for change in &self.key_changes {
                md.push_str(&format!("* {change}\n"));
            }
            md.push('\n');
        }

        if !self.issues.is_empty() {
            md.push_str("### ⚠️ Identified Issues & Recommendations\n\n");
            md.push_str("| Severity | Category | File | Description |\n");
            md.push_str("| :--- | :--- | :--- | :--- |\n");
            for issue in &self.issues {
                let loc = match (&issue.file, issue.line) {
                    (Some(f), Some(l)) => format!("`{f}:{l}`"),
                    (Some(f), None) => format!("`{f}`"),
                    _ => "-".to_string(),
                };
                md.push_str(&format!(
                    "| **{}** | {} | {} | {} |\n",
                    issue.severity, issue.category, loc, issue.description
                ));
            }
            md.push('\n');
        }

        if !self.inline_comments.is_empty() {
            md.push_str("### 💬 Inline Comments Overview\n\n");
            for comment in &self.inline_comments {
                let line_str = match comment.start_line {
                    Some(start) => format!("Lines {start}-{line}", line = comment.line),
                    None => format!("Line {line}", line = comment.line),
                };
                md.push_str(&format!(
                    "**`{}`** ({line_str}):\n\n{}\n\n---\n",
                    comment.path, comment.body
                ));
            }
        }

        md.push_str("\n\n*Generated by [PinchPR](https://github.com/pinchpr/pinchpr)*\n");
        md
    }
}
