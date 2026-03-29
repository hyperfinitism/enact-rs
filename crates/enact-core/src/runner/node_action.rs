// SPDX-License-Identifier: Apache-2.0

use super::shell::exec_shell;
use crate::error::Error;
use log::warn;
use std::collections::HashMap;
use std::path::Path;

/// Execute a Node.js action.
pub fn run_node_action(
    action_dir: &Path,
    main_file: &str,
    env: &HashMap<String, String>,
    _workspace: &Path,
    runner_temp: &Path,
    secrets: &[String],
) -> Result<HashMap<String, String>, Error> {
    let script = format!("node '{main_file}'");

    // Run node from the action directory directly instead of using `cd`
    let result = exec_shell("sh", &script, env, action_dir, 9999, secrets)?;

    if result.exit_code != 0 {
        warn!("    Node.js action exited with code {}", result.exit_code);
        return Err(Error::StepFailed {
            job: "action".into(),
            step: main_file.into(),
            exit_code: result.exit_code,
        });
    }

    // Read GITHUB_OUTPUT using runner-controlled path
    let output_path = runner_temp.join("github_output");
    let outputs = crate::context::env_files::parse_github_file(&output_path);
    Ok(outputs)
}
