// SPDX-License-Identifier: Apache-2.0

use crate::error::Error;
use log::info;
use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

/// Emulate actions/cache (both save and restore).
pub fn run_cache(
    inputs: &HashMap<String, String>,
    workspace: &Path,
    cache_base: &Path,
) -> Result<HashMap<String, String>, Error> {
    // Try restore first
    let outputs = restore(inputs, workspace, cache_base)?;
    // Save will happen in post-step (we always save for simplicity)
    // For the emulator, we just note the paths for later
    if outputs.get("cache-hit").map(|v| v.as_str()) != Some("true") {
        // Will be saved in post-step
        info!("    Cache miss, will save after job");
    }
    Ok(outputs)
}

/// Emulate actions/cache/restore.
pub fn restore(
    inputs: &HashMap<String, String>,
    workspace: &Path,
    cache_base: &Path,
) -> Result<HashMap<String, String>, Error> {
    let key = inputs.get("INPUT_KEY").cloned().unwrap_or_default();
    let paths = inputs.get("INPUT_PATH").cloned().unwrap_or_default();
    let restore_keys = inputs
        .get("INPUT_RESTORE-KEYS")
        .or_else(|| inputs.get("INPUT_RESTORE_KEYS"))
        .cloned()
        .unwrap_or_default();

    if key.is_empty() {
        return Err(Error::Action {
            action: "actions/cache".into(),
            message: "key input is required".into(),
        });
    }

    let cache_dir = cache_base.join("cache");
    std::fs::create_dir_all(&cache_dir)?;

    let mut outputs = HashMap::new();
    outputs.insert("cache-primary-key".into(), key.clone());

    // Try exact key match
    let cache_archive = cache_dir.join(format!("{}.tar.gz", sanitize_key(&key)));
    if cache_archive.exists() {
        extract_cache(&cache_archive, &paths, workspace)?;
        info!("    Cache hit: {key}");
        outputs.insert("cache-hit".into(), "true".into());
        outputs.insert("cache-matched-key".into(), key);
        return Ok(outputs);
    }

    // Try restore-keys prefix match
    for restore_key in restore_keys.lines() {
        let restore_key = restore_key.trim();
        if restore_key.is_empty() {
            continue;
        }
        // Find any cache file whose name starts with this prefix
        if let Ok(entries) = std::fs::read_dir(&cache_dir) {
            let prefix = sanitize_key(restore_key);
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with(&prefix) && name.ends_with(".tar.gz") {
                    extract_cache(&entry.path(), &paths, workspace)?;
                    let matched = name.trim_end_matches(".tar.gz").to_string();
                    info!("    Cache restored (partial match): {matched}");
                    outputs.insert("cache-hit".into(), "false".into());
                    outputs.insert("cache-matched-key".into(), matched);
                    return Ok(outputs);
                }
            }
        }
    }

    info!("    Cache miss: {key}");
    outputs.insert("cache-hit".into(), "false".into());
    Ok(outputs)
}

/// Emulate actions/cache/save.
pub fn save(
    inputs: &HashMap<String, String>,
    workspace: &Path,
    cache_base: &Path,
) -> Result<HashMap<String, String>, Error> {
    let key = inputs.get("INPUT_KEY").cloned().unwrap_or_default();
    let paths = inputs.get("INPUT_PATH").cloned().unwrap_or_default();

    if key.is_empty() || paths.is_empty() {
        return Err(Error::Action {
            action: "actions/cache/save".into(),
            message: "key and path inputs are required".into(),
        });
    }

    let cache_dir = cache_base.join("cache");
    std::fs::create_dir_all(&cache_dir)?;

    let cache_archive = cache_dir.join(format!("{}.tar.gz", sanitize_key(&key)));

    // Don't overwrite existing cache (immutable)
    if cache_archive.exists() {
        info!("    Cache already exists for key: {key}");
        return Ok(HashMap::new());
    }

    create_cache(&cache_archive, &paths, workspace)?;
    info!("    Cache saved: {key}");

    Ok(HashMap::new())
}

fn sanitize_key(key: &str) -> String {
    key.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn create_cache(archive: &Path, paths: &str, workspace: &Path) -> Result<(), Error> {
    let path_args: Vec<&str> = paths
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect();

    if path_args.is_empty() {
        return Ok(());
    }

    let mut args = vec![
        "czf".to_string(),
        archive.to_string_lossy().into(),
        "-C".to_string(),
        workspace.to_string_lossy().into(),
    ];
    args.extend(path_args.iter().map(|s| s.to_string()));

    let status = Command::new("tar")
        .args(&args)
        .status()
        .map_err(|e| Error::Action {
            action: "cache/save".into(),
            message: format!("tar failed: {e}"),
        })?;

    if !status.success() {
        log::warn!("    Cache tar creation returned non-zero (some paths may not exist)");
    }

    Ok(())
}

fn extract_cache(archive: &Path, _paths: &str, workspace: &Path) -> Result<(), Error> {
    let status = Command::new("tar")
        .args([
            "xzf",
            &archive.to_string_lossy(),
            "-C",
            &workspace.to_string_lossy(),
        ])
        .status()
        .map_err(|e| Error::Action {
            action: "cache/restore".into(),
            message: format!("tar extract failed: {e}"),
        })?;

    if !status.success() {
        log::warn!("    Cache extraction returned non-zero");
    }

    Ok(())
}
