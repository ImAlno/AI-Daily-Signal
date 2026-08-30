#!/bin/bash
set -euo pipefail

repository_root="$(cd "$(dirname "$0")/.." && pwd -P)"
bundle="$repository_root/target/macos/AI Daily Signal.app"
contents="$bundle/Contents"
executable="$contents/MacOS/AI Daily Signal"
dylib="$contents/Frameworks/libsignal_ffi.dylib"
plist="$contents/Info.plist"
icon="$contents/Resources/AppIcon.png"

fail() {
  echo "bundle verification failed: $*" >&2
  exit 1
}

[[ -d "$bundle" ]] || fail "missing bundle: $bundle"
[[ -f "$plist" ]] || fail "missing Info.plist"
[[ -x "$executable" ]] || fail "missing executable"
[[ -f "$dylib" ]] || fail "missing libsignal_ffi.dylib"
[[ -f "$icon" ]] || fail "missing AppIcon.png"

[[ "$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$plist")" == "com.AIDailySignal.AI-Daily-Signal" ]] || fail "unexpected bundle identifier"
[[ "$(/usr/libexec/PlistBuddy -c 'Print :LSUIElement' "$plist")" == "true" ]] || fail "LSUIElement must be true"
[[ "$(/usr/libexec/PlistBuddy -c 'Print :LSMinimumSystemVersion' "$plist")" == "26.0" ]] || fail "minimum macOS version must be 26.0"

file "$executable" | grep -Eq 'Mach-O 64-bit executable arm64|Mach-O universal binary.*arm64' || fail "executable is not arm64"
file "$dylib" | grep -Eq 'Mach-O 64-bit dynamically linked shared library arm64|Mach-O universal binary.*arm64' || fail "dylib is not arm64"
otool -L "$executable" | grep -Fq '@rpath/libsignal_ffi.dylib' || fail "executable does not link @rpath/libsignal_ffi.dylib"
otool -l "$executable" | grep -A2 LC_RPATH | grep -Fq '@executable_path/../Frameworks' || fail "executable has no Frameworks rpath"
[[ "$(otool -D "$dylib" | tail -n 1)" == '@rpath/libsignal_ffi.dylib' ]] || fail "dylib install name is not @rpath/libsignal_ffi.dylib"

if find "$contents" -type f -print0 | xargs -0 grep -aFIl -- "$repository_root" >/dev/null 2>&1; then
  fail "bundle contains an absolute repository path"
fi
[[ ! -e "$contents/MacOS/signal" && ! -e "$contents/Resources/signal" ]] || fail "bundle contains the optional signal CLI"

sentinel_pattern='SENTINEL|SIGNAL_TEST_CREDENTIAL|SIGNAL_TEST_PROVIDER_BODY|credential-contract-secret|provider-body-sentinel'
if find "$contents" -type f -print0 | xargs -0 grep -aEIil -- "$sentinel_pattern" >/dev/null 2>&1; then
  fail "bundle contains a credential or provider-body sentinel"
fi

echo "verified standalone macOS bundle: $bundle"
