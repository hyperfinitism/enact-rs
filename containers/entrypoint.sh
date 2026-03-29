#!/bin/bash
# SPDX-License-Identifier: Apache-2.0
set -euo pipefail

# If /mnt/repo has content (read-only mount), copy it into the writable workspace.
echo "[entrypoint] checking /mnt/repo ..." >&2
ls -la /mnt/repo/ >&2 || true
if [ -d /mnt/repo ] && [ "$(ls -A /mnt/repo 2>/dev/null)" ]; then
    echo "[entrypoint] copying /mnt/repo -> /workspace ..." >&2
    cp -a /mnt/repo/. /workspace/
    echo "[entrypoint] copy done, workspace contents:" >&2
    ls -la /workspace/ >&2 || true
else
    echo "[entrypoint] /mnt/repo is empty or missing, skipping copy" >&2
fi

exec enact --workspace /workspace "$@"
