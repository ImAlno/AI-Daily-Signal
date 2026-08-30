#!/bin/bash
set -euo pipefail

repository_root="$(cd "$(dirname "$0")/.." && pwd -P)"
bundle="$repository_root/target/macos/AI Daily Signal.app"
source_executable="$bundle/Contents/MacOS/AI Daily Signal"
source_dylib="$bundle/Contents/Frameworks/libsignal_ffi.dylib"
test_root="$(mktemp -d)"
# shellcheck source=scripts/macos-packaging-common.sh
source "$repository_root/scripts/macos-packaging-common.sh"

cleanup() {
  rm -rf "$test_root"
}
trap cleanup EXIT

expect_rejected() {
  if verify_macho_contract "$1" "$2" >/dev/null 2>&1; then
    echo "Mach-O verifier accepted an adversarial fixture" >&2
    exit 1
  fi
}

[[ -f "$source_executable" && -f "$source_dylib" ]] || {
  echo "Mach-O adversarial test requires a built app bundle" >&2
  exit 1
}

cp "$source_executable" "$test_root/app"
cp "$source_dylib" "$test_root/libsignal_ffi.dylib"
verify_macho_contract "$test_root/app" "$test_root/libsignal_ffi.dylib"

install_name_tool -add_rpath '@loader_path' "$test_root/app"
expect_rejected "$test_root/app" "$test_root/libsignal_ffi.dylib"

cp "$source_executable" "$test_root/app"
install_name_tool -change '/usr/lib/libobjc.A.dylib' '/tmp/unapproved-review-fixture.dylib' \
  "$test_root/app"
expect_rejected "$test_root/app" "$test_root/libsignal_ffi.dylib"

cp "$source_executable" "$test_root/app"
cp "$source_dylib" "$test_root/libsignal_ffi.dylib"
install_name_tool -add_rpath '/tmp/unapproved-review-rpath' "$test_root/libsignal_ffi.dylib"
expect_rejected "$test_root/app" "$test_root/libsignal_ffi.dylib"

echo "macOS Mach-O verifier adversarial tests passed"
