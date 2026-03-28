// SPDX-License-Identifier: Apache-2.0

use crate::error::Error;
use crate::security::path_sanitizer::safe_resolve;
use log::info;
use std::collections::HashMap;
use std::path::Path;

/// Emulate actions/upload-artifact.
/// Copies matched files to a local artifacts directory.
pub fn upload(
    inputs: &HashMap<String, String>,
    workspace: &Path,
    runner_temp: &Path,
) -> Result<HashMap<String, String>, Error> {
    let name = inputs
        .get("INPUT_NAME")
        .cloned()
        .unwrap_or_else(|| "artifact".into());
    let path_input = inputs.get("INPUT_PATH").cloned().unwrap_or_default();
    let if_no_files = inputs
        .get("INPUT_IF-NO-FILES-FOUND")
        .or_else(|| inputs.get("INPUT_IF_NO_FILES_FOUND"))
        .cloned()
        .unwrap_or_else(|| "warn".into());

    if path_input.is_empty() {
        return Err(Error::Action {
            action: "actions/upload-artifact".into(),
            message: "path input is required".into(),
        });
    }

    let artifacts_base = runner_temp.join("artifacts");
    let dest = artifacts_base.join(&name);
    std::fs::create_dir_all(&dest)?;

    let mut file_count = 0u64;
    for pattern in path_input.lines() {
        let pattern = pattern.trim();
        if pattern.is_empty() {
            continue;
        }

        // Path traversal check
        safe_resolve(workspace, pattern).map_err(|e| Error::Action {
            action: "actions/upload-artifact".into(),
            message: format!("unsafe path: {e}"),
        })?;

        let full_pattern = workspace.join(pattern).to_string_lossy().to_string();
        if let Ok(entries) = glob::glob(&full_pattern) {
            for entry in entries.flatten() {
                if entry.is_file() {
                    let rel = entry.strip_prefix(workspace).unwrap_or(&entry);
                    let dest_file = dest.join(rel);
                    if let Some(parent) = dest_file.parent() {
                        std::fs::create_dir_all(parent)?;
                    }
                    std::fs::copy(&entry, &dest_file)?;
                    file_count += 1;
                }
            }
        }
    }

    if file_count == 0 {
        match if_no_files.as_str() {
            "error" => {
                return Err(Error::Action {
                    action: "actions/upload-artifact".into(),
                    message: "no files found to upload".into(),
                });
            }
            "warn" => log::warn!("    No files found matching: {path_input}"),
            _ => {} // ignore
        }
    } else {
        info!("    Uploaded {file_count} file(s) as artifact '{name}'");
    }

    let mut outputs = HashMap::new();
    outputs.insert("artifact-id".into(), name.clone());
    Ok(outputs)
}

/// Emulate actions/download-artifact.
/// Copies from local artifacts directory to the specified path.
pub fn download(
    inputs: &HashMap<String, String>,
    workspace: &Path,
    runner_temp: &Path,
) -> Result<HashMap<String, String>, Error> {
    let name = inputs
        .get("INPUT_NAME")
        .cloned()
        .unwrap_or_else(|| "artifact".into());
    let dest_path = inputs
        .get("INPUT_PATH")
        .cloned()
        .unwrap_or_else(|| ".".into());

    // Path traversal check on destination — use the resolved path
    let dest = safe_resolve(workspace, &dest_path).map_err(|e| Error::Action {
        action: "actions/download-artifact".into(),
        message: format!("unsafe path: {e}"),
    })?;

    let artifacts_base = runner_temp.join("artifacts");
    let src = artifacts_base.join(&name);

    if !src.exists() {
        return Err(Error::Action {
            action: "actions/download-artifact".into(),
            message: format!("artifact '{name}' not found"),
        });
    }

    std::fs::create_dir_all(&dest)?;
    copy_dir_recursive(&src, &dest)?;

    info!("    Downloaded artifact '{name}' to {dest_path}");
    Ok(HashMap::new())
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<(), Error> {
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if src_path.is_dir() {
            std::fs::create_dir_all(&dst_path)?;
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            std::fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}
