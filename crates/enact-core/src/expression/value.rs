// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::fmt;

/// Runtime value in the GitHub Actions expression language.
#[derive(Debug, Clone)]
pub enum Value {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<Value>),
    Object(BTreeMap<String, Value>),
}

impl Value {
    /// GitHub Actions truthiness: false, 0, -0, "", null are falsy.
    pub fn is_truthy(&self) -> bool {
        match self {
            Value::Null => false,
            Value::Bool(b) => *b,
            Value::Number(n) => *n != 0.0,
            Value::String(s) => !s.is_empty(),
            Value::Array(_) | Value::Object(_) => true,
        }
    }

    /// Coerce to boolean following GitHub Actions rules.
    pub fn to_bool(&self) -> bool {
        self.is_truthy()
    }

    /// Coerce to f64 following GitHub Actions rules.
    pub fn to_number(&self) -> f64 {
        match self {
            Value::Null => 0.0,
            Value::Bool(true) => 1.0,
            Value::Bool(false) => 0.0,
            Value::Number(n) => *n,
            Value::String(s) => {
                let s = s.trim();
                if s.is_empty() {
                    return 0.0;
                }
                if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
                    return u64::from_str_radix(hex, 16)
                        .map(|v| v as f64)
                        .unwrap_or(f64::NAN);
                }
                s.parse::<f64>().unwrap_or(f64::NAN)
            }
            Value::Array(_) | Value::Object(_) => f64::NAN,
        }
    }

    /// Coerce to string following GitHub Actions rules.
    pub fn to_str(&self) -> String {
        match self {
            Value::Null => String::new(),
            Value::Bool(true) => "true".to_string(),
            Value::Bool(false) => "false".to_string(),
            Value::Number(n) => format_number(*n),
            Value::String(s) => s.clone(),
            Value::Array(_) | Value::Object(_) => {
                // toJSON-style output for interpolation
                serde_json::to_string(&self.to_json()).unwrap_or_default()
            }
        }
    }

    /// Convert to serde_json::Value for interop.
    pub fn to_json(&self) -> serde_json::Value {
        match self {
            Value::Null => serde_json::Value::Null,
            Value::Bool(b) => serde_json::Value::Bool(*b),
            Value::Number(n) => serde_json::Number::from_f64(*n)
                .map(serde_json::Value::Number)
                .unwrap_or(serde_json::Value::Null),
            Value::String(s) => serde_json::Value::String(s.clone()),
            Value::Array(arr) => {
                serde_json::Value::Array(arr.iter().map(|v| v.to_json()).collect())
            }
            Value::Object(map) => {
                let m: serde_json::Map<String, serde_json::Value> =
                    map.iter().map(|(k, v)| (k.clone(), v.to_json())).collect();
                serde_json::Value::Object(m)
            }
        }
    }

    /// Create from serde_json::Value.
    pub fn from_json(v: &serde_json::Value) -> Value {
        match v {
            serde_json::Value::Null => Value::Null,
            serde_json::Value::Bool(b) => Value::Bool(*b),
            serde_json::Value::Number(n) => Value::Number(n.as_f64().unwrap_or(f64::NAN)),
            serde_json::Value::String(s) => Value::String(s.clone()),
            serde_json::Value::Array(arr) => {
                Value::Array(arr.iter().map(Value::from_json).collect())
            }
            serde_json::Value::Object(map) => {
                let m: BTreeMap<String, Value> = map
                    .iter()
                    .map(|(k, v)| (k.clone(), Value::from_json(v)))
                    .collect();
                Value::Object(m)
            }
        }
    }

    /// Loose equality following GitHub Actions rules.
    /// String comparisons are case-insensitive.
    /// When types differ, both are coerced to numbers.
    pub fn loose_eq(&self, other: &Value) -> bool {
        match (self, other) {
            (Value::Null, Value::Null) => true,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Number(a), Value::Number(b)) => {
                // NaN != NaN
                if a.is_nan() || b.is_nan() {
                    return false;
                }
                a == b
            }
            (Value::String(a), Value::String(b)) => a.to_lowercase() == b.to_lowercase(),
            // Same type arrays/objects: reference identity in real GHA, we compare structurally
            (Value::Array(_), Value::Array(_)) | (Value::Object(_), Value::Object(_)) => false,
            // Different types: coerce to number
            _ => {
                let a = self.to_number();
                let b = other.to_number();
                if a.is_nan() || b.is_nan() {
                    return false;
                }
                a == b
            }
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_str())
    }
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        self.loose_eq(other)
    }
}

fn format_number(n: f64) -> String {
    if n.is_nan() {
        return "NaN".to_string();
    }
    if n.is_infinite() {
        return if n > 0.0 {
            "Infinity".to_string()
        } else {
            "-Infinity".to_string()
        };
    }
    if n == n.trunc() && n.abs() < 1e15 {
        // Integer-like: format without decimal point
        format!("{}", n as i64)
    } else {
        format!("{n}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truthiness() {
        assert!(!Value::Null.is_truthy());
        assert!(!Value::Bool(false).is_truthy());
        assert!(!Value::Number(0.0).is_truthy());
        assert!(!Value::String(String::new()).is_truthy());
        assert!(Value::Bool(true).is_truthy());
        assert!(Value::Number(1.0).is_truthy());
        assert!(Value::String("hello".into()).is_truthy());
        assert!(Value::Array(vec![]).is_truthy());
    }

    #[test]
    fn test_to_number() {
        assert_eq!(Value::Null.to_number(), 0.0);
        assert_eq!(Value::Bool(true).to_number(), 1.0);
        assert_eq!(Value::Bool(false).to_number(), 0.0);
        assert_eq!(Value::String("42".into()).to_number(), 42.0);
        assert_eq!(Value::String("0xFF".into()).to_number(), 255.0);
        assert!(Value::String("abc".into()).to_number().is_nan());
        assert_eq!(Value::String("".into()).to_number(), 0.0);
    }

    #[test]
    fn test_to_str() {
        assert_eq!(Value::Null.to_str(), "");
        assert_eq!(Value::Bool(true).to_str(), "true");
        assert_eq!(Value::Number(42.0).to_str(), "42");
        assert_eq!(Value::Number(1.2).to_str(), "1.2");
    }

    #[test]
    fn test_loose_eq() {
        // Same type
        assert!(Value::Null.loose_eq(&Value::Null));
        assert!(Value::Bool(true).loose_eq(&Value::Bool(true)));
        assert!(!Value::Bool(true).loose_eq(&Value::Bool(false)));
        // Case-insensitive strings
        assert!(Value::String("Hello".into()).loose_eq(&Value::String("hello".into())));
        // Cross-type: coerce to number
        assert!(Value::Bool(true).loose_eq(&Value::Number(1.0)));
        assert!(Value::Bool(false).loose_eq(&Value::Number(0.0)));
        assert!(Value::Null.loose_eq(&Value::Number(0.0)));
        // NaN
        assert!(!Value::String("abc".into()).loose_eq(&Value::String("def".into())));
    }

    #[test]
    fn test_json_roundtrip() {
        let v = Value::Object({
            let mut m = BTreeMap::new();
            m.insert("key".into(), Value::String("value".into()));
            m.insert("num".into(), Value::Number(42.0));
            m
        });
        let json = v.to_json();
        let back = Value::from_json(&json);
        assert_eq!(back.to_str(), v.to_str());
    }
}
