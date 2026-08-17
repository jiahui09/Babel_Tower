#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "$0")/.." && pwd)
status=0

check_json() {
  local file=$1
  if [[ ! -s "$file" ]]; then
    printf 'MISSING %s\n' "${file#$repo_root/}"
    status=1
    return
  fi
  jq empty "$file" >/dev/null
}

s_corpus="$repo_root/.omx/phase3/s-corpus.json"
check_json "$s_corpus"
if [[ -s "$s_corpus" ]]; then
  if ! jq -e 'has("cold_open_ms") and (.thresholds | has("cold_open_ms"))' "$s_corpus" >/dev/null; then
    printf 'FAIL %s: S corpus gate must record cold_open_ms and thresholds.cold_open_ms\n' "${s_corpus#$repo_root/}"
    status=1
  fi
  if jq -e '.thresholds | has("import_ms")' "$s_corpus" >/dev/null; then
    printf 'FAIL %s: thresholds.import_ms is stale for Phase 3 S corpus gating\n' "${s_corpus#$repo_root/}"
    status=1
  fi
fi

for manifest in "$repo_root"/release/*/release-manifest.json; do
  [[ -e "$manifest" ]] || continue
  check_json "$manifest"
  platform=$(jq -r '.platform' "$manifest")
  scope=$(jq -r '.scope' "$manifest")
  if [[ "$scope" != "phase3-txt-vertical-slice" ]]; then
    printf 'FAIL %s: scope must be phase3-txt-vertical-slice\n' "${manifest#$repo_root/}"
    status=1
  fi
  artifact=$(jq -r '.artifact' "$manifest")
  expected_hash=$(jq -r '.artifact_sha256' "$manifest")
  artifact_path=$(dirname "$manifest")/$artifact
  if [[ ! -s "$artifact_path" ]] || [[ "$(sha256sum "$artifact_path" | cut -d' ' -f1)" != "$expected_hash" ]]; then
    printf 'FAIL %s: packaged artifact is missing or its SHA-256 does not match\n' "${manifest#$repo_root/}"
    status=1
  fi
  if [[ "$platform" == linux-* ]]; then
    jq -e '
      .clean_image.performed == true and
      .clean_image.network_blocked_before_install == true and
      .clean_image.installed == true and
      .clean_image.launched == true and
      (.clean_image.component_probes | index("core-service")) and
      (.clean_image.component_probes | index("txt-worker")) and
      (.clean_image.component_probes | index("txt-adapter")) and
      .clean_image.network_attempts == 0
    ' "$manifest" >/dev/null || {
      printf 'FAIL %s: Arch manifest must contain offline clean-image core/txt-worker/txt-adapter probes\n' "${manifest#$repo_root/}"
      status=1
    }
  fi
  if [[ "$platform" == windows-* ]]; then
    runner_kind=$(jq -r '.runner_kind // ""' "$manifest")
    if [[ "$runner_kind" == "linux-cross-build" ]]; then
      if jq -e '.clean_image.performed == true or .clean_image.installed == true or .clean_image.launched == true' "$manifest" >/dev/null; then
        printf 'FAIL %s: linux-cross-build Windows manifest must not claim native clean-image verification\n' "${manifest#$repo_root/}"
        status=1
      fi
    else
      jq -e '
        .runner_kind == "windows-native" and
        .clean_image.performed == true and
        .clean_image.network_blocked_before_install == true and
        .clean_image.installed == true and
        .clean_image.launched == true
      ' "$manifest" >/dev/null || {
        printf 'FAIL %s: Windows support requires native offline clean-image evidence\n' "${manifest#$repo_root/}"
        status=1
      }
    fi
  fi
done

for platform_dir in arch windows; do
  manifest="$repo_root/release/$platform_dir/release-manifest.json"
  if [[ ! -s "$manifest" ]]; then
    printf 'FAIL %s: required Phase 3 platform release manifest is missing\n' "${manifest#$repo_root/}"
    status=1
  fi
done

if [[ -s "$repo_root/release/windows/pe-build-manifest.json" && ! -s "$repo_root/release/windows/release-manifest.json" ]]; then
  printf 'FAIL release/windows: PE binaries exist but the Windows installer is still pending\n'
  status=1
fi

if [[ "$status" -eq 0 ]]; then
  printf 'Phase 3 evidence audit passed\n'
fi
exit "$status"
