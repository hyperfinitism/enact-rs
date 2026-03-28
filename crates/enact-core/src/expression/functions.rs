// SPDX-License-Identifier: Apache-2.0

use super::value::Value;
use crate::error::Error;
use sha2::{Digest, Sha256};
use std::path::Path;

/// Evaluate a built-in function call.
pub fn call_function(name: &str, args: &[Value], workspace: &Path) -> Result<Value, Error> {
    match name {
        "contains" => fn_contains(args),
        "startsWith" | "startswith" => fn_starts_with(args),
        "endsWith" | "endswith" => fn_ends_with(args),
        "format" => fn_format(args),
        "join" => fn_join(args),
        "toJSON" | "tojson" => fn_to_json(args),
        "fromJSON" | "fromjson" => fn_from_json(args),
        "hashFiles" | "hashfiles" => fn_hash_files(args, workspace),
        // Status functions are handled in the evaluator directly
        _ => Err(Error::UnknownFunction(name.to_string())),
    }
}

fn expect_args(name: &str, args: &[Value], min: usize, max: usize) -> Result<(), Error> {
    if args.len() < min || args.len() > max {
        return Err(Error::ExpressionEval(format!(
            "{name}() expects {min}-{max} arguments, got {}",
            args.len()
        )));
    }
    Ok(())
}

/// contains(search, item) — case-insensitive string search or array membership.
fn fn_contains(args: &[Value]) -> Result<Value, Error> {
    expect_args("contains", args, 2, 2)?;
    let result = match &args[0] {
        Value::String(haystack) => {
            let needle = args[1].to_str();
            haystack.to_lowercase().contains(&needle.to_lowercase())
        }
        Value::Array(arr) => {
            let needle = &args[1];
            arr.iter().any(|item| {
                let a = item.to_str().to_lowercase();
                let b = needle.to_str().to_lowercase();
                a == b
            })
        }
        _ => {
            let haystack = args[0].to_str();
            let needle = args[1].to_str();
            haystack.to_lowercase().contains(&needle.to_lowercase())
        }
    };
    Ok(Value::Bool(result))
}

/// startsWith(str, prefix) — case-insensitive.
fn fn_starts_with(args: &[Value]) -> Result<Value, Error> {
    expect_args("startsWith", args, 2, 2)?;
    let s = args[0].to_str().to_lowercase();
    let prefix = args[1].to_str().to_lowercase();
    Ok(Value::Bool(s.starts_with(&prefix)))
}

/// endsWith(str, suffix) — case-insensitive.
fn fn_ends_with(args: &[Value]) -> Result<Value, Error> {
    expect_args("endsWith", args, 2, 2)?;
    let s = args[0].to_str().to_lowercase();
    let suffix = args[1].to_str().to_lowercase();
    Ok(Value::Bool(s.ends_with(&suffix)))
}

/// format(fmt, args...) — replace {0}, {1}, etc. Escape braces with {{ and }}.
fn fn_format(args: &[Value]) -> Result<Value, Error> {
    if args.is_empty() {
        return Err(Error::ExpressionEval(
            "format() requires at least 1 argument".to_string(),
        ));
    }
    let fmt = args[0].to_str();
    let replacements: Vec<String> = args[1..].iter().map(|v| v.to_str()).collect();

    let mut result = String::new();
    let chars: Vec<char> = fmt.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '{' {
            if i + 1 < chars.len() && chars[i + 1] == '{' {
                result.push('{');
                i += 2;
            } else if let Some(close) = chars[i + 1..].iter().position(|&c| c == '}') {
                let index_str: String = chars[i + 1..i + 1 + close].iter().collect();
                if let Ok(idx) = index_str.parse::<usize>() {
                    result.push_str(replacements.get(idx).map(|s| s.as_str()).unwrap_or(""));
                }
                i += close + 2;
            } else {
                result.push('{');
                i += 1;
            }
        } else if chars[i] == '}' && i + 1 < chars.len() && chars[i + 1] == '}' {
            result.push('}');
            i += 2;
        } else {
            result.push(chars[i]);
            i += 1;
        }
    }
    Ok(Value::String(result))
}

/// join(array, separator) — default separator is ",".
fn fn_join(args: &[Value]) -> Result<Value, Error> {
    expect_args("join", args, 1, 2)?;
    let sep = if args.len() > 1 {
        args[1].to_str()
    } else {
        ",".to_string()
    };
    match &args[0] {
        Value::Array(arr) => {
            let parts: Vec<String> = arr.iter().map(|v| v.to_str()).collect();
            Ok(Value::String(parts.join(&sep)))
        }
        other => Ok(Value::String(other.to_str())),
    }
}

/// toJSON(value) — pretty-print JSON.
fn fn_to_json(args: &[Value]) -> Result<Value, Error> {
    expect_args("toJSON", args, 1, 1)?;
    let json = args[0].to_json();
    let s = serde_json::to_string_pretty(&json)
        .map_err(|e| Error::ExpressionEval(format!("toJSON failed: {e}")))?;
    Ok(Value::String(s))
}

/// fromJSON(str) — parse JSON to typed value.
fn fn_from_json(args: &[Value]) -> Result<Value, Error> {
    expect_args("fromJSON", args, 1, 1)?;
    let s = args[0].to_str();
    let json: serde_json::Value = serde_json::from_str(&s)
        .map_err(|e| Error::ExpressionEval(format!("fromJSON failed: {e}")))?;
    Ok(Value::from_json(&json))
}

/// hashFiles(patterns...) — SHA-256 of matched files.
fn fn_hash_files(args: &[Value], workspace: &Path) -> Result<Value, Error> {
    if args.is_empty() {
        return Err(Error::ExpressionEval(
            "hashFiles() requires at least 1 argument".to_string(),
        ));
    }
    let mut all_files: Vec<std::path::PathBuf> = Vec::new();
    for arg in args {
        let pattern = arg.to_str();
        let full_pattern = if Path::new(&pattern).is_absolute() {
            pattern
        } else {
            format!("{}/{}", workspace.display(), pattern)
        };
        if let Ok(entries) = glob::glob(&full_pattern) {
            for entry in entries.flatten() {
                if entry.is_file() {
                    all_files.push(entry);
                }
            }
        }
    }
    if all_files.is_empty() {
        return Ok(Value::String(String::new()));
    }
    all_files.sort();
    let mut hasher = Sha256::new();
    for file in &all_files {
        if let Ok(contents) = std::fs::read(file) {
            hasher.update(&contents);
        }
    }
    let hash = hasher.finalize();
    let hex = hash.iter().map(|b| format!("{b:02x}")).collect::<String>();
    Ok(Value::String(hex))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn ws() -> PathBuf {
        PathBuf::from("/tmp")
    }

    #[test]
    fn test_contains_string() {
        let args = vec![
            Value::String("Hello World".into()),
            Value::String("hello".into()),
        ];
        assert_eq!(fn_contains(&args).unwrap(), Value::Bool(true));
    }

    #[test]
    fn test_contains_array() {
        let args = vec![
            Value::Array(vec![
                Value::String("foo".into()),
                Value::String("bar".into()),
            ]),
            Value::String("Foo".into()),
        ];
        assert_eq!(fn_contains(&args).unwrap(), Value::Bool(true));
    }

    #[test]
    fn test_starts_with() {
        let args = vec![
            Value::String("Hello World".into()),
            Value::String("hello".into()),
        ];
        assert_eq!(fn_starts_with(&args).unwrap(), Value::Bool(true));
    }

    #[test]
    fn test_ends_with() {
        let args = vec![
            Value::String("Hello World".into()),
            Value::String("WORLD".into()),
        ];
        assert_eq!(fn_ends_with(&args).unwrap(), Value::Bool(true));
    }

    #[test]
    fn test_format() {
        let args = vec![
            Value::String("Hello {0}, you are {1}!".into()),
            Value::String("world".into()),
            Value::Number(42.0),
        ];
        let result = fn_format(&args).unwrap();
        assert_eq!(result.to_str(), "Hello world, you are 42!");
    }

    #[test]
    fn test_format_escape_braces() {
        let args = vec![
            Value::String("{{0}} = {0}".into()),
            Value::String("x".into()),
        ];
        let result = fn_format(&args).unwrap();
        assert_eq!(result.to_str(), "{0} = x");
    }

    #[test]
    fn test_join() {
        let args = vec![
            Value::Array(vec![
                Value::String("a".into()),
                Value::String("b".into()),
                Value::String("c".into()),
            ]),
            Value::String(", ".into()),
        ];
        let result = fn_join(&args).unwrap();
        assert_eq!(result.to_str(), "a, b, c");
    }

    #[test]
    fn test_to_json() {
        let args = vec![Value::Bool(true)];
        let result = fn_to_json(&args).unwrap();
        assert_eq!(result.to_str(), "true");
    }

    #[test]
    fn test_from_json() {
        let args = vec![Value::String(r#"{"key": "value"}"#.into())];
        let result = fn_from_json(&args).unwrap();
        match result {
            Value::Object(map) => {
                assert_eq!(map.get("key").unwrap().to_str(), "value");
            }
            other => panic!("expected object, got {other:?}"),
        }
    }

    #[test]
    fn test_hash_files_empty() {
        let args = vec![Value::String("nonexistent_pattern_*.xyz".into())];
        let result = call_function("hashFiles", &args, &ws()).unwrap();
        assert_eq!(result.to_str(), "");
    }
}
