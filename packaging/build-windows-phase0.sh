#!/usr/bin/env bash
set -euo pipefail
repo_root=$(cd "$(dirname "$0")/.." && pwd)
release_dir="$repo_root/release/windows"
mkdir -p "$release_dir"
rm -f "$release_dir"/*.exe "$release_dir/release-manifest.json"
export PATH="$repo_root/.phase0-tools/bin:$PATH"
export XWIN_ACCEPT_LICENSE=1
xwin_environment=$(cargo xwin env --target x86_64-pc-windows-msvc)
eval "$xwin_environment"
export CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS="${CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS:-} -C target-feature=+crt-static"
export CFLAGS_x86_64_pc_windows_msvc="${CFLAGS_x86_64_pc_windows_msvc:-} /MT"
cargo build --release --target x86_64-pc-windows-msvc -p babel-phase3 -p babel-txt-worker
app_hash=$(sha256sum "$repo_root/target/x86_64-pc-windows-msvc/release/babel-phase3.exe" | cut -d' ' -f1)
worker_hash=$(sha256sum "$repo_root/target/x86_64-pc-windows-msvc/release/babel-txt-worker.exe" | cut -d' ' -f1)
jq -n --arg app_hash "$app_hash" --arg worker_hash "$worker_hash" '{
  schema_version: 1,
  scope: "phase3-txt-vertical-slice",
  platform: "windows-x86_64",
  verdict: "PE_BUILT_INSTALLER_PENDING",
  binaries: [
    {artifact:"target/x86_64-pc-windows-msvc/release/babel-phase3.exe", sha256:$app_hash},
    {artifact:"target/x86_64-pc-windows-msvc/release/babel-txt-worker.exe", sha256:$worker_hash}
  ],
  installer_built: false,
  native_validation_performed: false
}' > "$release_dir/pe-build-manifest.json"
rm -f "$repo_root/packaging/windows/babel-tower-phase3-txt-windows-x64.exe"
if command -v makensis >/dev/null 2>&1; then
  (
    cd "$repo_root/packaging/windows"
    makensis \
      "-DAPP_EXE=$repo_root/target/x86_64-pc-windows-msvc/release/babel-phase3.exe" \
      "-DTXT_WORKER_EXE=$repo_root/target/x86_64-pc-windows-msvc/release/babel-txt-worker.exe" \
      phase0-installer.nsi
  )
else
  app_exe=$(winepath -w "$repo_root/target/x86_64-pc-windows-msvc/release/babel-phase3.exe")
  txt_worker_exe=$(winepath -w "$repo_root/target/x86_64-pc-windows-msvc/release/babel-txt-worker.exe")
  installer_script=$(winepath -w "$repo_root/packaging/windows/phase0-installer.nsi")
  WINEDEBUG=-all wine "$HOME/.wine/drive_c/Program Files (x86)/NSIS/Bin/makensis.exe" \
    "/DAPP_EXE=$app_exe" \
    "/DTXT_WORKER_EXE=$txt_worker_exe" \
    "$installer_script"
fi
installer="$repo_root/packaging/windows/babel-tower-phase3-txt-windows-x64.exe"
test -s "$installer"
cp "$installer" "$release_dir/"
artifact_hash=$(sha256sum "$release_dir/babel-tower-phase3-txt-windows-x64.exe" | cut -d' ' -f1)
jq -n --arg hash "$artifact_hash" '{
  schema_version: 1,
  scope: "phase3-txt-vertical-slice",
  runner_kind: "linux-cross-build",
  platform: "windows-x86_64",
  artifact: "babel-tower-phase3-txt-windows-x64.exe",
  artifact_sha256: $hash,
  bundled_components: ["core-service", "txt-worker", "txt-adapter"],
  packaged_dependencies: [],
  allowed_external_dependencies: ["bcryptprimitives.dll", "advapi32.dll", "KERNEL32.dll", "ntdll.dll", "api-ms-win-core-synch-l1-2-0.dll"],
  static_dependencies: [],
  runtime_dependencies: ["bcryptprimitives.dll", "advapi32.dll", "KERNEL32.dll", "ntdll.dll", "api-ms-win-core-synch-l1-2-0.dll"],
  clean_image: {performed:false, network_blocked_before_install:false, installed:false, launched:false, component_probes:[], network_attempts:0},
  windows: {webview_install_mode:"not-applicable-phase3-txt"}
}' > "$release_dir/release-manifest.json"
printf '%s\n' "Windows installer artifact built: $release_dir (installation was not tested on Linux)"
