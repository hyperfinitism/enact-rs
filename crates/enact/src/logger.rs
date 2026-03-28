// SPDX-License-Identifier: Apache-2.0

use anyhow::{Context, Result};
use flexi_logger::{Duplicate, FileSpec, LevelFilter, Logger};
use std::path::PathBuf;

pub(crate) fn init_logger(verbosity: LevelFilter, log_file: Option<PathBuf>) -> Result<()> {
    let logger = if let Some(p) = &log_file {
        Logger::with(verbosity)
            .log_to_file(FileSpec::try_from(p)?)
            .duplicate_to_stderr(Duplicate::from(verbosity))
    } else {
        Logger::with(verbosity).log_to_stdout()
    };

    if let Err(e) = logger.start() {
        return Err(e).context("failed to start flexi_logger");
    }

    Ok(())
}
