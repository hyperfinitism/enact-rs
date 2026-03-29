// SPDX-License-Identifier: Apache-2.0

use std::path::Path;

/// Generate an event.json payload for the given event type.
/// If `override_path` points to a user-supplied event file, use that instead.
pub fn generate_event_json(
    event: &str,
    repository: &str,
    sha: &str,
    git_ref: &str,
    override_path: Option<&Path>,
) -> serde_json::Value {
    if let Some(path) = override_path
        && let Ok(content) = std::fs::read_to_string(path)
        && let Ok(json) = serde_json::from_str(&content)
    {
        return json;
    }

    let owner = repository.split('/').next().unwrap_or("local");
    let repo_name = repository.split('/').nth(1).unwrap_or("repo");

    match event {
        "push" => serde_json::json!({
            "ref": git_ref,
            "before": "0000000000000000000000000000000000000000",
            "after": sha,
            "repository": {
                "full_name": repository,
                "name": repo_name,
                "owner": { "login": owner }
            },
            "sender": { "login": "local", "id": 0 },
            "head_commit": {
                "id": sha,
                "message": "local execution"
            },
            "commits": []
        }),
        "pull_request" => serde_json::json!({
            "action": "opened",
            "number": 1,
            "pull_request": {
                "number": 1,
                "head": { "ref": git_ref, "sha": sha },
                "base": { "ref": "main", "sha": sha },
                "title": "Local PR",
                "body": ""
            },
            "repository": {
                "full_name": repository,
                "name": repo_name,
                "owner": { "login": owner }
            },
            "sender": { "login": "local", "id": 0 }
        }),
        "workflow_dispatch" => serde_json::json!({
            "inputs": {},
            "ref": git_ref,
            "repository": {
                "full_name": repository,
                "name": repo_name,
                "owner": { "login": owner }
            },
            "sender": { "login": "local", "id": 0 }
        }),
        _ => serde_json::json!({
            "repository": {
                "full_name": repository,
                "name": repo_name,
                "owner": { "login": owner }
            },
            "sender": { "login": "local", "id": 0 }
        }),
    }
}
