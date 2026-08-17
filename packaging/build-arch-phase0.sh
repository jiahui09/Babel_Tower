#!/usr/bin/env bash
set -euo pipefail
repo_root=$(cd "$(dirname "$0")/.." && pwd)
release_dir="$repo_root/release/arch"
mkdir -p "$release_dir"
cargo build --release -p babel-phase3 -p babel-txt-worker
cp "$repo_root/target/release/babel-phase3" "$repo_root/packaging/arch/babel-phase3"
cp "$repo_root/target/release/babel-txt-worker" "$repo_root/packaging/arch/babel-txt-worker"
export BABEL_PHASE3_SHA256
export BABEL_TXT_WORKER_SHA256
BABEL_PHASE3_SHA256=$(sha256sum "$repo_root/packaging/arch/babel-phase3" | cut -d' ' -f1)
BABEL_TXT_WORKER_SHA256=$(sha256sum "$repo_root/packaging/arch/babel-txt-worker" | cut -d' ' -f1)
rm -f "$repo_root"/packaging/arch/*.pkg.tar.zst
(
  cd "$repo_root/packaging/arch"
  makepkg --cleanbuild --clean --force --nodeps
)
package=$(find "$repo_root/packaging/arch" -maxdepth 1 -name '*.pkg.tar.zst' -print -quit)
rm -f "$release_dir"/*.pkg.tar.zst
cp "$package" "$release_dir/"
artifact_name=$(basename "$package")
artifact_hash=$(sha256sum "$release_dir/$artifact_name" | cut -d' ' -f1)
jq -n --arg artifact "$artifact_name" --arg hash "$artifact_hash" '{
  schema_version: 1,
  scope: "phase3-txt-vertical-slice",
  runner_kind: "linux-local-build",
  platform: "linux-arch-x86_64",
  artifact: $artifact,
  artifact_sha256: $hash,
  bundled_components: ["core-service", "txt-worker", "txt-adapter", "licenses", "sbom"],
  packaged_dependencies: [],
  allowed_external_dependencies: ["libgcc_s.so.1", "libm.so.6", "libc.so.6"],
  static_dependencies: [],
  runtime_dependencies: ["libgcc_s.so.1", "libm.so.6", "libc.so.6"],
  clean_image: {performed:false, network_blocked_before_install:false, installed:false, launched:false, component_probes:[], network_attempts:0}
}' > "$release_dir/release-manifest.json"
if [[ "${BABEL_SKIP_ARCH_CLEAN_IMAGE:-0}" != "1" ]]; then
  "$repo_root/packaging/verify-arch-phase0.sh"
fi
