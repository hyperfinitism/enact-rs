// SPDX-License-Identifier: Apache-2.0

use crate::error::Error;
use std::path::{Path, PathBuf};

/// Discover workflow files in a directory (*.yml and *.yaml).
pub fn discover_workflows(dir: &Path) -> Result<Vec<PathBuf>, Error> {
    let mut files = Vec::new();
    for ext in ["yml", "yaml"] {
        let pattern = format!("{}/*.{ext}", dir.display());
        let entries = glob::glob(&pattern).map_err(|e| Error::WorkflowParse {
            file: dir.to_path_buf(),
            message: format!("invalid glob pattern: {e}"),
        })?;
        for entry in entries {
            let path = entry.map_err(|e| Error::WorkflowParse {
                file: dir.to_path_buf(),
                message: format!("error reading: {e}"),
            })?;
            files.push(path);
        }
    }
    files.sort();
    files.dedup();
    Ok(files)
}
