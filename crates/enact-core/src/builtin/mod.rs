// SPDX-License-Identifier: Apache-2.0

pub mod artifact;
pub mod cache;
pub mod checkout;

use crate::error::Error;
use log::info;
use std::collections::HashMap;
use std::path::Path;

/// Try to handle an action as a built-in. Returns None if not a built-in.
pub fn try_builtin_action(
    uses: &str,
    inputs: &HashMap<String, String>,
    workspace: &Path,
    runner_temp: &Path,
    actions_cache: &Path,
) -> Option<Result<HashMap<String, String>, Error>> {
    let action_name = uses.split('@').next().unwrap_or(uses);

    match action_name {
        "actions/checkout" => {
            info!("    Built-in: actions/checkout");
            Some(checkout::run(inputs, workspace))
        }
        "actions/upload-artifact" => {
            info!("    Built-in: actions/upload-artifact");
            Some(artifact::upload(inputs, workspace, runner_temp))
        }
        "actions/download-artifact" => {
            info!("    Built-in: actions/download-artifact");
            Some(artifact::download(inputs, workspace, runner_temp))
        }
        "actions/cache" => {
            info!("    Built-in: actions/cache");
            Some(cache::run_cache(inputs, workspace, actions_cache))
        }
        "actions/cache/save" => {
            info!("    Built-in: actions/cache/save");
            Some(cache::save(inputs, workspace, actions_cache))
        }
        "actions/cache/restore" => {
            info!("    Built-in: actions/cache/restore");
            Some(cache::restore(inputs, workspace, actions_cache))
        }
        // No-op actions: skip without error
        "Swatinem/rust-cache"
        | "actions/setup-node"
        | "actions/setup-python"
        | "actions/setup-java"
        | "actions/setup-go"
        | "actions/setup-dotnet" => {
            info!("    Built-in: {action_name} (no-op)");
            Some(Ok(HashMap::new()))
        }
        _ => None,
    }
}
