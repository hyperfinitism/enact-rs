// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;

/// Full expression evaluation context — all GitHub Actions contexts.
#[derive(Debug, Clone)]
pub struct ExpressionContext {
    /// Full github context as JSON (supports deep property access).
    pub github: serde_json::Value,
    pub env: HashMap<String, String>,
    pub secrets: HashMap<String, String>,
    pub vars: HashMap<String, String>,
    pub runner: HashMap<String, String>,
    pub inputs: HashMap<String, String>,
    /// Matrix values for current job expansion.
    pub matrix: serde_json::Value,
    /// Strategy context (fail-fast, job-index, job-total, max-parallel).
    pub strategy: serde_json::Value,
    /// Needs context (dependent job results).
    pub needs: serde_json::Value,
    /// Job context (status, container, services).
    pub job: serde_json::Value,
    /// Steps context (outputs, outcome, conclusion).
    pub steps: serde_json::Value,
    /// Current job status.
    pub job_status: JobStatus,
}

impl Default for ExpressionContext {
    fn default() -> Self {
        ExpressionContext {
            github: serde_json::Value::Object(serde_json::Map::new()),
            env: HashMap::new(),
            secrets: HashMap::new(),
            vars: HashMap::new(),
            runner: HashMap::new(),
            inputs: HashMap::new(),
            matrix: serde_json::Value::Object(serde_json::Map::new()),
            strategy: serde_json::Value::Object(serde_json::Map::new()),
            needs: serde_json::Value::Object(serde_json::Map::new()),
            job: serde_json::Value::Object(serde_json::Map::new()),
            steps: serde_json::Value::Object(serde_json::Map::new()),
            job_status: JobStatus::Success,
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub enum JobStatus {
    #[default]
    Success,
    Failure,
    Cancelled,
}
