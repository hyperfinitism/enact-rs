// SPDX-License-Identifier: Apache-2.0

use super::model::Workflow;
use crate::error::Error;
use std::path::Path;

/// Parse a workflow YAML file.
pub fn parse_workflow_file(path: &Path) -> Result<Workflow, Error> {
    let contents = std::fs::read_to_string(path).map_err(|e| Error::WorkflowParse {
        file: path.to_path_buf(),
        message: format!("failed to read: {e}"),
    })?;
    parse_workflow_string(&contents, path)
}

/// Parse a workflow YAML string.
pub fn parse_workflow_string(yaml: &str, path: &Path) -> Result<Workflow, Error> {
    let workflow: Workflow = yaml_serde::from_str(yaml).map_err(|e| Error::WorkflowParse {
        file: path.to_path_buf(),
        message: format!("YAML parse error: {e}"),
    })?;
    validate_workflow(&workflow)?;
    Ok(workflow)
}

fn validate_workflow(workflow: &Workflow) -> Result<(), Error> {
    if workflow.jobs.is_empty() {
        return Err(Error::Validation(
            "workflow must have at least one job".into(),
        ));
    }
    for (job_id, job) in &workflow.jobs {
        let steps = job.steps.as_ref();
        if steps.is_none() || steps.is_some_and(|s| s.is_empty()) {
            return Err(Error::Validation(format!(
                "job '{job_id}' must have at least one step"
            )));
        }
        if let Some(steps) = &job.steps {
            for (i, step) in steps.iter().enumerate() {
                if step.run.is_none() && step.uses.is_none() {
                    let name = step.name.as_deref().unwrap_or("(unnamed)");
                    return Err(Error::Validation(format!(
                        "job '{job_id}', step {i} ('{name}'): must have either 'run' or 'uses'"
                    )));
                }
                if step.run.is_some() && step.uses.is_some() {
                    let name = step.name.as_deref().unwrap_or("(unnamed)");
                    return Err(Error::Validation(format!(
                        "job '{job_id}', step {i} ('{name}'): cannot have both 'run' and 'uses'"
                    )));
                }
            }
        }
        if let Some(needs) = &job.needs {
            for dep in needs.as_vec() {
                if !workflow.jobs.contains_key(dep) {
                    return Err(Error::Validation(format!(
                        "job '{job_id}' depends on non-existent job '{dep}'"
                    )));
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn p() -> PathBuf {
        PathBuf::from("<test>")
    }

    #[test]
    fn test_parse_simple() {
        let yaml = r#"
name: CI
on: push
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - run: cargo build
"#;
        let wf = parse_workflow_string(yaml, &p()).unwrap();
        assert_eq!(wf.name.as_deref(), Some("CI"));
        assert!(wf.jobs.contains_key("build"));
        assert_eq!(wf.jobs["build"].steps.as_ref().unwrap().len(), 2);
    }

    #[test]
    fn test_parse_multi_event() {
        let yaml = r#"
name: Test
on: [push, pull_request]
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo hello
"#;
        let wf = parse_workflow_string(yaml, &p()).unwrap();
        let events = wf.on.unwrap().events();
        assert!(events.contains(&"push".to_string()));
        assert!(events.contains(&"pull_request".to_string()));
    }

    #[test]
    fn test_validate_no_jobs() {
        let yaml = "name: Empty\non: push\njobs: {}\n";
        let err = parse_workflow_string(yaml, &p()).unwrap_err();
        assert!(err.to_string().contains("at least one job"));
    }

    #[test]
    fn test_validate_step_missing_run_or_uses() {
        let yaml = r#"
name: Bad
on: push
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - name: nothing
"#;
        let err = parse_workflow_string(yaml, &p()).unwrap_err();
        assert!(err.to_string().contains("must have either"));
    }

    #[test]
    fn test_validate_needs_nonexistent() {
        let yaml = r#"
name: Bad
on: push
jobs:
  build:
    runs-on: ubuntu-latest
    needs: [nonexistent]
    steps:
      - run: echo hello
"#;
        let err = parse_workflow_string(yaml, &p()).unwrap_err();
        assert!(err.to_string().contains("non-existent job"));
    }

    #[test]
    fn test_parse_matrix() {
        let yaml = r#"
name: Matrix
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    strategy:
      matrix:
        os: [ubuntu-latest, macos-latest]
        node: ["18", "20"]
    steps:
      - run: echo hello
"#;
        let wf = parse_workflow_string(yaml, &p()).unwrap();
        let strategy = wf.jobs["test"].strategy.as_ref().unwrap();
        let matrix = strategy.matrix.as_ref().unwrap();
        assert!(matrix.dimensions.contains_key("os"));
        assert!(matrix.dimensions.contains_key("node"));
    }

    #[test]
    fn test_parse_defaults() {
        let yaml = r#"
name: Defaults
on: push
defaults:
  run:
    shell: bash
    working-directory: ./src
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - run: echo hello
"#;
        let wf = parse_workflow_string(yaml, &p()).unwrap();
        let defaults = wf.defaults.as_ref().unwrap();
        assert_eq!(
            defaults.run.as_ref().unwrap().shell.as_deref(),
            Some("bash")
        );
    }

    #[test]
    fn test_parse_trigger_map_with_null_value() {
        let yaml = r#"
name: Test
on:
  push:
    paths:
      - 'src/**'
  workflow_dispatch:
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - run: echo hello
"#;
        let wf = parse_workflow_string(yaml, &p()).unwrap();
        let trigger = wf.on.as_ref().expect("on should be present");
        let events = trigger.events();
        assert!(events.contains(&"push".to_string()));
        assert!(events.contains(&"workflow_dispatch".to_string()));
    }
}
