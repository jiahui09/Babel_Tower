#!/usr/bin/env bash
set -euo pipefail
repo_root=$(cd "$(dirname "$0")/.." && pwd)
workspace_uid=$(stat -c %u "$repo_root")
workspace_gid=$(stat -c %g "$repo_root")
docker run --rm \
  -v "$repo_root:/workspace" \
  -w /workspace \
  archlinux:base-devel \
  bash -euxo pipefail -c '
    pacman -Syu --noconfirm rust cargo jq
    groupadd --gid '"$workspace_gid"' babel-builder
    useradd --uid '"$workspace_uid"' --gid '"$workspace_gid"' --create-home babel-builder
    install -d -o '"$workspace_uid"' -g '"$workspace_gid"' /tmp/babel-cargo
    runuser -u babel-builder -- env \
      CARGO_HOME=/tmp/babel-cargo \
      BABEL_SKIP_ARCH_CLEAN_IMAGE=1 \
      ./packaging/build-arch-phase0.sh
  '
"$repo_root/packaging/verify-arch-phase0.sh"
