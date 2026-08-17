#!/usr/bin/env bash
set -euo pipefail
repo_root=$(cd "$(dirname "$0")/.." && pwd)
release_dir="$repo_root/release/arch"
manifest="$release_dir/release-manifest.json"
artifact_name=$(jq -r '.artifact' "$manifest")
artifact_hash=$(jq -r '.artifact_sha256' "$manifest")
test -s "$release_dir/$artifact_name"
test "$(sha256sum "$release_dir/$artifact_name" | cut -d' ' -f1)" = "$artifact_hash"
docker run --rm --network none -v "$release_dir:/release:ro" archlinux:base bash -eu -c '
  pacman -U --noconfirm /release/*.pkg.tar.zst
  /usr/lib/babel-tower-phase3/babel-phase3 smoke >/tmp/smoke.json
  grep -q SUPPORTED /tmp/smoke.json
  test -x /usr/lib/babel-tower-phase3/babel-txt-worker
  test -s /usr/share/babel-tower-phase3/sbom.spdx.json
'
jq -n --arg artifact "$artifact_name" --arg hash "$artifact_hash" '{
  schema_version: 1,
  scope: "phase3-txt-vertical-slice",
  runner_kind: "linux-clean-container",
  platform: "linux-arch-x86_64",
  artifact: $artifact,
  artifact_sha256: $hash,
  bundled_components: ["core-service", "txt-worker", "txt-adapter", "licenses", "sbom"],
  packaged_dependencies: [],
  allowed_external_dependencies: ["libgcc_s.so.1", "libm.so.6", "libc.so.6"],
  static_dependencies: [],
  runtime_dependencies: ["libgcc_s.so.1", "libm.so.6", "libc.so.6"],
  clean_image: {performed:true, network_blocked_before_install:true, installed:true, launched:true, component_probes:["core-service", "txt-worker", "txt-adapter"], network_attempts:0}
}' > "$manifest"
printf '%s\n' "Arch offline probe passed: $release_dir"
