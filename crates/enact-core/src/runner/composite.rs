// SPDX-License-Identifier: Apache-2.0

use super::action_resolver::ActionYml;
use super::shell::exec_shell;
use crate::context::env_files::{parse_github_file, parse_github_path};
use crate::context::types::ExpressionContext;
use crate::error::Error;
use crate::expression::evaluator::{evaluate_condition, evaluate_expression};
use crate::security::path_sanitizer::safe_resolve_within;
use log::{debug, info, warn};
use std::collections::HashMap;
use std::path::Path;

/// Result of running a composite action.
pub struct CompositeResult {
    pub outputs: HashMap<String, String>,
    pub success: bool,
}

/// Execute a composite action's steps.
#[allow(clippy::too_many_arguments)]
pub fn run_composite_action(
    action: &ActionYml,
    action_dir: &Path,
    step_env: &HashMap<String, String>,
    inputs: &HashMap<String, String>,
    expr_ctx: &ExpressionContext,
    workspace: &Path,
    runner_temp: &Path,
    secrets: &[String],
) -> Result<CompositeResult, Error> {
    let steps = match &action.runs.steps {
        Some(s) => s,
        None => {
            warn!("    Composite action has no steps");
            return Ok(CompositeResult {
                outputs: HashMap::new(),
                success: true,
            });
        }
    };

    // Build env with default input values
    let mut env = step_env.clone();
    if let Some(action_inputs) = &action.inputs {
        for (key, input_def) in action_inputs {
            let env_key = format!("INPUT_{}", key.to_uppercase().replace('-', "_"));
            if !inputs.contains_key(&env_key)
                && let Some(default) = &input_def.default
            {
                env.insert(env_key, default.clone());
            }
        }
    }
    for (k, v) in inputs {
        env.insert(k.clone(), v.clone());
    }

    // Build inputs context for expressions
    let mut composite_ctx = expr_ctx.clone();
    for (k, v) in &env {
        if let Some(input_name) = k.strip_prefix("INPUT_") {
            composite_ctx
                .inputs
                .insert(input_name.to_lowercase().replace('_', "-"), v.clone());
        }
    }

    let mut outputs = HashMap::new();

    for (i, step) in steps.iter().enumerate() {
        let default_name = format!("composite step {i}");
        let step_name_resolved;
        let step_name = if let Some(name) = &step.name {
            step_name_resolved = evaluate_expression(name, &composite_ctx, workspace)
                .unwrap_or_else(|_| name.clone());
            &step_name_resolved
        } else {
            &default_name
        };

        // Evaluate condition
        if let Some(cond) = &step.condition {
            match evaluate_condition(cond, &composite_ctx, workspace) {
                Ok(true) => {}
                Ok(false) => {
                    debug!("    Skipping: {step_name} (condition: {cond})");
                    continue;
                }
                Err(e) => {
                    debug!("    Could not evaluate condition '{cond}': {e}, running anyway");
                }
            }
        }

        // Merge step-level env (with expression evaluation)
        let mut step_exec_env = env.clone();
        if let Some(step_env_map) = &step.env {
            for (k, v) in step_env_map {
                let resolved_v =
                    evaluate_expression(v, &composite_ctx, workspace).unwrap_or_else(|_| v.clone());
                step_exec_env.insert(k.clone(), resolved_v);
            }
        }

        if let Some(run_cmd) = &step.run {
            let shell = step.shell.as_deref().unwrap_or("bash");
            let resolved = evaluate_expression(run_cmd, &composite_ctx, workspace)
                .unwrap_or_else(|_| run_cmd.clone());

            let cwd = if let Some(d) = &step.working_directory {
                let resolved_d =
                    evaluate_expression(d, &composite_ctx, workspace).unwrap_or_else(|_| d.clone());
                safe_resolve_within(
                    workspace,
                    &resolved_d,
                    &[workspace, runner_temp, action_dir],
                )?
            } else {
                workspace.to_path_buf()
            };

            debug!("    Composite step {i}: {step_name} (shell: {shell})");

            let result = exec_shell(shell, &resolved, &step_exec_env, &cwd, 1000 + i, secrets)?;

            // Process GITHUB_ENV updates (use runner-controlled path)
            {
                let env_file = runner_temp.join("github_env");
                let new_vars = parse_github_file(&env_file);
                for (k, v) in &new_vars {
                    composite_ctx.env.insert(k.clone(), v.clone());
                    env.insert(k.clone(), v.clone());
                }
                let _ = std::fs::write(&env_file, "");
            }

            // Process GITHUB_PATH updates (use runner-controlled path)
            {
                let path_file = runner_temp.join("github_path");
                let new_paths = parse_github_path(&path_file);
                if !new_paths.is_empty() {
                    let current_path = env
                        .get("PATH")
                        .cloned()
                        .unwrap_or_else(|| "/usr/local/bin:/usr/bin:/bin".into());
                    let added = new_paths.join(":");
                    let updated = format!("{added}:{current_path}");
                    env.insert("PATH".into(), updated.clone());
                    composite_ctx.env.insert("PATH".into(), updated);
                    info!("    PATH updated: +{added}");
                }
                let _ = std::fs::write(&path_file, "");
            }

            // Process GITHUB_OUTPUT updates (use runner-controlled path)
            {
                let output_file = runner_temp.join("github_output");
                let step_outputs = parse_github_file(&output_file);
                if let Some(step_id) = &step.id {
                    let step_data = serde_json::json!({
                        "outputs": &step_outputs,
                        "outcome": if result.exit_code == 0 { "success" } else { "failure" },
                        "conclusion": if result.exit_code == 0 { "success" } else { "failure" },
                    });
                    if let serde_json::Value::Object(ref mut map) = composite_ctx.steps {
                        map.insert(step_id.clone(), step_data);
                    }
                }
                outputs.extend(step_outputs);
                let _ = std::fs::write(&output_file, "");
            }

            if result.exit_code != 0 {
                warn!(
                    "    Composite step '{step_name}' failed (exit code {})",
                    result.exit_code
                );
                return Err(Error::StepFailed {
                    job: "composite".into(),
                    step: step_name.into(),
                    exit_code: result.exit_code,
                });
            }
        } else if let Some(uses) = &step.uses {
            debug!("    Composite step {i} uses: {uses} (sub-action — skipping)");
        }
    }

    Ok(CompositeResult {
        outputs,
        success: true,
    })
}
