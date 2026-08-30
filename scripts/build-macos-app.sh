#!/bin/bash
set -euo pipefail

repository_root="$(cd "$(dirname "$0")/.." && pwd -P)"
bundle="$repository_root/target/macos/AI Daily Signal.app"
expected_bundle="$repository_root/target/macos/AI Daily Signal.app"

[[ "$bundle" == "$expected_bundle" ]] || {
  echo "refusing to assemble an unexpected bundle path: $bundle" >&2
  exit 1
}

cd "$repository_root"
scripts/generate-swift-bindings.sh
cargo build -p signal-ffi --release
swift build --package-path apps/macos -c release

swift_binary="$repository_root/apps/macos/.build/release/SignalMacApp"
rust_dylib="$repository_root/target/release/libsignal_ffi.dylib"
plist_source="$repository_root/apps/macos/Resources/Info.plist"
icon_source="$repository_root/apps/macos/Resources/AppIcon.png"

for source in "$swift_binary" "$rust_dylib" "$plist_source" "$icon_source"; do
  [[ -f "$source" ]] || {
    echo "missing bundle input: $source" >&2
    exit 1
  }
done

rm -rf "$bundle"
mkdir -p \
  "$bundle/Contents/MacOS" \
  "$bundle/Contents/Frameworks" \
  "$bundle/Contents/Resources"

cp "$swift_binary" "$bundle/Contents/MacOS/AI Daily Signal"
cp "$rust_dylib" "$bundle/Contents/Frameworks/libsignal_ffi.dylib"
cp "$plist_source" "$bundle/Contents/Info.plist"
cp "$icon_source" "$bundle/Contents/Resources/AppIcon.png"
chmod 755 "$bundle/Contents/MacOS/AI Daily Signal"

bundled_executable="$bundle/Contents/MacOS/AI Daily Signal"
bundled_dylib="$bundle/Contents/Frameworks/libsignal_ffi.dylib"
linked_dylib="$(otool -L "$bundled_executable" | awk '/libsignal_ffi\.dylib/ { print $1; exit }')"
[[ -n "$linked_dylib" ]] || {
  echo "Swift executable does not link libsignal_ffi.dylib" >&2
  exit 1
}

install_name_tool -id '@rpath/libsignal_ffi.dylib' "$bundled_dylib"
if [[ "$linked_dylib" != '@rpath/libsignal_ffi.dylib' ]]; then
  install_name_tool -change "$linked_dylib" '@rpath/libsignal_ffi.dylib' "$bundled_executable"
fi

if ! codesign --verify --deep --strict "$bundle" >/dev/null 2>&1; then
  codesign --force --sign - "$bundled_dylib"
  codesign --force --sign - --deep "$bundle"
fi

echo "assembled standalone app: $bundle"
