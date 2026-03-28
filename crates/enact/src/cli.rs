// SPDX-License-Identifier: Apache-2.0

use anyhow::{Context, Result, bail};
use clap::Parser;
use enact_core::runner::engine::{EngineConfig, run_workflow};
use enact_core::workflow::loader::discover_workflows;
use enact_core::workflow::model::Event;
use enact_core::workflow::parser::parse_workflow_file;
use flexi_logger::LevelFilter;
use log::info;
use std::collections::HashMap;
use std::path::PathBuf;

use crate::logger::init_logger;

#[derive(Parser)]
#[command(name = "enact", about = "GitHub Actions runner emulator")]
struct Cli {
    /// Path to the workspace (default: current directory)
    #[arg(short, long, default_value = ".")]
    workspace: PathBuf,

    /// Path to the workflow file (default: discover from .github/workflows/)
    #[arg(short = 'f', long)]
    workflow: Option<PathBuf>,

    /// Event name (default: push)
    #[arg(short, long, value_enum, default_value_t = Event::Push)]
    event: Event,

    /// Path to event.json payload
    #[arg(long)]
    event_file: Option<PathBuf>,

    /// Target a specific job
    #[arg(short, long)]
    job: Option<String>,

    /// Set environment variable (KEY=VALUE), repeatable
    #[arg(long = "env", value_name = "KEY=VALUE")]
    envs: Vec<String>,

    /// Set secret (KEY=VALUE), repeatable
    #[arg(short, long = "secret", value_name = "KEY=VALUE")]
    secrets: Vec<String>,

    /// Default shell (default: bash)
    #[arg(long, default_value = "bash")]
    default_shell: String,

    /// Runner temp directory
    #[arg(long, default_value = "/tmp/enact/runner")]
    runner_temp: PathBuf,

    /// Actions cache directory
    #[arg(long, default_value = "/tmp/enact/actions-cache")]
    actions_cache: PathBuf,

    /// Verbosity level (Trace, Debug, Info, Warn, Error, Off)
    #[arg(short = 'v', long, default_value = "Info")]
    pub verbosity: LevelFilter,

    /// Log file path (default: None)
    #[arg(short = 'l', long = "log-file")]
    pub log_file: Option<PathBuf>,
}

fn parse_kv_pairs(pairs: &[String]) -> Result<HashMap<String, String>> {
    let mut map = HashMap::new();
    for s in pairs {
        let (k, v) = s
            .split_once('=')
            .ok_or_else(|| anyhow::anyhow!("invalid KEY=VALUE argument: '{s}'"))?;
        map.insert(k.to_string(), v.to_string());
    }
    Ok(map)
}

pub fn run() -> Result<()> {
    let cli = Cli::parse();

    if let Err(e) = init_logger(cli.verbosity, cli.log_file.clone()) {
        eprintln!("error: failed to initialise logger: {e}");
        std::process::exit(1);
    }

    let workspace = cli
        .workspace
        .canonicalize()
        .context("failed to resolve workspace path")?;

    // Set RUNNER_TEMP so built-in actions can find it
    std::fs::create_dir_all(&cli.runner_temp)?;
    // SAFETY: We are single-threaded at this point (before any workflow execution).
    unsafe { std::env::set_var("RUNNER_TEMP", &cli.runner_temp) };

    std::fs::create_dir_all(&cli.actions_cache)?;

    let extra_env = parse_kv_pairs(&cli.envs)?;
    let secrets = parse_kv_pairs(&cli.secrets)?;

    // Discover or use specified workflow file(s)
    let workflow_files = if let Some(ref wf) = cli.workflow {
        let resolved = if wf.is_absolute() {
            wf.clone()
        } else {
            workspace.join(wf)
        };
        vec![resolved]
    } else {
        let wf_dir = workspace.join(".github").join("workflows");
        if !wf_dir.exists() {
            bail!(
                "No .github/workflows/ directory found in {}",
                workspace.display()
            );
        }
        let files = discover_workflows(&wf_dir)?;
        if files.is_empty() {
            bail!("No workflow files found in {}", wf_dir.display());
        }
        files
    };

    let mut all_success = true;

    for wf_path in &workflow_files {
        let workflow = parse_workflow_file(wf_path)
            .with_context(|| format!("failed to parse {}", wf_path.display()))?;

        let wf_name = wf_path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "workflow".into());

        let config = EngineConfig {
            event_name: cli.event.as_str(),
            workspace: &workspace,
            extra_env: &extra_env,
            secrets: &secrets,
            target_job: cli.job.as_deref(),
            event_file: cli.event_file.as_deref(),
            runner_temp: &cli.runner_temp,
            actions_cache: &cli.actions_cache,
            default_shell: &cli.default_shell,
        };

        match run_workflow(&workflow, &wf_name, &config) {
            Ok(success) => {
                if !success {
                    all_success = false;
                }
            }
            Err(e) => {
                log::error!("Workflow '{}' failed: {e}", wf_name);
                all_success = false;
            }
        }
    }

    if !all_success {
        bail!("One or more workflows failed");
    }

    info!("\x1b[32mAll workflows completed successfully\x1b[0m");
    Ok(())
}
