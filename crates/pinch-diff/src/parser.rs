use pinch_core::{DiffError, DiffFile, DiffHunk, DiffLine, DiffLineKind, FileStatus};
use regex::Regex;
use std::sync::LazyLock;

static HUNK_HEADER_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^@@\s+-(\d+)(?:,(\d+))?\s+\+(\d+)(?:,(\d+))?\s+@@(.*)$").unwrap()
});

/// Parse a raw Git unified diff into structured `Vec<DiffFile>`
pub fn parse_git_diff(raw_diff: &str) -> Result<Vec<DiffFile>, DiffError> {
    let mut files = Vec::new();
    let mut current_file: Option<DiffFile> = None;
    let mut current_hunk: Option<DiffHunk> = None;

    let mut current_old_line = 0u32;
    let mut current_new_line = 0u32;

    for line in raw_diff.lines() {
        if line.starts_with("diff --git ") {
            // Finalize previous hunk and file
            if let Some(hunk) = current_hunk.take() {
                if let Some(file) = &mut current_file {
                    file.hunks.push(hunk);
                }
            }
            if let Some(file) = current_file.take() {
                files.push(file);
            }

            // Extract paths: "diff --git a/path/to/file b/path/to/file"
            let (old_p, new_p) = parse_diff_git_paths(line);

            current_file = Some(DiffFile {
                old_path: old_p,
                new_path: new_p,
                status: FileStatus::Modified,
                hunks: Vec::new(),
                is_binary: false,
            });
            continue;
        }

        if let Some(file) = &mut current_file {
            if line.starts_with("new file mode") {
                file.status = FileStatus::Added;
                continue;
            }
            if line.starts_with("deleted file mode") {
                file.status = FileStatus::Deleted;
                continue;
            }
            if line.starts_with("similarity index") || line.starts_with("rename from") || line.starts_with("rename to") {
                file.status = FileStatus::Renamed;
                continue;
            }
            if line.starts_with("Binary files ") && line.ends_with(" differ") {
                file.is_binary = true;
                file.status = FileStatus::Binary;
                continue;
            }
            if line.starts_with("--- ") {
                let path = line.trim_start_matches("--- ").trim();
                if path != "/dev/null" {
                    let cleaned = path.strip_prefix("a/").unwrap_or(path);
                    file.old_path = Some(cleaned.to_string());
                } else {
                    file.status = FileStatus::Added;
                }
                continue;
            }
            if line.starts_with("+++ ") {
                let path = line.trim_start_matches("+++ ").trim();
                if path != "/dev/null" {
                    let cleaned = path.strip_prefix("b/").unwrap_or(path);
                    file.new_path = Some(cleaned.to_string());
                } else {
                    file.status = FileStatus::Deleted;
                }
                continue;
            }
        }

        // Hunk header: @@ -old_start,old_count +new_start,new_count @@
        if line.starts_with("@@ ") {
            if let Some(hunk) = current_hunk.take() {
                if let Some(file) = &mut current_file {
                    file.hunks.push(hunk);
                }
            }

            if let Some(caps) = HUNK_HEADER_RE.captures(line) {
                let old_start: u32 = caps[1].parse().unwrap_or(1);
                let old_lines: u32 = caps.get(2).map_or(1, |m| m.as_str().parse().unwrap_or(1));
                let new_start: u32 = caps[3].parse().unwrap_or(1);
                let new_lines: u32 = caps.get(4).map_or(1, |m| m.as_str().parse().unwrap_or(1));

                current_old_line = old_start;
                current_new_line = new_start;

                current_hunk = Some(DiffHunk {
                    header: line.to_string(),
                    old_start,
                    old_lines,
                    new_start,
                    new_lines,
                    lines: Vec::new(),
                });
            }
            continue;
        }

        // Hunk body lines
        if let Some(hunk) = &mut current_hunk {
            if line.starts_with('+') && !line.starts_with("+++") {
                hunk.lines.push(DiffLine {
                    kind: DiffLineKind::Add,
                    content: line[1..].to_string(),
                    old_lineno: None,
                    new_lineno: Some(current_new_line),
                });
                current_new_line += 1;
            } else if line.starts_with('-') && !line.starts_with("---") {
                hunk.lines.push(DiffLine {
                    kind: DiffLineKind::Delete,
                    content: line[1..].to_string(),
                    old_lineno: Some(current_old_line),
                    new_lineno: None,
                });
                current_old_line += 1;
            } else if line.starts_with(' ') {
                hunk.lines.push(DiffLine {
                    kind: DiffLineKind::Context,
                    content: line[1..].to_string(),
                    old_lineno: Some(current_old_line),
                    new_lineno: Some(current_new_line),
                });
                current_old_line += 1;
                current_new_line += 1;
            } else if line.starts_with('\\') {
                // e.g. "\ No newline at end of file" - skip or ignore for line numbering
                continue;
            }
        }
    }

    // Flush last hunk and file
    if let Some(hunk) = current_hunk {
        if let Some(file) = &mut current_file {
            file.hunks.push(hunk);
        }
    }
    if let Some(file) = current_file {
        files.push(file);
    }

    Ok(files)
}

fn parse_diff_git_paths(header: &str) -> (Option<String>, Option<String>) {
    let parts: Vec<&str> = header.split_whitespace().collect();
    if parts.len() >= 4 {
        let old_p = parts[2].strip_prefix("a/").unwrap_or(parts[2]).to_string();
        let new_p = parts[3].strip_prefix("b/").unwrap_or(parts[3]).to_string();
        (Some(old_p), Some(new_p))
    } else {
        (None, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_diff() {
        let raw = r#"diff --git a/src/main.rs b/src/main.rs
index e69de29..496d64b 100644
--- a/src/main.rs
+++ b/src/main.rs
@@ -1,3 +1,4 @@
 fn main() {
-    println!("hello");
+    println!("hello world");
+    println!("new line");
 }
"#;
        let files = parse_git_diff(raw).unwrap();
        assert_eq!(files.len(), 1);
        let f = &files[0];
        assert_eq!(f.path(), "src/main.rs");
        assert_eq!(f.status, FileStatus::Modified);
        assert_eq!(f.hunks.len(), 1);

        let hunk = &f.hunks[0];
        assert_eq!(hunk.old_start, 1);
        assert_eq!(hunk.new_start, 1);
        assert_eq!(hunk.lines.len(), 5);

        // Verification of line numbers
        assert_eq!(hunk.lines[0].kind, DiffLineKind::Context);
        assert_eq!(hunk.lines[0].old_lineno, Some(1));
        assert_eq!(hunk.lines[0].new_lineno, Some(1));

        assert_eq!(hunk.lines[1].kind, DiffLineKind::Delete);
        assert_eq!(hunk.lines[1].old_lineno, Some(2));
        assert_eq!(hunk.lines[1].new_lineno, None);

        assert_eq!(hunk.lines[2].kind, DiffLineKind::Add);
        assert_eq!(hunk.lines[2].old_lineno, None);
        assert_eq!(hunk.lines[2].new_lineno, Some(2));

        assert_eq!(hunk.lines[3].kind, DiffLineKind::Add);
        assert_eq!(hunk.lines[3].old_lineno, None);
        assert_eq!(hunk.lines[3].new_lineno, Some(3));

        assert_eq!(hunk.lines[4].kind, DiffLineKind::Context);
        assert_eq!(hunk.lines[4].old_lineno, Some(3));
        assert_eq!(hunk.lines[4].new_lineno, Some(4));

        assert!(f.contains_new_line(2));
        assert!(f.contains_new_line(3));
        assert!(!f.contains_new_line(99));
    }

    #[test]
    fn test_parse_new_file() {
        let raw = r#"diff --git a/hello.txt b/hello.txt
new file mode 100644
index 0000000..ce01362
--- /dev/null
+++ b/hello.txt
@@ -0,0 +1,2 @@
+hello
+world
"#;
        let files = parse_git_diff(raw).unwrap();
        assert_eq!(files.len(), 1);
        let f = &files[0];
        assert_eq!(f.status, FileStatus::Added);
        assert_eq!(f.additions_count(), 2);
        assert_eq!(f.deletions_count(), 0);
    }
}
