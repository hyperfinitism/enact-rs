// SPDX-License-Identifier: Apache-2.0

use super::step::run_step;
use crate::context::env_files::{parse_github_file, parse_github_path};
use crate::context::github::{build_github_context, build_github_env};
use crate::context::runner_ctx::build_runner_context;
use crate::context::types::{ExpressionContext, JobStatus};
use crate::error::Error;
use crate::expression::evaluator::evaluate_condition;
use crate::workflow::model::Job;
use log::{error, info, warn};
use std::collections::HashMap;
use std::path::Path;

/// Configuration for running a job.
pub struct JobConfig<'a> {
    pub job_id: &'a str,
    pub job: &'a Job,
    pub event_name: &'a str,
    pub workspace: &'a Path,
    pub repository: &'a str,
    pub sha: &'a str,
    pub git_ref: &'a str,
    pub extra_env: &'a HashMap<String, String>,
    pub secrets: &'a HashMap<String, String>,
    pub workflow_env: &'a HashMap<String, String>,
    pub runner_temp: &'a Path,
    pub actions_cache: &'a Path,
    pub event_payload: &'a serde_json::Value,
    pub matrix_values: &'a serde_json::Value,
    pub default_shell: &'a str,
    pub needs_ctx: &'a serde_json::Value,
}

/// Result of running a job.
pub struct JobResult {
    pub success: bool,
    pub outputs: HashMap<String, String>,
}

/// Run a single job.
pub fn run_job(config: JobConfig<'_>) -> Result<JobResult, Error> {
    let job_name = config.job.name.as_deref().unwrap_or(config.job_id);
    info!("Job: {job_name} ({})", config.job_id);

    // Evaluate job-level `if` condition
    let expr_ctx = build_job_expr_ctx(&config);
    if let Some(condition) = &config.job.condition {
        let should_run = evaluate_condition(condition, &expr_ctx, config.workspace)?;
        if !should_run {
            info!("  Skipped (condition: {condition})");
            return Ok(JobResult {
                success: true,
                outputs: HashMap::new(),
            });
        }
    }

    // Initialize GITHUB_* files
    init_github_files(config.runner_temp)?;

    // Write event.json
    let event_path = config.runner_temp.join("event.json");
    let _ = std::fs::write(
        &event_path,
        serde_json::to_string_pretty(config.event_payload).unwrap_or_default(),
    );

    // Build environment variables
    let mut env_vars = build_github_env(
        config.event_name,
        config.workspace,
        config.repository,
        config.sha,
        config.git_ref,
        config.job_id,
        config.runner_temp,
    );

    // Merge workflow env → job env → extra env
    for (k, v) in config.workflow_env {
        env_vars.insert(k.clone(), v.clone());
    }
    if let Some(job_env) = &config.job.env {
        for (k, v) in job_env {
            env_vars.insert(k.clone(), v.clone());
        }
    }
    for (k, v) in config.extra_env {
        env_vars.insert(k.clone(), v.clone());
    }

    // Container config env
    if let Some(container) = &config.job.container
        && let crate::workflow::model::Container::Config(c) = container
        && let Some(container_env) = &c.env
    {
        for (k, v) in container_env {
            env_vars.insert(k.clone(), v.clone());
        }
    }

    let steps = config
        .job
        .steps
        .as_ref()
        .ok_or_else(|| Error::Validation(format!("job '{}' has no steps", config.job_id)))?;

    let secrets_list: Vec<String> = config.secrets.values().cloned().collect();

    // Build mutable expression context
    let mut expr_ctx = build_job_expr_ctx(&config);
    let mut live_env = env_vars.clone();
    let mut all_outputs = HashMap::new();
    let mut job_success = true;

    for (i, step) in steps.iter().enumerate() {
        let result = run_step(
            step,
            i,
            config.job_id,
            config.default_shell,
            &expr_ctx,
            &live_env,
            config.workspace,
            config.actions_cache,
            config.runner_temp,
            &secrets_list,
        );

        let continue_on_error = step.continue_on_error.unwrap_or(false);

        match result {
            Ok(step_result) => {
                // Record step outputs in expression context.
                // For continue-on-error steps, outcome reflects the actual result
                // but conclusion is always "success" (GitHub Actions semantics).
                if let Some(step_id) = &step.id {
                    let outcome = if step_result.success {
                        "success"
                    } else {
                        "failure"
                    };
                    let conclusion = if continue_on_error {
                        "success"
                    } else {
                        outcome
                    };
                    let step_data = serde_json::json!({
                        "outputs": step_result.outputs,
                        "outcome": outcome,
                        "conclusion": conclusion,
                    });
                    if let serde_json::Value::Object(ref mut map) = expr_ctx.steps {
                        map.insert(step_id.clone(), step_data);
                    }
                }
                all_outputs.extend(step_result.outputs);

                // continue-on-error steps do not fail the job
                if !step_result.success && !continue_on_error {
                    job_success = false;
                    expr_ctx.job_status = JobStatus::Failure;
                }

                // Process GITHUB_ENV updates
                let env_file = config.runner_temp.join("github_env");
                let new_vars = parse_github_file(&env_file);
                for (k, v) in &new_vars {
                    expr_ctx.env.insert(k.clone(), v.clone());
                    live_env.insert(k.clone(), v.clone());
                }
                let _ = std::fs::write(&env_file, "");

                // Process GITHUB_PATH updates
                let path_file = config.runner_temp.join("github_path");
                let new_paths = parse_github_path(&path_file);
                if !new_paths.is_empty() {
                    let current_path = live_env
                        .get("PATH")
                        .cloned()
                        .unwrap_or_else(|| "/usr/local/bin:/usr/bin:/bin".into());
                    let added = new_paths.join(":");
                    let updated = format!("{added}:{current_path}");
                    live_env.insert("PATH".into(), updated.clone());
                    expr_ctx.env.insert("PATH".into(), updated);
                    info!("  PATH updated: +{added}");
                }
                let _ = std::fs::write(&path_file, "");
            }
            Err(e) => {
                if continue_on_error {
                    warn!("  Step failed but continue-on-error: {e}");
                    // continue-on-error: job still succeeds
                } else {
                    error!("  Job {} failed: {e}", config.job_id);
                    return Ok(JobResult {
                        success: false,
                        outputs: all_outputs,
                    });
                }
            }
        }
    }

    if job_success {
        info!("  Job {} \x1b[32msucceeded\x1b[0m", config.job_id);
    } else {
        error!("  Job {} \x1b[31mfailed\x1b[0m", config.job_id);
    }

    Ok(JobResult {
        success: job_success,
        outputs: all_outputs,
    })
}

fn build_job_expr_ctx(config: &JobConfig<'_>) -> ExpressionContext {
    let mut ctx = ExpressionContext {
        github: build_github_context(
            config.event_name,
            config.workspace,
            config.repository,
            config.sha,
            config.git_ref,
            config.job_id,
            config.event_payload,
        ),
        runner: build_runner_context(config.runner_temp),
        matrix: config.matrix_values.clone(),
        needs: config.needs_ctx.clone(),
        strategy: serde_json::json!({
            "fail-fast": true,
            "job-index": 0,
            "job-total": 1,
            "max-parallel": 1,
        }),
        ..Default::default()
    };
    // Seed env context: workflow env → job env → extra env (same merge order as live env)
    for (k, v) in config.workflow_env {
        ctx.env.insert(k.clone(), v.clone());
    }
    if let Some(job_env) = &config.job.env {
        for (k, v) in job_env {
            ctx.env.insert(k.clone(), v.clone());
        }
    }
    for (k, v) in config.extra_env {
        ctx.env.insert(k.clone(), v.clone());
    }
    for (k, v) in config.secrets {
        ctx.secrets.insert(k.clone(), v.clone());
    }
    ctx
}

fn init_github_files(runner_temp: &Path) -> Result<(), Error> {
    std::fs::create_dir_all(runner_temp)?;
    for name in [
        "github_output",
        "github_env",
        "github_path",
        "github_state",
        "github_step_summary",
    ] {
        let path = runner_temp.join(name);
        if !path.exists() {
            std::fs::write(&path, "")?;
        }
    }
    Ok(())
}
