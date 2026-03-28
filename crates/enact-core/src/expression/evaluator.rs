// SPDX-License-Identifier: Apache-2.0

use super::ast::{BinaryOp, Expr};
use super::functions::call_function;
use super::lexer::Lexer;
use super::parser::Parser;
use super::value::Value;
use crate::context::types::{ExpressionContext, JobStatus};
use crate::error::Error;
use std::path::Path;

/// Evaluate a GitHub Actions expression string (may contain `${{ }}` interpolation).
pub fn evaluate_expression(
    input: &str,
    ctx: &ExpressionContext,
    workspace: &Path,
) -> Result<String, Error> {
    if !input.contains("${{") {
        // No expression interpolation needed — return the string as-is.
        return Ok(input.to_string());
    }
    let mut result = input.to_string();
    while let Some(start) = result.find("${{") {
        let end = result[start..]
            .find("}}")
            .ok_or_else(|| Error::ExpressionEval(format!("unclosed expression in: {input}")))?
            + start
            + 2;
        let expr_str = result[start + 3..end - 2].trim();
        let value = eval_expr_string(expr_str, ctx, workspace)?;
        result = format!("{}{}{}", &result[..start], value.to_str(), &result[end..]);
    }
    Ok(result)
}

/// Evaluate a condition (for `if` fields). Returns true if the condition passes.
///
/// Unlike `evaluate_expression`, bare strings without `${{ }}` are treated as
/// expressions (matching GitHub Actions behaviour for `if:` conditions).
pub fn evaluate_condition(
    condition: &str,
    ctx: &ExpressionContext,
    workspace: &Path,
) -> Result<bool, Error> {
    let condition = condition.trim();
    if condition.contains("${{") {
        // Has explicit interpolation markers — evaluate them first.
        let value_str = evaluate_expression(condition, ctx, workspace)?;
        Ok(is_truthy_str(&value_str))
    } else {
        // Bare expression (GitHub Actions `if:` conditions are implicitly
        // wrapped in `${{ }}` when the markers are absent).
        let value = eval_expr_string(condition, ctx, workspace)?;
        Ok(value.is_truthy())
    }
}

fn is_truthy_str(s: &str) -> bool {
    let v = s.trim().to_lowercase();
    !v.is_empty() && v != "false" && v != "0" && v != "null" && v != "undefined"
}

/// Parse and evaluate a single expression string to a Value.
fn eval_expr_string(expr: &str, ctx: &ExpressionContext, workspace: &Path) -> Result<Value, Error> {
    if expr.is_empty() {
        return Ok(Value::String(String::new()));
    }
    let tokens = Lexer::new(expr).tokenize()?;
    let ast = Parser::new(tokens).parse()?;
    eval_ast(&ast, ctx, workspace)
}

/// Recursively evaluate an AST node.
fn eval_ast(expr: &Expr, ctx: &ExpressionContext, workspace: &Path) -> Result<Value, Error> {
    match expr {
        Expr::Literal(v) => Ok(v.clone()),

        Expr::Ident(name) => resolve_ident(name, ctx),

        Expr::Property(base, prop) => {
            let base_val = eval_ast(base, ctx, workspace)?;
            Ok(get_property(&base_val, prop))
        }

        Expr::Index(base, index) => {
            let base_val = eval_ast(base, ctx, workspace)?;
            let index_val = eval_ast(index, ctx, workspace)?;
            let key = index_val.to_str();
            Ok(get_property(&base_val, &key))
        }

        Expr::Wildcard(base) => {
            let base_val = eval_ast(base, ctx, workspace)?;
            match base_val {
                Value::Array(arr) => Ok(Value::Array(arr)),
                Value::Object(map) => Ok(Value::Array(map.into_values().collect())),
                _ => Ok(Value::Array(vec![])),
            }
        }

        Expr::FunctionCall(name, args) => {
            // Status functions
            match name.as_str() {
                "success" => {
                    return Ok(Value::Bool(ctx.job_status == JobStatus::Success));
                }
                "failure" => {
                    return Ok(Value::Bool(ctx.job_status == JobStatus::Failure));
                }
                "always" => return Ok(Value::Bool(true)),
                "cancelled" => {
                    return Ok(Value::Bool(ctx.job_status == JobStatus::Cancelled));
                }
                _ => {}
            }
            let evaluated_args: Vec<Value> = args
                .iter()
                .map(|a| eval_ast(a, ctx, workspace))
                .collect::<Result<_, _>>()?;
            call_function(name, &evaluated_args, workspace)
        }

        Expr::Not(inner) => {
            let val = eval_ast(inner, ctx, workspace)?;
            Ok(Value::Bool(!val.is_truthy()))
        }

        Expr::BinaryOp(op, left, right) => eval_binary_op(*op, left, right, ctx, workspace),
    }
}

fn eval_binary_op(
    op: BinaryOp,
    left: &Expr,
    right: &Expr,
    ctx: &ExpressionContext,
    workspace: &Path,
) -> Result<Value, Error> {
    match op {
        // || returns the value, not just boolean (short-circuit)
        BinaryOp::Or => {
            let l = eval_ast(left, ctx, workspace)?;
            if l.is_truthy() {
                Ok(l)
            } else {
                eval_ast(right, ctx, workspace)
            }
        }
        // && returns the value, not just boolean (short-circuit)
        BinaryOp::And => {
            let l = eval_ast(left, ctx, workspace)?;
            if !l.is_truthy() {
                Ok(l)
            } else {
                eval_ast(right, ctx, workspace)
            }
        }
        BinaryOp::Eq => {
            let l = eval_ast(left, ctx, workspace)?;
            let r = eval_ast(right, ctx, workspace)?;
            Ok(Value::Bool(l.loose_eq(&r)))
        }
        BinaryOp::Neq => {
            let l = eval_ast(left, ctx, workspace)?;
            let r = eval_ast(right, ctx, workspace)?;
            Ok(Value::Bool(!l.loose_eq(&r)))
        }
        BinaryOp::Lt | BinaryOp::Gt | BinaryOp::Lte | BinaryOp::Gte => {
            let l = eval_ast(left, ctx, workspace)?;
            let r = eval_ast(right, ctx, workspace)?;
            let ln = l.to_number();
            let rn = r.to_number();
            if ln.is_nan() || rn.is_nan() {
                return Ok(Value::Bool(false));
            }
            let result = match op {
                BinaryOp::Lt => ln < rn,
                BinaryOp::Gt => ln > rn,
                BinaryOp::Lte => ln <= rn,
                BinaryOp::Gte => ln >= rn,
                _ => unreachable!(),
            };
            Ok(Value::Bool(result))
        }
    }
}

/// Resolve a top-level identifier to a context object.
fn resolve_ident(name: &str, ctx: &ExpressionContext) -> Result<Value, Error> {
    match name {
        "github" => Ok(Value::from_json(&ctx.github)),
        "env" => Ok(hash_to_value(&ctx.env)),
        "secrets" => Ok(hash_to_value(&ctx.secrets)),
        "vars" => Ok(hash_to_value(&ctx.vars)),
        "runner" => Ok(hash_to_value(&ctx.runner)),
        "inputs" => Ok(hash_to_value(&ctx.inputs)),
        "matrix" => Ok(Value::from_json(&ctx.matrix)),
        "strategy" => Ok(Value::from_json(&ctx.strategy)),
        "needs" => Ok(Value::from_json(&ctx.needs)),
        "job" => Ok(Value::from_json(&ctx.job)),
        "steps" => Ok(Value::from_json(&ctx.steps)),
        // Unknown context — return empty string (GitHub Actions behavior)
        _ => Ok(Value::String(String::new())),
    }
}

fn hash_to_value(map: &std::collections::HashMap<String, String>) -> Value {
    let mut obj = std::collections::BTreeMap::new();
    for (k, v) in map {
        obj.insert(k.clone(), Value::String(v.clone()));
    }
    Value::Object(obj)
}

fn get_property(value: &Value, key: &str) -> Value {
    match value {
        Value::Object(map) => map.get(key).cloned().unwrap_or(Value::Null),
        Value::Array(arr) => {
            // Numeric index
            if let Ok(idx) = key.parse::<usize>() {
                arr.get(idx).cloned().unwrap_or(Value::Null)
            } else {
                // Property filter: collect the property from each array element
                let filtered: Vec<Value> = arr
                    .iter()
                    .filter_map(|item| {
                        let prop = get_property(item, key);
                        if matches!(prop, Value::Null) {
                            None
                        } else {
                            Some(prop)
                        }
                    })
                    .collect();
                if filtered.is_empty() {
                    Value::Null
                } else {
                    Value::Array(filtered)
                }
            }
        }
        _ => Value::Null,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::types::ExpressionContext;
    use std::path::PathBuf;

    fn ws() -> PathBuf {
        PathBuf::from("/tmp")
    }

    fn make_ctx() -> ExpressionContext {
        let mut ctx = ExpressionContext {
            github: serde_json::json!({
                "ref": "refs/heads/main",
                "event_name": "push",
                "event": {
                    "pull_request": {
                        "head": { "ref": "feature-branch" },
                        "labels": [
                            { "name": "bug" },
                            { "name": "enhancement" }
                        ]
                    }
                },
                "repository": "owner/repo"
            }),
            ..Default::default()
        };
        ctx.env.insert("CI".into(), "true".into());
        ctx.secrets.insert("MY_TOKEN".into(), "abc123".into());
        ctx.matrix = serde_json::json!({"os": "ubuntu-latest"});
        ctx.steps = serde_json::json!({
            "build": {
                "outputs": { "result": "ok" },
                "outcome": "success"
            }
        });
        ctx.runner.insert("os".into(), "Linux".into());
        ctx
    }

    #[test]
    fn test_simple_context_access() {
        let ctx = make_ctx();
        let result = evaluate_expression("${{ github.ref }}", &ctx, &ws()).unwrap();
        assert_eq!(result, "refs/heads/main");
    }

    #[test]
    fn test_deep_context_access() {
        let ctx = make_ctx();
        let result =
            evaluate_expression("${{ github.event.pull_request.head.ref }}", &ctx, &ws()).unwrap();
        assert_eq!(result, "feature-branch");
    }

    #[test]
    fn test_env_context() {
        let ctx = make_ctx();
        let result = evaluate_expression("${{ env.CI }}", &ctx, &ws()).unwrap();
        assert_eq!(result, "true");
    }

    #[test]
    fn test_step_outputs() {
        let ctx = make_ctx();
        let result = evaluate_expression("${{ steps.build.outputs.result }}", &ctx, &ws()).unwrap();
        assert_eq!(result, "ok");
    }

    #[test]
    fn test_comparison() {
        let ctx = make_ctx();
        let result =
            evaluate_expression("${{ github.event_name == 'push' }}", &ctx, &ws()).unwrap();
        assert_eq!(result, "true");
    }

    #[test]
    fn test_logical_and() {
        let ctx = make_ctx();
        let result = evaluate_expression(
            "${{ github.ref == 'refs/heads/main' && env.CI == 'true' }}",
            &ctx,
            &ws(),
        )
        .unwrap();
        assert_eq!(result, "true");
    }

    #[test]
    fn test_negation() {
        let ctx = make_ctx();
        let result = evaluate_expression("${{ !failure() }}", &ctx, &ws()).unwrap();
        assert_eq!(result, "true");
    }

    #[test]
    fn test_string_interpolation() {
        let ctx = make_ctx();
        let result =
            evaluate_expression("Hello ${{ github.event_name }} world", &ctx, &ws()).unwrap();
        assert_eq!(result, "Hello push world");
    }

    #[test]
    fn test_condition_evaluation() {
        let ctx = make_ctx();
        assert!(evaluate_condition("success()", &ctx, &ws()).unwrap());
        assert!(!evaluate_condition("failure()", &ctx, &ws()).unwrap());
        assert!(evaluate_condition("github.event_name == 'push'", &ctx, &ws()).unwrap());
    }

    #[test]
    fn test_contains_function() {
        let ctx = make_ctx();
        let result =
            evaluate_expression("${{ contains(github.repository, 'owner') }}", &ctx, &ws())
                .unwrap();
        assert_eq!(result, "true");
    }

    #[test]
    fn test_starts_with_function() {
        let ctx = make_ctx();
        let result =
            evaluate_expression("${{ startsWith(github.ref, 'refs/heads/') }}", &ctx, &ws())
                .unwrap();
        assert_eq!(result, "true");
    }

    #[test]
    fn test_or_returns_value() {
        let ctx = make_ctx();
        // || should return the truthy value, not just "true"
        let result = evaluate_expression("${{ matrix.os || 'default' }}", &ctx, &ws()).unwrap();
        assert_eq!(result, "ubuntu-latest");
    }

    #[test]
    fn test_or_fallback() {
        let ctx = make_ctx();
        let result =
            evaluate_expression("${{ matrix.nonexistent || 'fallback' }}", &ctx, &ws()).unwrap();
        assert_eq!(result, "fallback");
    }

    #[test]
    fn test_format_function() {
        let ctx = make_ctx();
        let result =
            evaluate_expression("${{ format('Hello {0}!', 'world') }}", &ctx, &ws()).unwrap();
        assert_eq!(result, "Hello world!");
    }

    #[test]
    fn test_relational_comparison() {
        let ctx = make_ctx();
        let result = evaluate_expression("${{ 1 < 2 }}", &ctx, &ws()).unwrap();
        assert_eq!(result, "true");
        let result = evaluate_expression("${{ 3 >= 3 }}", &ctx, &ws()).unwrap();
        assert_eq!(result, "true");
    }

    #[test]
    fn test_undefined_returns_empty() {
        let ctx = make_ctx();
        let result = evaluate_expression("${{ github.nonexistent_field }}", &ctx, &ws()).unwrap();
        assert_eq!(result, "");
    }

    #[test]
    fn test_always() {
        let ctx = make_ctx();
        assert!(evaluate_condition("always()", &ctx, &ws()).unwrap());
    }
}
