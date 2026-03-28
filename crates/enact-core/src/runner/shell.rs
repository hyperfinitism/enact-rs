// SPDX-License-Identifier: Apache-2.0

use crate::error::Error;
use log::info;
use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Command, Stdio};

/// Result of executing a shell command.
pub struct ExecResult {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}

/// Map a GitHub Actions shell name to the actual command template.
/// The `{0}` placeholder is replaced with the script file path.
pub fn resolve_shell_template(shell: &str) -> (String, Vec<String>) {
    match shell {
        "bash" => (
            "bash".into(),
            vec![
                "--noprofile".into(),
                "--norc".into(),
                "-eo".into(),
                "pipefail".into(),
                "{0}".into(),
            ],
        ),
        "sh" => ("sh".into(), vec!["-e".into(), "{0}".into()]),
        "python" => ("python3".into(), vec!["{0}".into()]),
        "pwsh" => ("pwsh".into(), vec!["-command".into(), ". '{0}'".into()]),
        other if other.contains("{0}") => {
            // Custom template like "bash -e {0}"
            let parts: Vec<&str> = other.split_whitespace().collect();
            let program = parts[0].to_string();
            let args: Vec<String> = parts[1..].iter().map(|s| s.to_string()).collect();
            (program, args)
        }
        other => (other.into(), vec!["-e".into(), "{0}".into()]),
    }
}

/// Execute a script in a shell, streaming output in real time.
/// Returns the captured output and exit code.
pub fn exec_shell(
    shell: &str,
    script: &str,
    env: &HashMap<String, String>,
    cwd: &Path,
    step_index: usize,
    secrets: &[String],
) -> Result<ExecResult, Error> {
    let (program, args_template) = resolve_shell_template(shell);

    // Write script to a unique temp file
    let script_path =
        std::env::temp_dir().join(format!("gha_step_{step_index}_{}.sh", std::process::id()));
    std::fs::write(&script_path, script)?;

    // Replace {0} with the script path
    let script_path_str = script_path.to_string_lossy();
    let args: Vec<String> = if args_template.iter().any(|a| a.contains("{0}")) {
        args_template
            .iter()
            .map(|a| a.replace("{0}", &script_path_str))
            .collect()
    } else {
        let mut a = args_template;
        a.push("-c".into());
        a.push(script.into());
        a
    };

    let mut child = match Command::new(&program)
        .args(&args)
        .envs(env)
        .current_dir(cwd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(e) => {
            let _ = std::fs::remove_file(&script_path);
            return Err(Error::Action {
                action: format!("shell:{program}"),
                message: format!("failed to spawn: {e}"),
            });
        }
    };

    // Stream stdout and stderr in real time using threads
    let stdout_pipe = child.stdout.take();
    let stderr_pipe = child.stderr.take();
    let secrets_clone = secrets.to_vec();
    let secrets_clone2 = secrets.to_vec();

    let stdout_handle = std::thread::spawn(move || {
        let mut captured = String::new();
        if let Some(pipe) = stdout_pipe {
            let reader = BufReader::new(pipe);
            for line in reader.lines().map_while(Result::ok) {
                let redacted = redact_secrets(&line, &secrets_clone);
                info!("    | {redacted}");
                captured.push_str(&line);
                captured.push('\n');
            }
        }
        captured
    });

    let stderr_handle = std::thread::spawn(move || {
        let mut captured = String::new();
        if let Some(pipe) = stderr_pipe {
            let reader = BufReader::new(pipe);
            for line in reader.lines().map_while(Result::ok) {
                let redacted = redact_secrets(&line, &secrets_clone2);
                info!("    ! {redacted}");
                captured.push_str(&line);
                captured.push('\n');
            }
        }
        captured
    });

    let status = child.wait().map_err(|e| Error::Action {
        action: format!("shell:{program}"),
        message: format!("failed to wait: {e}"),
    })?;

    let stdout = stdout_handle.join().unwrap_or_default();
    let stderr = stderr_handle.join().unwrap_or_default();

    // Clean up temp script
    let _ = std::fs::remove_file(&script_path);

    Ok(ExecResult {
        exit_code: status.code().unwrap_or(-1),
        stdout,
        stderr,
    })
}

fn redact_secrets(line: &str, secrets: &[String]) -> String {
    let mut result = line.to_string();
    for secret in secrets {
        if !secret.is_empty() {
            result = result.replace(secret, "***");
        }
    }
    result
}
