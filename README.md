# enact

![SemVer: pre-release](https://img.shields.io/badge/enact-pre--release-blue)
![MSRV: 1.88.0](https://img.shields.io/badge/MSRV-1.88.0-brown.svg)
[![License: Apache-2.0](https://img.shields.io/badge/License-Apache--2.0-red.svg)](https://www.apache.org/licenses/LICENSE-2.0)

`enact` is a Rust-based GitHub Actions workflow emulator, which parses your `.github/workflows/*.yml` files and executes them on your machine — no GitHub runner needed.
Because running arbitrary workflow steps directly on your host can modify system state, we recommend running `enact` inside a container (e.g. Podman or Docker).

## Features

- Full workflow YAML parsing (jobs, steps, matrix strategies, `needs` dependencies)
- GitHub Actions expression language with all 12 built-in functions
- All standard event types (`push`, `pull_request`, `workflow_dispatch`, etc.)
- Job dependency resolution via topological sort with cycle detection
- Matrix expansion with `include`/`exclude` rules
- Built-in emulation of common actions:
  - `actions/checkout`
  - `actions/cache` (save / restore)
  - `actions/upload-artifact` / `actions/download-artifact`
- Composite and Node.js action support
- `GITHUB_OUTPUT`, `GITHUB_ENV`, `GITHUB_PATH` file-based mechanisms

## Quick start

### Prerequisites

- [Podman](https://podman.io/) or [Docker](https://www.docker.com/) (for containerised execution)
- [Rust](https://rust-lang.org/) 1.88.0+ (to build from source)

### Run inside a container (recommended)

> [!INFO]
> Running inside a container isolates workflow side effects from your host system.
> Your repository is mounted **read-only** at `/mnt/repo` inside the container.
> On startup the entrypoint copies it to a writable `/workspace` directory.
> `enact` can create temporary files without ever modifying your local checkout.

```bash
# Build a container image
git clone https://github.com/hyperfinitism/enact-rs
cd enact-rs
podman build -t enact:ubuntu-24.04 -f containers/ubuntu-24.04/Containerfile .
```

```bash
cd /path/to/your-repo

# Run all workflows
podman run --rm -v ./:/mnt/repo:ro,Z enact:ubuntu-24.04 -e push

# Run a specific workflow
podman run --rm -v ./:/mnt/repo:ro,Z enact:ubuntu-24.04 -f .github/workflows/ci.yml -e push
```

### Run directly on your machine

> [!WARNING]
> Workflow steps execute shell commands directly on your host.
> Only use this if you fully trust the workflows you are running.

```bash
git clone https://github.com/hyperfinitism/enact-rs
cd enact-rs
cargo build --release
# Binary: ./target/release/enact[.exe]
```

```bash
cd /path/to/your-repo
enact -w . -f .github/workflows/ci.yml -e push
```

## Usage

```bash
# Run inside a container
podman run --rm -v ./:/mnt/repo:ro,Z enact:<image-name> [OPTIONS]

# Run directly on your machine
enact [OPTIONS]
```

### Options

| Option | Description | Default |
| ------ | ----------- | ------- |
| `-w, --workspace <PATH>` | Path to the repository root | `.` |
| `-f, --workflow <FILE>` | Workflow file  (auto-discovers from `.github/workflows/` if omitted) | *all* |
| `-e, --event <EVENT>` | Event type (`push`, `pull_request`, `workflow_dispatch`, ...) | `push` |
| `--event-file <FILE>` | Custom `event.json` payload | |
| `-j, --job <JOB>` | Run a specific job only | |
| `--env <KEY=VALUE>` | Set environment variable (repeatable) | |
| `-s, --secret <KEY=VALUE>` | Set secret (repeatable) | |
| `--default-shell <SHELL>` | Default shell for `run:` steps | `bash` |
| `--runner-temp <PATH>` | Runner temp directory | `/tmp/enact/runner` |
| `--actions-cache <PATH>` | Actions cache directory | `/tmp/enact/actions-cache` |
| `-v, --verbosity <LEVEL>` | Log level (`Trace`, `Debug`, `Info`, `Warn`, `Error`, `Off`) | `Info` |
| `-l, --log-file <FILE>` | Write logs to a file | |

### Available container images

| Image | Base |
| ----- | ---- |
| `containers/ubuntu-24.04/Containerfile` | Ubuntu 24.04 (Noble) |
| `containers/ubuntu-26.04/Containerfile` | Ubuntu 26.04 (Resolute) |
