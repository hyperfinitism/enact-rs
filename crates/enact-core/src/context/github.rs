// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::path::Path;

/// Build the github context as a JSON value, and the GITHUB_* env vars.
pub fn build_github_context(
    event_name: &str,
    workspace: &Path,
    repository: &str,
    sha: &str,
    git_ref: &str,
    job_id: &str,
    event_payload: &serde_json::Value,
) -> serde_json::Value {
    serde_json::json!({
        "event_name": event_name,
        "event": event_payload,
        "repository": repository,
        "sha": sha,
        "ref": git_ref,
        "ref_name": ref_name(git_ref),
        "ref_type": ref_type(git_ref),
        "job": job_id,
        "workspace": workspace.to_string_lossy(),
        "action": "",
        "action_path": "",
        "actor": "local",
        "actor_id": "0",
        "run_id": "1",
        "run_number": "1",
        "run_attempt": "1",
        "server_url": "https://github.com",
        "api_url": "https://api.github.com",
        "graphql_url": "https://api.github.com/graphql",
        "repository_owner": repository.split('/').next().unwrap_or("local"),
        "workflow": "",
        "head_ref": "",
        "base_ref": "",
        "token": "",
        "retention_days": "90",
    })
}

/// Build the GITHUB_* environment variables map.
pub fn build_github_env(
    event_name: &str,
    workspace: &Path,
    repository: &str,
    sha: &str,
    git_ref: &str,
    job_id: &str,
    runner_temp: &Path,
) -> HashMap<String, String> {
    let mut env = HashMap::new();
    env.insert("CI".into(), "true".into());
    env.insert("GITHUB_ACTIONS".into(), "true".into());
    env.insert("GITHUB_EVENT_NAME".into(), event_name.into());
    env.insert(
        "GITHUB_WORKSPACE".into(),
        workspace.to_string_lossy().into(),
    );
    env.insert("GITHUB_REPOSITORY".into(), repository.into());
    env.insert("GITHUB_SHA".into(), sha.into());
    env.insert("GITHUB_REF".into(), git_ref.into());
    env.insert("GITHUB_REF_NAME".into(), ref_name(git_ref));
    env.insert("GITHUB_REF_TYPE".into(), ref_type(git_ref));
    env.insert("GITHUB_JOB".into(), job_id.into());
    env.insert(
        "GITHUB_REPOSITORY_OWNER".into(),
        repository.split('/').next().unwrap_or("local").into(),
    );
    env.insert("GITHUB_ACTOR".into(), "local".into());
    env.insert("GITHUB_RUN_ID".into(), "1".into());
    env.insert("GITHUB_RUN_NUMBER".into(), "1".into());
    env.insert("GITHUB_RUN_ATTEMPT".into(), "1".into());
    env.insert("GITHUB_SERVER_URL".into(), "https://github.com".into());
    env.insert("GITHUB_API_URL".into(), "https://api.github.com".into());
    // File-based output mechanism
    let temp = runner_temp.to_string_lossy();
    env.insert("GITHUB_OUTPUT".into(), format!("{temp}/github_output"));
    env.insert("GITHUB_ENV".into(), format!("{temp}/github_env"));
    env.insert("GITHUB_PATH".into(), format!("{temp}/github_path"));
    env.insert("GITHUB_STATE".into(), format!("{temp}/github_state"));
    env.insert(
        "GITHUB_STEP_SUMMARY".into(),
        format!("{temp}/github_step_summary"),
    );
    env.insert("GITHUB_EVENT_PATH".into(), format!("{temp}/event.json"));
    // Runner env vars
    env.insert("RUNNER_OS".into(), detect_os().into());
    env.insert("RUNNER_ARCH".into(), detect_arch().into());
    env.insert("RUNNER_TEMP".into(), temp.to_string());
    env.insert("RUNNER_TOOL_CACHE".into(), format!("{temp}/../tool_cache"));
    env
}

/// Detect git info from a workspace directory.
pub fn detect_git_info(repo_dir: &Path) -> (String, String, String) {
    let repository = read_git_remote(repo_dir).unwrap_or_else(|| "local/repo".to_string());
    let sha = read_git_sha(repo_dir)
        .unwrap_or_else(|| "0000000000000000000000000000000000000000".to_string());
    let git_ref = read_git_ref(repo_dir).unwrap_or_else(|| "refs/heads/main".to_string());
    (repository, sha, git_ref)
}

fn read_git_sha(dir: &Path) -> Option<String> {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(dir)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn read_git_ref(dir: &Path) -> Option<String> {
    let output = std::process::Command::new("git")
        .args(["symbolic-ref", "HEAD"])
        .current_dir(dir)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn read_git_remote(dir: &Path) -> Option<String> {
    let output = std::process::Command::new("git")
        .args(["remote", "get-url", "origin"])
        .current_dir(dir)
        .output()
        .ok()?;
    if output.status.success() {
        let url = String::from_utf8_lossy(&output.stdout).trim().to_string();
        extract_repo_from_url(&url)
    } else {
        None
    }
}

fn extract_repo_from_url(url: &str) -> Option<String> {
    if let Some(rest) = url.strip_prefix("git@") {
        let path = rest.split(':').nth(1)?;
        return Some(path.trim_end_matches(".git").to_string());
    }
    if let Some(idx) = url.find("github.com/") {
        let path = &url[idx + "github.com/".len()..];
        return Some(path.trim_end_matches(".git").to_string());
    }
    None
}

/// Detect the runner OS in GitHub Actions format.
fn detect_os() -> &'static str {
    if cfg!(target_os = "linux") {
        "Linux"
    } else if cfg!(target_os = "macos") {
        "macOS"
    } else if cfg!(target_os = "windows") {
        "Windows"
    } else {
        "Linux"
    }
}

/// Detect the runner architecture in GitHub Actions format.
fn detect_arch() -> &'static str {
    if cfg!(target_arch = "x86_64") {
        "X64"
    } else if cfg!(target_arch = "aarch64") {
        "ARM64"
    } else if cfg!(target_arch = "x86") {
        "X86"
    } else {
        "X64"
    }
}

fn ref_name(git_ref: &str) -> String {
    git_ref
        .strip_prefix("refs/heads/")
        .or_else(|| git_ref.strip_prefix("refs/tags/"))
        .unwrap_or(git_ref)
        .to_string()
}

fn ref_type(git_ref: &str) -> String {
    if git_ref.starts_with("refs/tags/") {
        "tag".to_string()
    } else {
        "branch".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_repo_ssh() {
        assert_eq!(
            extract_repo_from_url("git@github.com:owner/repo.git"),
            Some("owner/repo".to_string())
        );
    }

    #[test]
    fn test_extract_repo_https() {
        assert_eq!(
            extract_repo_from_url("https://github.com/owner/repo.git"),
            Some("owner/repo".to_string())
        );
    }

    #[test]
    fn test_ref_name() {
        assert_eq!(ref_name("refs/heads/main"), "main");
        assert_eq!(ref_name("refs/tags/v1.0"), "v1.0");
    }

    #[test]
    fn test_ref_type() {
        assert_eq!(ref_type("refs/heads/main"), "branch");
        assert_eq!(ref_type("refs/tags/v1.0"), "tag");
    }
}
