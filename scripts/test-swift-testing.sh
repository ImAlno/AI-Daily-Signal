#!/bin/bash
set -euo pipefail

repository_root="$(cd "$(dirname "$0")/.." && pwd -P)"
package="$repository_root/apps/macos"
clt_frameworks="/Library/Developer/CommandLineTools/Library/Developer/Frameworks"
clt_libraries="/Library/Developer/CommandLineTools/Library/Developer/usr/lib"
test_root="$(mktemp -d)"
test_output="$test_root/swift-testing.out"

cleanup() {
  rm -rf "$test_root"
}
trap cleanup EXIT

cd "$repository_root"

if [[ "$(xcode-select -p)" == '/Library/Developer/CommandLineTools' \
  && -d "$clt_frameworks/Testing.framework" ]]; then
  # SwiftPM under Command Line Tools compiles Swift Testing sources but currently creates a bundle
  # runner without importing Testing, so it reports zero executed tests. Recompile SwiftPM's
  # generated runner with the shipped framework and link the same object list as an executable.
  SIGNAL_SWIFT_CLT_TESTING=1 swift test --package-path "$package" >"$test_root/build.out" 2>&1
  bin_path="$(SIGNAL_SWIFT_CLT_TESTING=1 swift build --package-path "$package" \
    -c debug --show-bin-path)"
  derived_runner="$bin_path/AIDailySignalMacPackageTests.derived/runner.swift"
  link_list="$bin_path/AIDailySignalMacPackageTests.product/Objects.LinkFileList"
  rebuilt_runner="$test_root/runner.swift.o"
  executable="$test_root/AIDailySignalMacPackageTests"
  filtered_link_list="$test_root/Objects.LinkFileList"

  [[ -f "$derived_runner" && -f "$link_list" ]] || {
    echo "SwiftPM did not produce the expected test runner inputs" >&2
    sed -n '1,160p' "$test_root/build.out" >&2
    exit 1
  }

  xcrun swiftc -parse-as-library \
    -module-name AIDailySignalMacPackageTests \
    -F "$clt_frameworks" \
    -c "$derived_runner" \
    -o "$rebuilt_runner"
  grep -v '/runner\.swift\.o$' "$link_list" >"$filtered_link_list"
  printf '%s\n' "$rebuilt_runner" >>"$filtered_link_list"
  xcrun swiftc \
    -o "$executable" \
    "@$filtered_link_list" \
    -L "$repository_root/target/release" \
    -lsignal_ffi \
    -F "$clt_frameworks" \
    -Xlinker -rpath -Xlinker "$clt_frameworks" \
    -Xlinker -rpath -Xlinker "$clt_libraries"
  "$executable" --testing-library swift-testing 2>&1 | tee "$test_output"
else
  swift test --package-path "$package" 2>&1 | tee "$test_output"
fi

grep -Eq 'Test run with [1-9][0-9]* tests? in [1-9][0-9]* suites? passed' "$test_output" || {
  echo "Swift Testing did not report a nonzero passing test count" >&2
  exit 1
}
[[ "$(grep -Ec 'Test completePersonalAlphaFlowIsReachableThroughTheAppModel\(\) passed' \
  "$test_output")" -eq 1 ]] || {
  echo "AlphaAcceptanceTests did not execute exactly once" >&2
  exit 1
}
grep -Fq 'Suite AlphaAcceptanceTests passed' "$test_output" || {
  echo "AlphaAcceptanceTests suite did not pass" >&2
  exit 1
}

echo "verified nonzero Swift Testing execution including AlphaAcceptanceTests"
