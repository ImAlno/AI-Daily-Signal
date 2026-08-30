#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

cargo build -p signal-ffi --release

generation_root="$(mktemp -d)"
trap 'rm -rf "$generation_root"' EXIT
first_output="$generation_root/first"
second_output="$generation_root/second"
mkdir -p "$first_output" "$second_output"

generate_bindings() {
  local output_directory="$1"
  cargo run -p signal-ffi --features bindgen-cli --bin uniffi-bindgen -- \
    generate --library --language swift \
    --out-dir "$output_directory" \
    target/release/libsignal_ffi.dylib
}

generate_bindings "$first_output"
generate_bindings "$second_output"
diff -ru "$first_output" "$second_output"

mkdir -p apps/macos/Generated/Swift apps/macos/Generated/CSignalFFI
cp "$first_output/SignalFFIBindings.swift" apps/macos/Generated/Swift/
cp "$first_output/CSignalFFI.h" apps/macos/Generated/CSignalFFI/
cp "$first_output/CSignalFFI.modulemap" apps/macos/Generated/CSignalFFI/module.modulemap
