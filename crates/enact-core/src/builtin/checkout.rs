// SPDX-License-Identifier: Apache-2.0

use crate::error::Error;
use crate::security::path_sanitizer::safe_resolve;
use log::info;
use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

/// Emulate actions/checkout.
/// If the workspace is already a git repo, this is a no-op.
/// Otherwise, clone the specified repository.
pub fn run(
    inputs: &HashMap<String, String>,
    workspace: &Path,
) -> Result<HashMap<String, String>, Error> {
    // Check if already a git repo
    let git_dir = workspace.join(".git");
    if git_dir.exists() {
        info!("    Workspace is already a git repo, checkout is a no-op");

        // If a specific ref is requested, check it out
        if let Some(git_ref) = inputs.get("INPUT_REF")
            && !git_ref.is_empty()
        {
            let status = Command::new("git")
                .args(["checkout", git_ref])
                .current_dir(workspace)
                .status()
                .map_err(|e| Error::Action {
                    action: "actions/checkout".into(),
                    message: format!("git checkout failed: {e}"),
                })?;
            if !status.success() {
                log::warn!("    git checkout {git_ref} failed (non-fatal)");
            }
        }

        // Handle submodules
        if let Some(submodules) = inputs.get("INPUT_SUBMODULES")
            && (submodules == "true" || submodules == "recursive")
        {
            let args = if submodules == "recursive" {
                vec!["submodule", "update", "--init", "--recursive"]
            } else {
                vec!["submodule", "update", "--init"]
            };
            let sub_status = Command::new("git")
                .args(&args)
                .current_dir(workspace)
                .status();
            match sub_status {
                Ok(s) if !s.success() => {
                    log::warn!("    git submodule update exited with non-zero status");
                }
                Err(e) => {
                    log::warn!("    git submodule update failed: {e}");
                }
                _ => {}
            }
        }

        return Ok(HashMap::new());
    }

    // Need to clone
    let repository = inputs.get("INPUT_REPOSITORY").cloned().unwrap_or_default();

    if repository.is_empty() {
        info!("    No repository to clone and workspace is not a git repo");
        return Ok(HashMap::new());
    }

    let url = if repository.contains("://") || repository.starts_with("git@") {
        repository.clone()
    } else {
        format!("https://github.com/{repository}.git")
    };

    let fetch_depth = inputs
        .get("INPUT_FETCH-DEPTH")
        .or_else(|| inputs.get("INPUT_FETCH_DEPTH"))
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(1);

    let checkout_path = if let Some(p) = inputs.get("INPUT_PATH") {
        safe_resolve(workspace, p).map_err(|e| Error::Action {
            action: "actions/checkout".into(),
            message: format!("unsafe path: {e}"),
        })?
    } else {
        workspace.to_path_buf()
    };

    let mut args = vec!["clone".to_string()];
    if fetch_depth > 0 {
        args.push("--depth".into());
        args.push(fetch_depth.to_string());
    }
    args.push(url);
    args.push(checkout_path.to_string_lossy().into());

    info!("    Cloning {repository}");
    let status = Command::new("git")
        .args(&args)
        .status()
        .map_err(|e| Error::Action {
            action: "actions/checkout".into(),
            message: format!("git clone failed: {e}"),
        })?;

    if !status.success() {
        return Err(Error::Action {
            action: "actions/checkout".into(),
            message: "git clone failed".into(),
        });
    }

    Ok(HashMap::new())
}
