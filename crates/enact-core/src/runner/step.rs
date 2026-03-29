// SPDX-License-Identifier: Apache-2.0

use super::action_resolver::{self, ActionRef};
use super::composite;
use super::node_action;
use super::shell::exec_shell;
use crate::builtin;
use crate::context::env_files::parse_github_file;
use crate::context::types::ExpressionContext;
use crate::error::Error;
use crate::expression::evaluator::{evaluate_condition, evaluate_expression};
use crate::security::path_sanitizer::safe_resolve;
use crate::workflow::model::Step;
use log::{debug, error, info, warn};
use std::collections::HashMap;
use std::path::Path;

/// Result of running a single step.
pub struct StepResult {
    pub success: bool,
    pub outputs: HashMap<String, String>,
}

#[allow(clippy::too_many_arguments)]
/// Execute a single step.
pub fn run_step(
    step: &Step,
    step_index: usize,
    job_id: &str,
    default_shell: &str,
    expr_ctx: &ExpressionContext,
    live_env: &HashMap<String, String>,
    workspace: &Path,
    actions_cache: &Path,
    runner_temp: &Path,
    secrets: &[String],
) -> Result<StepResult, Error> {
    let step_name = step.display_name();
    info!("  Step {step_index}: {step_name}");

    // Evaluate `if` condition
    if let Some(condition) = &step.condition {
        let should_run = evaluate_condition(condition, expr_ctx, workspace)?;
        if !should_run {
            info!("    Skipped (condition: {condition})");
            return Ok(StepResult {
                success: true,
                outputs: HashMap::new(),
            });
        }
    }

    // Merge environment: live env < step env (with expression resolution)
    let mut step_env = live_env.clone();
    if let Some(env) = &step.env {
        for (k, v) in env {
            let resolved = evaluate_expression(v, expr_ctx, workspace)?;
            step_env.insert(k.clone(), resolved);
        }
    }

    if let Some(run_script) = &step.run {
        run_run_step(
            run_script,
            step,
            step_index,
            job_id,
            default_shell,
            &step_env,
            expr_ctx,
            workspace,
            runner_temp,
            secrets,
        )
    } else if let Some(uses) = &step.uses {
        run_uses_step(
            uses,
            step,
            step_index,
            job_id,
            &step_env,
            expr_ctx,
            workspace,
            actions_cache,
            runner_temp,
            secrets,
        )
    } else {
        Err(Error::Validation(
            "step has neither 'run' nor 'uses'".into(),
        ))
    }
}

#[allow(clippy::too_many_arguments)]
fn run_run_step(
    script: &str,
    step: &Step,
    step_index: usize,
    job_id: &str,
    default_shell: &str,
    step_env: &HashMap<String, String>,
    expr_ctx: &ExpressionContext,
    workspace: &Path,
    runner_temp: &Path,
    secrets: &[String],
) -> Result<StepResult, Error> {
    let shell = step.shell.as_deref().unwrap_or(default_shell);

    // Resolve expressions in the script
    let resolved_script = evaluate_expression(script, expr_ctx, workspace)?;

    let cwd = if let Some(d) = &step.working_directory {
        safe_resolve(workspace, d)?
    } else {
        workspace.to_path_buf()
    };

    debug!("    Shell: {shell}");

    let result = exec_shell(shell, &resolved_script, step_env, &cwd, step_index, secrets)?;

    // Read GITHUB_OUTPUT
    let outputs = read_step_outputs(runner_temp)?;

    if result.exit_code != 0 {
        let step_name = step.display_name();
        if step.continue_on_error.unwrap_or(false) {
            warn!(
                "    Step failed (exit code {}) but continue-on-error is set",
                result.exit_code
            );
            return Ok(StepResult {
                success: false,
                outputs,
            });
        }
        error!("    Step failed with exit code {}", result.exit_code);
        return Err(Error::StepFailed {
            job: job_id.into(),
            step: step_name,
            exit_code: result.exit_code,
        });
    }

    info!("    Success");
    Ok(StepResult {
        success: true,
        outputs,
    })
}

#[allow(clippy::too_many_arguments)]
fn run_uses_step(
    uses: &str,
    step: &Step,
    _step_index: usize,
    _job_id: &str,
    step_env: &HashMap<String, String>,
    expr_ctx: &ExpressionContext,
    workspace: &Path,
    actions_cache: &Path,
    runner_temp: &Path,
    secrets: &[String],
) -> Result<StepResult, Error> {
    // Build INPUT_* env vars from `with`
    let mut inputs = step_env.clone();
    if let Some(with_values) = &step.with {
        for (key, value) in with_values {
            let env_key = format!("INPUT_{}", key.to_uppercase().replace('-', "_"));
            let str_value = match value {
                serde_json::Value::String(s) => evaluate_expression(s, expr_ctx, workspace)?,
                other => other.to_string(),
            };
            inputs.insert(env_key, str_value);
        }
    }

    // Check for built-in action emulation
    if let Some(result) =
        builtin::try_builtin_action(uses, &inputs, workspace, runner_temp, actions_cache)
    {
        return match result {
            Ok(outputs) => {
                info!("    Success (built-in)");
                Ok(StepResult {
                    success: true,
                    outputs,
                })
            }
            Err(e) => {
                if step.continue_on_error.unwrap_or(false) {
                    warn!("    Built-in action failed but continue-on-error: {e}");
                    Ok(StepResult {
                        success: false,
                        outputs: HashMap::new(),
                    })
                } else {
                    Err(e)
                }
            }
        };
    }

    // Resolve and execute the action
    let action_ref = action_resolver::resolve_action(uses, workspace, actions_cache)?;
    let action_dir = match &action_ref {
        ActionRef::Local(dir) => dir.clone(),
        ActionRef::Remote { dir, .. } => dir.clone(),
    };

    info!("    Action: {uses}");

    // Set GITHUB_ACTION_PATH so scripts inside the action can reference
    // sibling files via $GITHUB_ACTION_PATH (e.g. enarx/spdx's verify-spdx-headers).
    let action_path_str = action_dir.to_string_lossy().to_string();
    let mut step_env = step_env.clone();
    step_env.insert("GITHUB_ACTION_PATH".into(), action_path_str.clone());
    inputs.insert("GITHUB_ACTION_PATH".into(), action_path_str.clone());

    let mut expr_ctx = expr_ctx.clone();
    if let serde_json::Value::Object(ref mut gh) = expr_ctx.github {
        gh.insert(
            "action_path".into(),
            serde_json::Value::String(action_path_str.clone()),
        );
    }

    // Parse action.yml
    let action_yml = action_resolver::read_action_yml(&action_dir);
    let Some(action_yml) = action_yml else {
        warn!("    No action.yml found in {}", action_dir.display());
        if step.continue_on_error.unwrap_or(false) {
            return Ok(StepResult {
                success: false,
                outputs: HashMap::new(),
            });
        }
        return Err(Error::Action {
            action: uses.into(),
            message: "no action.yml/action.yaml found".into(),
        });
    };

    let using = &action_yml.runs.using;
    debug!("    Action type: {using}");

    if using == "composite" {
        let result = composite::run_composite_action(
            &action_yml,
            &action_dir,
            &step_env,
            &inputs,
            &expr_ctx,
            workspace,
            runner_temp,
            secrets,
        )?;
        Ok(StepResult {
            success: result.success,
            outputs: result.outputs,
        })
    } else if using.starts_with("node") {
        if let Some(main) = &action_yml.runs.main {
            let outputs = node_action::run_node_action(
                &action_dir,
                main,
                &inputs,
                workspace,
                runner_temp,
                secrets,
            )?;
            Ok(StepResult {
                success: true,
                outputs,
            })
        } else {
            warn!("    Node.js action has no 'main' entry point");
            Ok(StepResult {
                success: false,
                outputs: HashMap::new(),
            })
        }
    } else {
        warn!("    Unsupported action type: {using}");
        if step.continue_on_error.unwrap_or(false) {
            Ok(StepResult {
                success: false,
                outputs: HashMap::new(),
            })
        } else {
            Err(Error::Action {
                action: uses.into(),
                message: format!("unsupported action type: {using}"),
            })
        }
    }
}

fn read_step_outputs(runner_temp: &Path) -> Result<HashMap<String, String>, Error> {
    let output_path = runner_temp.join("github_output");
    let outputs = parse_github_file(&output_path);
    // Truncate the file for the next step
    let _ = std::fs::write(&output_path, "");
    Ok(outputs)
}
