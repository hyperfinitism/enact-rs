// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::path::Path;

/// Parse a GITHUB_OUTPUT / GITHUB_ENV file with full multiline heredoc support.
///
/// Supports:
///   KEY=value                (simple)
///   KEY<<DELIMITER\nlines...\nDELIMITER  (multiline heredoc)
pub fn parse_github_file(path: &Path) -> HashMap<String, String> {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return HashMap::new(),
    };
    parse_github_file_content(&content)
}

pub fn parse_github_file_content(content: &str) -> HashMap<String, String> {
    let mut result = HashMap::new();
    let mut lines = content.lines().peekable();

    while let Some(line) = lines.next() {
        let line = line.trim_end();
        if line.is_empty() {
            continue;
        }

        // Check for heredoc: KEY<<DELIMITER
        if let Some(pos) = line.find("<<") {
            let key = line[..pos].to_string();
            let delimiter = line[pos + 2..].to_string();
            if key.is_empty() || delimiter.is_empty() {
                // Malformed, try as simple key=value
                if let Some(eq) = line.find('=') {
                    result.insert(line[..eq].to_string(), line[eq + 1..].to_string());
                }
                continue;
            }
            // Read lines until we hit the delimiter
            let mut value_lines = Vec::new();
            for next_line in lines.by_ref() {
                if next_line.trim_end() == delimiter {
                    break;
                }
                value_lines.push(next_line);
            }
            result.insert(key, value_lines.join("\n"));
        } else if let Some(eq) = line.find('=') {
            let key = line[..eq].to_string();
            let value = line[eq + 1..].to_string();
            result.insert(key, value);
        }
    }

    result
}

/// Parse a GITHUB_PATH file (newline-separated paths).
pub fn parse_github_path(path: &Path) -> Vec<String> {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    content
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .map(|l| l.to_string())
        .collect()
}

/// Parse a KEY=VALUE env file (with # comments).
pub fn parse_env_file(path: &Path) -> Result<HashMap<String, String>, std::io::Error> {
    let contents = std::fs::read_to_string(path)?;
    let mut vars = HashMap::new();
    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(pos) = line.find('=') {
            let key = line[..pos].trim().to_string();
            let value = line[pos + 1..].trim().to_string();
            vars.insert(key, value);
        }
    }
    Ok(vars)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_key_value() {
        let content = "KEY=value\nFOO=bar\n";
        let result = parse_github_file_content(content);
        assert_eq!(result.get("KEY").unwrap(), "value");
        assert_eq!(result.get("FOO").unwrap(), "bar");
    }

    #[test]
    fn test_heredoc_multiline() {
        let content = "RESULT<<EOF\nline 1\nline 2\nline 3\nEOF\n";
        let result = parse_github_file_content(content);
        assert_eq!(result.get("RESULT").unwrap(), "line 1\nline 2\nline 3");
    }

    #[test]
    fn test_mixed() {
        let content = "SIMPLE=hello\nMULTI<<DELIM\nfirst\nsecond\nDELIM\nANOTHER=world\n";
        let result = parse_github_file_content(content);
        assert_eq!(result.get("SIMPLE").unwrap(), "hello");
        assert_eq!(result.get("MULTI").unwrap(), "first\nsecond");
        assert_eq!(result.get("ANOTHER").unwrap(), "world");
    }

    #[test]
    fn test_empty_content() {
        let result = parse_github_file_content("");
        assert!(result.is_empty());
    }

    #[test]
    fn test_heredoc_with_equals_in_value() {
        let content = "DATA<<EOF\nkey=value\nanother=thing\nEOF\n";
        let result = parse_github_file_content(content);
        assert_eq!(result.get("DATA").unwrap(), "key=value\nanother=thing");
    }
}
