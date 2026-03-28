// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::path::Path;

/// Build the runner.* context values.
pub fn build_runner_context(temp: &Path) -> HashMap<String, String> {
    let mut m = HashMap::new();
    m.insert("os".into(), "Linux".into());
    m.insert("arch".into(), detect_arch());
    m.insert("name".into(), "gha-runner-local".into());
    m.insert("temp".into(), temp.to_string_lossy().into());
    m.insert(
        "tool_cache".into(),
        temp.parent()
            .unwrap_or(temp)
            .join("tool_cache")
            .to_string_lossy()
            .into(),
    );
    m.insert("environment".into(), "self-hosted".into());
    m
}

fn detect_arch() -> String {
    match std::env::consts::ARCH {
        "x86_64" => "X64".into(),
        "aarch64" => "ARM64".into(),
        "arm" => "ARM".into(),
        other => other.to_uppercase(),
    }
}
