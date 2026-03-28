// SPDX-License-Identifier: Apache-2.0

mod cli;
mod logger;

use anyhow::Result;

fn main() -> Result<()> {
    cli::run()
}
