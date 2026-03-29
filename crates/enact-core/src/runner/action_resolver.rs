// SPDX-License-Identifier: Apache-2.0

use crate::error::Error;
use log::{debug, info};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Resolved action reference.
pub enum ActionRef {
    /// Local action: `./path`
    Local(PathBuf),
    /// Remote action: `owner/repo@version` (with optional subpath)
    Remote {
        dir: PathBuf,
        owner_repo: String,
        version: String,
    },
}

/// Parse and resolve a `uses:` reference.
pub fn resolve_action(
    uses: &str,
    workspace: &Path,
    actions_cache: &Path,
) -> Result<ActionRef, Error> {
    // Reject ../path references — they can escape the workspace
    if uses.starts_with("../") {
        return Err(Error::Action {
            action: uses.to_string(),
            message:
                "relative parent paths (../) are not allowed for security reasons; use ./ instead"
                    .into(),
        });
    }

    // Local actions: ./path only
    if uses.starts_with("./") {
        let action_path = workspace.join(uses);
        // Canonicalize to resolve "..", symlinks, etc. and verify the result
        // stays within the workspace.
        let canonical = action_path.canonicalize().map_err(|_| Error::Action {
            action: uses.to_string(),
            message: format!("local action not found: {}", action_path.display()),
        })?;
        let workspace_canonical = workspace.canonicalize().map_err(|e| Error::Action {
            action: uses.to_string(),
            message: format!("cannot resolve workspace: {e}"),
        })?;
        if !canonical.starts_with(&workspace_canonical) {
            return Err(Error::Action {
                action: uses.to_string(),
                message: format!("local action '{}' resolves outside the workspace", uses),
            });
        }
        return Ok(ActionRef::Local(canonical));
    }

    // Docker actions: docker://image
    if uses.starts_with("docker://") {
        return Err(Error::Action {
            action: uses.to_string(),
            message: "docker:// actions not yet supported in emulator mode".into(),
        });
    }

    // Remote: owner/repo@version or owner/repo/subpath@version
    let (repo_path, version) = parse_action_ref(uses)?;
    let (owner_repo, subpath) = split_subpath(&repo_path);

    // Validate version and subpath components to prevent cache path traversal
    validate_path_component(&version, uses)?;
    if let Some(sub) = subpath {
        for segment in sub.split('/') {
            validate_path_component(segment, uses)?;
        }
    }

    let cache_dir = actions_cache
        .join(owner_repo.replace('/', "_"))
        .join(&version);

    // Clone if not already cached
    if !cache_dir.exists() {
        clone_action(owner_repo, &version, &cache_dir)?;
    }

    let action_dir = match subpath {
        Some(sub) => cache_dir.join(sub),
        None => cache_dir,
    };

    // Final check: canonicalize and verify the action_dir stays under actions_cache
    if action_dir.exists() {
        let canonical = action_dir.canonicalize().map_err(|e| Error::Action {
            action: uses.to_string(),
            message: format!("cannot resolve action dir: {e}"),
        })?;
        let cache_canonical = actions_cache.canonicalize().map_err(|e| Error::Action {
            action: uses.to_string(),
            message: format!("cannot resolve actions cache: {e}"),
        })?;
        if !canonical.starts_with(&cache_canonical) {
            return Err(Error::Action {
                action: uses.to_string(),
                message: "action path escapes the actions cache directory".into(),
            });
        }
    }

    Ok(ActionRef::Remote {
        dir: action_dir,
        owner_repo: owner_repo.to_string(),
        version,
    })
}

/// Reject path components that could cause traversal (e.g. "..", ".", or empty).
fn validate_path_component(component: &str, action: &str) -> Result<(), Error> {
    if component.is_empty() || component == "." || component == ".." || component.contains('\\') {
        return Err(Error::Action {
            action: action.to_string(),
            message: format!("invalid path component in action reference: '{component}'"),
        });
    }
    Ok(())
}

fn parse_action_ref(uses: &str) -> Result<(String, String), Error> {
    let parts: Vec<&str> = uses.splitn(2, '@').collect();
    if parts.len() != 2 {
        return Err(Error::Action {
            action: uses.to_string(),
            message: "invalid action reference: missing @version".into(),
        });
    }
    Ok((parts[0].to_string(), parts[1].to_string()))
}

/// Split "owner/repo/subpath" into ("owner/repo", Some("subpath")).
fn split_subpath(repo_path: &str) -> (&str, Option<&str>) {
    let parts: Vec<&str> = repo_path.splitn(3, '/').collect();
    if parts.len() >= 3 {
        // Find the position after owner/repo
        let owner_repo_end = parts[0].len() + 1 + parts[1].len();
        let owner_repo = &repo_path[..owner_repo_end];
        let subpath = &repo_path[owner_repo_end + 1..];
        (owner_repo, Some(subpath))
    } else {
        (repo_path, None)
    }
}

fn clone_action(owner_repo: &str, version: &str, target: &Path) -> Result<(), Error> {
    let url = format!("https://github.com/{owner_repo}.git");
    info!("    Cloning action: {owner_repo}@{version}");

    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let status = Command::new("git")
        .args(["clone", &url, &target.to_string_lossy()])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map_err(|e| Error::Action {
            action: owner_repo.to_string(),
            message: format!("git clone failed: {e}"),
        })?;
    if !status.success() {
        return Err(Error::Action {
            action: owner_repo.to_string(),
            message: "git clone failed".into(),
        });
    }

    let status = Command::new("git")
        .args(["checkout", version])
        .current_dir(target)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map_err(|e| Error::Action {
            action: owner_repo.to_string(),
            message: format!("git checkout {version} failed: {e}"),
        })?;
    if !status.success() {
        let _ = std::fs::remove_dir_all(target);
        return Err(Error::Action {
            action: owner_repo.to_string(),
            message: format!("version '{version}' not found in {owner_repo}"),
        });
    }

    debug!("    Cloned {owner_repo}@{version}");
    Ok(())
}

/// Parse an action.yml or action.yaml from the action directory.
pub fn read_action_yml(action_dir: &Path) -> Option<ActionYml> {
    let yml_path = action_dir.join("action.yml");
    let yaml_path = action_dir.join("action.yaml");

    let content = if yml_path.exists() {
        std::fs::read_to_string(&yml_path).ok()?
    } else if yaml_path.exists() {
        std::fs::read_to_string(&yaml_path).ok()?
    } else {
        return None;
    };

    match yaml_serde::from_str(&content) {
        Ok(action) => Some(action),
        Err(e) => {
            log::warn!("Failed to parse action YAML in {:?}: {}", action_dir, e);
            None
        }
    }
}

#[derive(Debug, serde::Deserialize)]
pub struct ActionYml {
    pub name: Option<String>,
    pub description: Option<String>,
    pub inputs: Option<std::collections::HashMap<String, ActionInput>>,
    pub runs: ActionRuns,
}

#[derive(Debug, serde::Deserialize)]
pub struct ActionInput {
    pub description: Option<String>,
    pub required: Option<bool>,
    pub default: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
pub struct ActionRuns {
    pub using: String,
    pub steps: Option<Vec<CompositeStep>>,
    pub main: Option<String>,
    pub pre: Option<String>,
    pub post: Option<String>,
    pub image: Option<String>,
    pub entrypoint: Option<String>,
    pub args: Option<Vec<String>>,
}

#[derive(Debug, serde::Deserialize)]
pub struct CompositeStep {
    pub name: Option<String>,
    pub run: Option<String>,
    pub uses: Option<String>,
    pub shell: Option<String>,
    #[serde(rename = "working-directory")]
    pub working_directory: Option<String>,
    pub env: Option<std::collections::HashMap<String, String>>,
    #[serde(rename = "if")]
    pub condition: Option<String>,
    #[serde(rename = "with")]
    pub with: Option<std::collections::HashMap<String, String>>,
    pub id: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_action_ref() {
        let (repo, version) = parse_action_ref("actions/checkout@v4").unwrap();
        assert_eq!(repo, "actions/checkout");
        assert_eq!(version, "v4");
    }

    #[test]
    fn test_split_subpath() {
        let (repo, sub) = split_subpath("actions/cache/restore");
        assert_eq!(repo, "actions/cache");
        assert_eq!(sub, Some("restore"));
    }

    #[test]
    fn test_split_no_subpath() {
        let (repo, sub) = split_subpath("actions/checkout");
        assert_eq!(repo, "actions/checkout");
        assert_eq!(sub, None);
    }

    #[test]
    fn test_parse_action_ref_missing_version() {
        let err = parse_action_ref("actions/checkout").unwrap_err();
        assert!(err.to_string().contains("missing @version"));
    }
}
