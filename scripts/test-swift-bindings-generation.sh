#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

stale_swift="apps/macos/Generated/Swift/ObsoleteBindings.swift"
stale_c="apps/macos/Generated/CSignalFFI/ObsoleteBindings.h"
mkdir -p "$(dirname "$stale_swift")" "$(dirname "$stale_c")"
touch "$stale_swift" "$stale_c"
trap 'rm -f "$stale_swift" "$stale_c"' EXIT

scripts/generate-swift-bindings.sh

if [[ -e "$stale_swift" || -e "$stale_c" ]]; then
  echo "obsolete generated binding survived regeneration" >&2
  exit 1
fi

test -f apps/macos/Generated/Swift/SignalFFIBindings.swift
test -f apps/macos/Generated/CSignalFFI/CSignalFFI.h
test -f apps/macos/Generated/CSignalFFI/module.modulemap
