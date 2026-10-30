use pinch_core::LlmError;
use regex::Regex;
use serde::de::DeserializeOwned;
use std::sync::LazyLock;

static JSON_BLOCK_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?s)```(?:json)?\s*(\{.*?\}|\[.*?\])\s*```").unwrap());

/// Extract and deserialize a JSON object or array from LLM response text
pub fn extract_and_parse_json<T: DeserializeOwned>(raw: &str) -> Result<T, LlmError> {
    let trimmed = raw.trim();

    // 1. Direct parse attempt
    if let Ok(val) = serde_json::from_str::<T>(trimmed) {
        return Ok(val);
    }

    // 2. Extract from markdown code fence
    if let Some(caps) = JSON_BLOCK_RE.captures(trimmed) {
        if let Some(matched) = caps.get(1) {
            if let Ok(val) = serde_json::from_str::<T>(matched.as_str()) {
                return Ok(val);
            }
        }
    }

    // 3. Fallback: Search for outer curly braces or square brackets
    if let Some(start) = trimmed.find('{') {
        if let Some(end) = trimmed.rfind('}') {
            if end > start {
                let candidate = &trimmed[start..=end];
                if let Ok(val) = serde_json::from_str::<T>(candidate) {
                    return Ok(val);
                }
            }
        }
    }

    Err(LlmError::Deserialization {
        message: "Failed to extract valid JSON payload from LLM output".to_string(),
        raw: raw.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Debug, Deserialize, PartialEq)]
    struct Sample {
        name: String,
        score: u32,
    }

    #[test]
    fn test_extract_direct_json() {
        let json = r#"{"name": "test", "score": 100}"#;
        let res: Sample = extract_and_parse_json(json).unwrap();
        assert_eq!(res.name, "test");
        assert_eq!(res.score, 100);
    }

    #[test]
    fn test_extract_markdown_fence() {
        let text = "Here is your response:\n```json\n{\"name\": \"test\", \"score\": 42}\n```\nHope that helps!";
        let res: Sample = extract_and_parse_json(text).unwrap();
        assert_eq!(res.name, "test");
        assert_eq!(res.score, 42);
    }

    #[test]
    fn test_extract_embedded_json() {
        let text = "The result is {\"name\": \"embedded\", \"score\": 7} as requested.";
        let res: Sample = extract_and_parse_json(text).unwrap();
        assert_eq!(res.name, "embedded");
        assert_eq!(res.score, 7);
    }
}
