// SPDX-License-Identifier: Apache-2.0

use super::model::Matrix;
use std::collections::HashMap;

/// Expand a matrix strategy into a list of concrete parameter combinations.
/// Each entry in the result is one job run with its matrix variable values.
pub fn expand_matrix(matrix: &Matrix) -> Vec<HashMap<String, serde_json::Value>> {
    // 1. Extract dimension arrays (excluding include/exclude which are handled separately)
    let mut dimensions: Vec<(String, Vec<serde_json::Value>)> = Vec::new();
    for (key, value) in &matrix.dimensions {
        let values = match value {
            serde_json::Value::Array(arr) => arr.clone(),
            other => vec![other.clone()],
        };
        if !values.is_empty() {
            dimensions.push((key.clone(), values));
        }
    }

    // 2. Compute cartesian product
    let mut combinations: Vec<HashMap<String, serde_json::Value>> = vec![HashMap::new()];
    for (key, values) in &dimensions {
        let mut new_combinations = Vec::new();
        for combo in &combinations {
            for val in values {
                let mut new_combo = combo.clone();
                new_combo.insert(key.clone(), val.clone());
                new_combinations.push(new_combo);
            }
        }
        combinations = new_combinations;
    }

    // 3. Apply exclude
    if let Some(excludes) = &matrix.exclude {
        combinations.retain(|combo| {
            !excludes.iter().any(|exclude| {
                exclude
                    .iter()
                    .all(|(k, v)| combo.get(k).is_some_and(|cv| cv == v))
            })
        });
    }

    // 4. Apply include
    if let Some(includes) = &matrix.include {
        for include in includes {
            // Check if this include matches an existing combination
            let mut matched = false;
            for combo in &mut combinations {
                let matches = include
                    .iter()
                    .all(|(k, v)| combo.get(k).is_none() || combo.get(k) == Some(v));
                if matches && include.keys().any(|k| combo.contains_key(k)) {
                    // Add extra properties to existing match
                    for (k, v) in include {
                        combo.insert(k.clone(), v.clone());
                    }
                    matched = true;
                }
            }
            if !matched {
                // Add as entirely new combination
                combinations.push(include.clone());
            }
        }
    }

    combinations
}

/// Format a matrix combination for display (e.g., "(os: ubuntu, node: 18)").
pub fn format_matrix_combo(combo: &HashMap<String, serde_json::Value>) -> String {
    if combo.is_empty() {
        return String::new();
    }
    let mut parts: Vec<String> = combo
        .iter()
        .map(|(k, v)| {
            let val = match v {
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            format!("{k}: {val}")
        })
        .collect();
    parts.sort();
    format!(" ({})", parts.join(", "))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn make_matrix(dims: Vec<(&str, Vec<serde_json::Value>)>) -> Matrix {
        let mut dimensions = HashMap::new();
        for (k, v) in dims {
            dimensions.insert(k.to_string(), json!(v));
        }
        Matrix {
            include: None,
            exclude: None,
            dimensions,
        }
    }

    #[test]
    fn test_simple_expansion() {
        let m = make_matrix(vec![
            ("os", vec![json!("ubuntu"), json!("macos")]),
            ("node", vec![json!(18), json!(20)]),
        ]);
        let combos = expand_matrix(&m);
        assert_eq!(combos.len(), 4);
    }

    #[test]
    fn test_single_dimension() {
        let m = make_matrix(vec![("os", vec![json!("ubuntu"), json!("macos")])]);
        let combos = expand_matrix(&m);
        assert_eq!(combos.len(), 2);
    }

    #[test]
    fn test_exclude() {
        let mut m = make_matrix(vec![
            ("os", vec![json!("ubuntu"), json!("windows")]),
            ("node", vec![json!(18), json!(20)]),
        ]);
        m.exclude = Some(vec![{
            let mut ex = HashMap::new();
            ex.insert("os".into(), json!("windows"));
            ex.insert("node".into(), json!(18));
            ex
        }]);
        let combos = expand_matrix(&m);
        assert_eq!(combos.len(), 3); // 4 - 1 excluded
    }

    #[test]
    fn test_include_new_combo() {
        let mut m = make_matrix(vec![("os", vec![json!("ubuntu")])]);
        m.include = Some(vec![{
            let mut inc = HashMap::new();
            inc.insert("os".into(), json!("macos"));
            inc.insert("extra".into(), json!(true));
            inc
        }]);
        let combos = expand_matrix(&m);
        assert_eq!(combos.len(), 2);
        let macos = combos.iter().find(|c| c.get("os") == Some(&json!("macos")));
        assert!(macos.is_some());
        assert_eq!(macos.unwrap().get("extra"), Some(&json!(true)));
    }

    #[test]
    fn test_empty_matrix() {
        let m = Matrix {
            include: None,
            exclude: None,
            dimensions: HashMap::new(),
        };
        let combos = expand_matrix(&m);
        assert_eq!(combos.len(), 1); // Single empty combination
    }

    #[test]
    fn test_format() {
        let mut combo = HashMap::new();
        combo.insert("os".into(), json!("ubuntu"));
        combo.insert("node".into(), json!(18));
        let s = format_matrix_combo(&combo);
        assert!(s.contains("node: 18"));
        assert!(s.contains("os: ubuntu"));
    }
}
