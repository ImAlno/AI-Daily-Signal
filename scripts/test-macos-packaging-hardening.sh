#!/bin/bash
set -euo pipefail

repository_root="$(cd "$(dirname "$0")/.." && pwd -P)"
# shellcheck source=scripts/macos-packaging-common.sh
source "$repository_root/scripts/macos-packaging-common.sh"

test_root="$(mktemp -d)"
cleanup() {
  rm -rf "$test_root"
}
trap cleanup EXIT

fail() {
  echo "macOS packaging hardening test failed: $*" >&2
  exit 1
}

expect_status() {
  local expected="$1"
  shift
  local actual=0
  "$@" >/dev/null 2>&1 || actual=$?
  [[ "$actual" -eq "$expected" ]] || fail "expected status $expected, got $actual from: $*"
}

mkdir -p "$test_root/scans"
printf 'clean\0ordinary-SENTINEL\0text' >"$test_root/scans/clean.bin"
printf 'prefix\0SIGNAL_TEST_CREDENTIAL\0suffix' >"$test_root/scans/credential.bin"
printf 'prefix\0SIGNAL_TEST_PROVIDER_BODY\0suffix' >"$test_root/scans/provider-body.bin"
printf 'prefix\0/Users/reviewer/build/AI-Daily-Signal/apps/macos/Sources\0suffix' \
  >"$test_root/scans/checkout-path.bin"

expect_status 1 binary_scan_tree "$credential_provider_sentinel_pattern" \
  "$test_root/scans/clean.bin"
expect_status 0 binary_scan_tree "$credential_provider_sentinel_pattern" \
  "$test_root/scans/credential.bin"
expect_status 0 binary_scan_tree "$credential_provider_sentinel_pattern" \
  "$test_root/scans/provider-body.bin"
expect_status 0 binary_scan_tree "$forbidden_build_path_pattern" \
  "$test_root/scans/checkout-path.bin"
expect_status 2 binary_scan_tree "$credential_provider_sentinel_pattern" \
  "$test_root/scans/does-not-exist"

safe_repository="$test_root/safe-repository"
mkdir -p "$safe_repository"
safe_repository="$(cd -P "$safe_repository" && pwd -P)"
safe_bundle="$safe_repository/target/macos/AI Daily Signal.app"
prepare_exact_bundle_parent "$safe_repository" "$safe_bundle"
mkdir -p "$safe_bundle"
printf 'replace me' >"$safe_bundle/marker"
delete_exact_bundle "$safe_repository" "$safe_bundle"
[[ ! -e "$safe_bundle" ]] || fail "exact child was not deleted"

external="$test_root/external"
redirected_repository="$test_root/redirected-repository"
mkdir -p "$external" "$redirected_repository/target"
external="$(cd -P "$external" && pwd -P)"
redirected_repository="$(cd -P "$redirected_repository" && pwd -P)"
printf 'must survive' >"$external/marker"
ln -s "$external" "$redirected_repository/target/macos"
redirected_bundle="$redirected_repository/target/macos/AI Daily Signal.app"
expect_status 1 delete_exact_bundle "$redirected_repository" "$redirected_bundle"
[[ "$(<"$external/marker")" == "must survive" ]] || fail "external marker was modified"

layout="$test_root/layout/AI Daily Signal.app"
mkdir -p "$layout/Contents/MacOS" "$layout/Contents/Frameworks" \
  "$layout/Contents/Resources"
touch "$layout/Contents/MacOS/AI Daily Signal" \
  "$layout/Contents/Frameworks/libsignal_ffi.dylib" \
  "$layout/Contents/Resources/AppIcon.png" \
  "$layout/Contents/Info.plist"
verify_exact_bundle_layout "$layout"

mkdir "$layout/Contents/_CodeSignature"
expect_status 1 verify_exact_bundle_layout "$layout"
touch "$layout/Contents/_CodeSignature/CodeResources"
verify_exact_bundle_layout "$layout"
rm "$layout/Contents/_CodeSignature/CodeResources"
rmdir "$layout/Contents/_CodeSignature"

touch "$layout/Contents/Resources/unexpected.txt"
expect_status 1 verify_exact_bundle_layout "$layout"
rm "$layout/Contents/Resources/unexpected.txt"
ln -s ../Info.plist "$layout/Contents/Resources/alias"
expect_status 1 verify_exact_bundle_layout "$layout"
rm "$layout/Contents/Resources/alias"
mkdir -p "$layout/Contents/Helpers"
touch "$layout/Contents/Helpers/signal"
expect_status 1 verify_exact_bundle_layout "$layout"

plist="$repository_root/apps/macos/Resources/Info.plist"
[[ "$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIconFile' "$plist")" == 'AppIcon.png' ]] \
  || fail "CFBundleIconFile must include the exact PNG filename"
[[ "$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$plist")" == '0.1.0' ]] \
  || fail "CFBundleShortVersionString must be numeric"

[[ -x "$repository_root/scripts/test-swift-testing.sh" ]] \
  || fail "missing executable committed Swift Testing runner"

echo "macOS packaging hardening adversarial tests passed"
