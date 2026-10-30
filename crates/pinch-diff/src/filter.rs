use pinch_core::DiffFile;

/// Filters out unwanted or high-noise files from PR reviews
pub struct DiffFilter {
    patterns: Vec<String>,
}

impl DiffFilter {
    pub fn new(patterns: Vec<String>) -> Self {
        Self { patterns }
    }

    /// Checks if a file path matches any ignore pattern
    pub fn should_ignore(&self, path: &str) -> bool {
        let filename = path.rsplit('/').next().unwrap_or(path);

        for pattern in &self.patterns {
            if pattern.starts_with("*.") {
                let ext = &pattern[1..]; // e.g. ".min.js" or ".svg"
                if path.ends_with(ext) || filename.ends_with(ext) {
                    return true;
                }
            } else if pattern == path || pattern == filename {
                return true;
            }
        }
        false
    }

    /// Filters a list of diff files, discarding ignored or binary files
    pub fn filter_files(&self, files: Vec<DiffFile>) -> Vec<DiffFile> {
        files
            .into_iter()
            .filter(|f| !f.is_binary && !self.should_ignore(f.path()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pinch_core::FileStatus;

    #[test]
    fn test_diff_filter() {
        let filter = DiffFilter::new(vec![
            "Cargo.lock".into(),
            "package-lock.json".into(),
            "*.min.js".into(),
            "*.svg".into(),
        ]);

        assert!(filter.should_ignore("Cargo.lock"));
        assert!(filter.should_ignore("crates/foo/Cargo.lock"));
        assert!(filter.should_ignore("assets/bundle.min.js"));
        assert!(filter.should_ignore("logo.svg"));
        assert!(!filter.should_ignore("src/main.rs"));
        assert!(!filter.should_ignore("Cargo.toml"));

        let sample_files = vec![
            DiffFile {
                old_path: Some("Cargo.lock".into()),
                new_path: Some("Cargo.lock".into()),
                status: FileStatus::Modified,
                hunks: vec![],
                is_binary: false,
            },
            DiffFile {
                old_path: Some("src/lib.rs".into()),
                new_path: Some("src/lib.rs".into()),
                status: FileStatus::Modified,
                hunks: vec![],
                is_binary: false,
            },
        ];

        let filtered = filter.filter_files(sample_files);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].path(), "src/lib.rs");
    }
}
