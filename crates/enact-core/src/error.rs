// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("workflow parse error in {file}: {message}")]
    WorkflowParse { file: PathBuf, message: String },

    #[error("workflow validation: {0}")]
    Validation(String),

    #[error("expression syntax error at position {position}: {message}")]
    ExpressionSyntax { position: usize, message: String },

    #[error("expression evaluation error: {0}")]
    ExpressionEval(String),

    #[error("unknown function: {0}")]
    UnknownFunction(String),

    #[error("job dependency cycle: {0}")]
    DependencyCycle(String),

    #[error("step '{step}' in job '{job}' failed (exit code {exit_code})")]
    StepFailed {
        job: String,
        step: String,
        exit_code: i32,
    },

    #[error("job '{0}' failed")]
    JobFailed(String),

    #[error("action '{action}' error: {message}")]
    Action { action: String, message: String },

    #[error("path traversal attempt: {0}")]
    PathTraversal(String),

    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
