// SPDX-License-Identifier: Apache-2.0

use super::job::{self, JobConfig, JobResult};
use crate::context::event::generate_event_json;
use crate::context::github::detect_git_info;
use crate::error::Error;
use crate::workflow::matrix::{expand_matrix, format_matrix_combo};
use crate::workflow::model::Workflow;
use log::{error, info, warn};
use std::collections::{HashMap, HashSet};
use std::path::Path;

/// Top-level configuration for the engine.
pub struct EngineConfig<'a> {
    pub event_name: &'a str,
    pub workspace: &'a Path,
    pub extra_env: &'a HashMap<String, String>,
    pub secrets: &'a HashMap<String, String>,
    pub target_job: Option<&'a str>,
    pub event_file: Option<&'a Path>,
    pub runner_temp: &'a Path,
    pub actions_cache: &'a Path,
    pub default_shell: &'a str,
}

/// Execute a workflow.
pub fn run_workflow(
    workflow: &Workflow,
    workflow_name: &str,
    config: &EngineConfig<'_>,
) -> Result<bool, Error> {
    info!("=== Workflow: {workflow_name} ===");

    let (repository, sha, git_ref) = detect_git_info(config.workspace);
    let event_payload = generate_event_json(
        config.event_name,
        &repository,
        &sha,
        &git_ref,
        config.event_file,
    );

    let job_order = topological_sort(&workflow.jobs, config.target_job)?;

    info!(
        "Execution order: {}",
        job_order
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join(" → ")
    );

    let workflow_env = workflow.env.clone().unwrap_or_default();
    let default_shell = workflow
        .defaults
        .as_ref()
        .and_then(|d| d.run.as_ref())
        .and_then(|r| r.shell.as_deref())
        .unwrap_or(config.default_shell);

    let mut all_success = true;
    let mut completed_jobs: HashMap<String, JobResult> = HashMap::new();
    let mut needs_ctx = serde_json::json!({});

    for job_id in &job_order {
        let job = &workflow.jobs[job_id.as_str()];

        // Check dependencies — if any dependency failed, skip this job entirely
        let mut deps_ok = true;
        if let Some(needs) = &job.needs {
            for dep in needs.as_vec() {
                if let Some(dep_result) = completed_jobs.get(dep)
                    && !dep_result.success
                {
                    info!("Skipping job '{job_id}': dependency '{dep}' failed");
                    deps_ok = false;
                    break;
                }
            }
        }
        if !deps_ok {
            completed_jobs.insert(
                job_id.clone(),
                JobResult {
                    success: false,
                    outputs: HashMap::new(),
                },
            );
            all_success = false;
            continue;
        }

        // Expand matrix
        let matrix_combos = if let Some(strategy) = &job.strategy {
            if let Some(matrix) = &strategy.matrix {
                expand_matrix(matrix)
            } else {
                vec![HashMap::new()]
            }
        } else {
            vec![HashMap::new()]
        };

        let mut job_outputs = HashMap::new();
        let mut job_succeeded = true;

        for combo in matrix_combos.iter() {
            let combo_suffix = format_matrix_combo(combo);
            let combo_name = format!("{job_id}{combo_suffix}");
            if matrix_combos.len() > 1 {
                info!("  Matrix run: {combo_name}");
            }

            let matrix_values = serde_json::to_value(combo).unwrap_or_default();

            let job_config = JobConfig {
                job_id,
                job,
                event_name: config.event_name,
                workspace: config.workspace,
                repository: &repository,
                sha: &sha,
                git_ref: &git_ref,
                extra_env: config.extra_env,
                secrets: config.secrets,
                workflow_env: &workflow_env,
                runner_temp: config.runner_temp,
                actions_cache: config.actions_cache,
                event_payload: &event_payload,
                matrix_values: &matrix_values,
                default_shell,
                needs_ctx: &needs_ctx,
            };

            match job::run_job(job_config) {
                Ok(result) => {
                    if !result.success {
                        job_succeeded = false;
                        // Check fail-fast
                        if job
                            .strategy
                            .as_ref()
                            .and_then(|s| s.fail_fast)
                            .unwrap_or(true)
                            && matrix_combos.len() > 1
                        {
                            warn!("  Matrix job failed with fail-fast, skipping remaining");
                            break;
                        }
                    }
                    job_outputs.extend(result.outputs);
                }
                Err(e) => {
                    error!("Job '{combo_name}' failed: {e}");
                    job_succeeded = false;
                    break;
                }
            }
        }

        if !job_succeeded {
            all_success = false;
        }

        // Record results for needs context
        let result_str = if job_succeeded { "success" } else { "failure" };
        if let serde_json::Value::Object(ref mut map) = needs_ctx {
            map.insert(
                job_id.clone(),
                serde_json::json!({
                    "result": result_str,
                    "outputs": job_outputs,
                }),
            );
        }

        completed_jobs.insert(
            job_id.clone(),
            JobResult {
                success: job_succeeded,
                outputs: job_outputs,
            },
        );
    }

    if all_success {
        info!("\x1b[32m=== Workflow '{workflow_name}' completed successfully ===\x1b[0m");
    } else {
        error!("\x1b[31m=== Workflow '{workflow_name}' failed ===\x1b[0m");
    }

    Ok(all_success)
}

/// Topologically sort jobs based on `needs` dependencies.
fn topological_sort(
    jobs: &HashMap<String, crate::workflow::model::Job>,
    target_job: Option<&str>,
) -> Result<Vec<String>, Error> {
    let job_ids: HashSet<String> = if let Some(target) = target_job {
        if !jobs.contains_key(target) {
            return Err(Error::Validation(format!(
                "target job '{target}' not found"
            )));
        }
        collect_transitive_deps(target, jobs)?
    } else {
        jobs.keys().cloned().collect()
    };

    // Kahn's algorithm
    let mut in_degree: HashMap<&str, usize> = HashMap::new();
    let mut adj: HashMap<&str, Vec<&str>> = HashMap::new();

    for job_id in &job_ids {
        in_degree.entry(job_id.as_str()).or_insert(0);
        adj.entry(job_id.as_str()).or_default();
    }

    for job_id in &job_ids {
        if let Some(job) = jobs.get(job_id.as_str())
            && let Some(needs) = &job.needs
        {
            for dep in needs.as_vec() {
                if job_ids.contains(dep) {
                    adj.entry(dep).or_default().push(job_id.as_str());
                    *in_degree.entry(job_id.as_str()).or_insert(0) += 1;
                }
            }
        }
    }

    let mut queue: Vec<&str> = in_degree
        .iter()
        .filter(|(_, deg)| **deg == 0)
        .map(|(&id, _)| id)
        .collect();
    queue.sort();

    let mut result = Vec::new();
    while let Some(node) = queue.pop() {
        result.push(node.to_string());
        if let Some(neighbors) = adj.get(node) {
            for &next in neighbors {
                if let Some(deg) = in_degree.get_mut(next) {
                    *deg -= 1;
                    if *deg == 0 {
                        queue.push(next);
                        queue.sort();
                    }
                }
            }
        }
    }

    if result.len() != job_ids.len() {
        return Err(Error::DependencyCycle(
            "circular dependency detected among jobs".into(),
        ));
    }

    Ok(result)
}

fn collect_transitive_deps(
    job_id: &str,
    jobs: &HashMap<String, crate::workflow::model::Job>,
) -> Result<HashSet<String>, Error> {
    let mut result = HashSet::new();
    let mut stack = vec![job_id.to_string()];

    while let Some(current) = stack.pop() {
        if result.contains(&current) {
            continue;
        }
        result.insert(current.clone());
        if let Some(job) = jobs.get(current.as_str())
            && let Some(needs) = &job.needs
        {
            for dep in needs.as_vec() {
                if !jobs.contains_key(dep) {
                    return Err(Error::Validation(format!(
                        "job '{current}' depends on non-existent job '{dep}'"
                    )));
                }
                stack.push(dep.to_string());
            }
        }
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::parser::parse_workflow_string;
    use std::path::PathBuf;

    fn p() -> PathBuf {
        PathBuf::from("<test>")
    }

    #[test]
    fn test_topological_sort_linear() {
        let yaml = r#"
name: Test
on: push
jobs:
  a:
    runs-on: ubuntu-latest
    steps: [{ run: "echo a" }]
  b:
    runs-on: ubuntu-latest
    needs: a
    steps: [{ run: "echo b" }]
  c:
    runs-on: ubuntu-latest
    needs: b
    steps: [{ run: "echo c" }]
"#;
        let wf = parse_workflow_string(yaml, &p()).unwrap();
        let order = topological_sort(&wf.jobs, None).unwrap();
        assert_eq!(order, vec!["a", "b", "c"]);
    }

    #[test]
    fn test_topological_sort_diamond() {
        let yaml = r#"
name: Test
on: push
jobs:
  a:
    runs-on: ubuntu-latest
    steps: [{ run: "echo a" }]
  b:
    runs-on: ubuntu-latest
    needs: a
    steps: [{ run: "echo b" }]
  c:
    runs-on: ubuntu-latest
    needs: a
    steps: [{ run: "echo c" }]
  d:
    runs-on: ubuntu-latest
    needs: [b, c]
    steps: [{ run: "echo d" }]
"#;
        let wf = parse_workflow_string(yaml, &p()).unwrap();
        let order = topological_sort(&wf.jobs, None).unwrap();
        assert_eq!(order[0], "a");
        assert_eq!(order[3], "d");
    }

    #[test]
    fn test_topological_sort_target() {
        let yaml = r#"
name: Test
on: push
jobs:
  build:
    runs-on: ubuntu-latest
    steps: [{ run: "echo build" }]
  test:
    runs-on: ubuntu-latest
    needs: build
    steps: [{ run: "echo test" }]
  deploy:
    runs-on: ubuntu-latest
    needs: test
    steps: [{ run: "echo deploy" }]
  unrelated:
    runs-on: ubuntu-latest
    steps: [{ run: "echo unrelated" }]
"#;
        let wf = parse_workflow_string(yaml, &p()).unwrap();
        let order = topological_sort(&wf.jobs, Some("test")).unwrap();
        assert_eq!(order.len(), 2);
        assert_eq!(order[0], "build");
        assert_eq!(order[1], "test");
    }
}
