#!/bin/bash
set -euo pipefail

repository_root="$(cd "$(dirname "$0")/.." && pwd -P)"
bundle="$repository_root/target/macos/AI Daily Signal.app"
contents="$bundle/Contents"
executable="$contents/MacOS/AI Daily Signal"
dylib="$contents/Frameworks/libsignal_ffi.dylib"
plist="$contents/Info.plist"
icon="$contents/Resources/AppIcon.png"
icon_source="$repository_root/apps/macos/Resources/AppIcon.png"
# shellcheck source=scripts/macos-packaging-common.sh
source "$repository_root/scripts/macos-packaging-common.sh"

fail() {
  echo "bundle verification failed: $*" >&2
  exit 1
}

plist_value() {
  /usr/libexec/PlistBuddy -c "Print :$1" "$plist"
}

assert_adhoc_signature_if_present() {
  local path="$1"
  local details
  if details="$(codesign -dv --verbose=4 "$path" 2>&1)"; then
    grep -Fq 'Signature=adhoc' <<<"$details" || fail "signature is not ad hoc: $path"
    grep -Fq 'TeamIdentifier=not set' <<<"$details" || fail "signature has a Team identifier: $path"
    codesign --verify --strict "$path" >/dev/null 2>&1 || fail "invalid code signature: $path"
  elif ! grep -Fq 'code object is not signed at all' <<<"$details"; then
    fail "could not validate code-signature state: $path"
  fi
}

[[ -d "$bundle" && ! -L "$bundle" ]] || fail "missing regular bundle: $bundle"
verify_exact_bundle_layout "$bundle" || fail "unexpected file, directory, symlink, or CLI payload"
[[ -f "$plist" && ! -L "$plist" ]] || fail "missing regular Info.plist"
[[ -x "$executable" && ! -L "$executable" ]] || fail "missing regular executable"
[[ -f "$dylib" && ! -L "$dylib" ]] || fail "missing regular libsignal_ffi.dylib"
[[ -f "$icon" && ! -L "$icon" ]] || fail "missing regular AppIcon.png"

plutil -lint "$plist" >/dev/null || fail "invalid Info.plist"
[[ "$(plist_value CFBundleIdentifier)" == 'com.AIDailySignal.AI-Daily-Signal' ]] \
  || fail "unexpected bundle identifier"
[[ "$(plist_value CFBundleExecutable)" == 'AI Daily Signal' ]] \
  || fail "unexpected executable metadata"
[[ "$(plist_value CFBundleIconFile)" == 'AppIcon.png' ]] \
  || fail "unexpected icon metadata"
[[ "$(plist_value CFBundleShortVersionString)" == '0.1.0' ]] \
  || fail "short version must be numeric 0.1.0"
[[ "$(plist_value CFBundleVersion)" == '1' ]] || fail "unexpected bundle version"
[[ "$(plist_value CFBundlePackageType)" == 'APPL' ]] || fail "unexpected package type"
[[ "$(plist_value LSUIElement)" == 'true' ]] || fail "LSUIElement must be true"
[[ "$(plist_value LSMinimumSystemVersion)" == '26.0' ]] \
  || fail "minimum macOS version must be 26.0"

verify_macho_contract "$executable" "$dylib" \
  || fail "architecture, dependency, install-name, or rpath contract failed"

[[ "$(shasum -a 256 "$icon" | awk '{ print $1 }')" \
  == "$(shasum -a 256 "$icon_source" | awk '{ print $1 }')" ]] \
  || fail "bundled icon differs from the approved source icon"
icon_properties="$(sips -g pixelWidth -g pixelHeight -g format -g hasAlpha "$icon" 2>/dev/null)"
grep -Fq 'pixelWidth: 1024' <<<"$icon_properties" || fail "icon width is not 1024"
grep -Fq 'pixelHeight: 1024' <<<"$icon_properties" || fail "icon height is not 1024"
grep -Fq 'format: png' <<<"$icon_properties" || fail "icon is not PNG"
grep -Fq 'hasAlpha: yes' <<<"$icon_properties" || fail "icon is not RGBA"

if [[ -e "$contents/_CodeSignature/CodeResources" ]]; then
  codesign --verify --deep --strict "$bundle" >/dev/null 2>&1 \
    || fail "invalid ad-hoc bundle signature"
fi
assert_adhoc_signature_if_present "$executable"
assert_adhoc_signature_if_present "$dylib"
assert_adhoc_signature_if_present "$bundle"

scan_status=0
binary_scan_tree "$forbidden_build_path_pattern" "$contents" || scan_status=$?
case "$scan_status" in
  0) fail "bundle contains an absolute checkout or build path" ;;
  1) ;;
  *) fail "absolute checkout/build path scan failed" ;;
esac

scan_status=0
binary_scan_tree "$credential_provider_sentinel_pattern" "$contents" || scan_status=$?
case "$scan_status" in
  0) fail "bundle contains a credential or provider-body sentinel" ;;
  1) ;;
  *) fail "credential/provider-body scan failed" ;;
esac

echo "verified exact standalone macOS bundle: $bundle"
