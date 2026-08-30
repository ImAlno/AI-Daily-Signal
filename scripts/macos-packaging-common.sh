#!/bin/bash

# Keep these alternatives specific and case-sensitive. A generic "SENTINEL" match would reject
# harmless test prose while still making it easy to miss the credential/provider-body variants that
# matter at this trust boundary.
credential_provider_sentinel_pattern='SIGNAL_TEST_CREDENTIAL|SIGNAL_TEST_PROVIDER_BODY|credential-contract-secret|provider-body-sentinel|provider-body-SENTINEL|swift-model-secret-SENTINEL|app-model-secret-SENTINEL|alpha-acceptance-secret'

# Reject checkout/build locations while allowing source paths from the Rust and Swift dependency
# caches. The repository name is deliberately not part of this expression.
forbidden_build_path_pattern='/(Users|home|private/tmp|var/folders)/[^[:cntrl:]]*/(apps/macos|target/(debug|release)|\.worktrees/[^/[:cntrl:]]+)'

# Return 0 for a match, 1 for a clean scan, and 2 for an input/find/grep error.
binary_scan_tree() {
  local pattern="$1"
  shift
  local listing
  local input
  local file
  local status

  [[ "$#" -gt 0 ]] || return 2
  listing="$(mktemp)" || return 2

  for input in "$@"; do
    if [[ -f "$input" ]]; then
      printf '%s\0' "$input" >>"$listing" || {
        rm -f "$listing"
        return 2
      }
    elif [[ -d "$input" ]]; then
      find -P "$input" -type f -print0 >>"$listing" || {
        rm -f "$listing"
        return 2
      }
    else
      rm -f "$listing"
      return 2
    fi
  done

  while IFS= read -r -d '' file; do
    status=0
    LC_ALL=C grep -aEq -- "$pattern" "$file" || status=$?
    case "$status" in
      0)
        rm -f "$listing"
        return 0
        ;;
      1) ;;
      *)
        rm -f "$listing"
        return 2
        ;;
    esac
  done <"$listing"

  rm -f "$listing"
  return 1
}

prepare_exact_bundle_parent() {
  local repository="$1"
  local bundle="$2"
  local repository_physical
  local expected_parent
  local parent_physical

  [[ -d "$repository" ]] || return 1
  repository_physical="$(cd -P "$repository" && pwd -P)" || return 1
  expected_parent="$repository_physical/target/macos"
  [[ "$bundle" == "$expected_parent/AI Daily Signal.app" ]] || return 1

  [[ ! -L "$repository_physical/target" ]] || return 1
  if [[ ! -e "$repository_physical/target" ]]; then
    mkdir "$repository_physical/target" || return 1
  fi
  [[ -d "$repository_physical/target" && ! -L "$repository_physical/target" ]] || return 1

  [[ ! -L "$expected_parent" ]] || return 1
  if [[ ! -e "$expected_parent" ]]; then
    mkdir "$expected_parent" || return 1
  fi
  [[ -d "$expected_parent" && ! -L "$expected_parent" ]] || return 1

  parent_physical="$(cd -P "$expected_parent" && pwd -P)" || return 1
  [[ "$parent_physical" == "$expected_parent" ]] || return 1
  [[ "$(basename "$bundle")" == 'AI Daily Signal.app' ]] || return 1
}

delete_exact_bundle() {
  local repository="$1"
  local bundle="$2"

  prepare_exact_bundle_parent "$repository" "$bundle" || return 1
  [[ ! -L "$bundle" ]] || return 1
  if [[ -e "$bundle" ]]; then
    [[ -d "$bundle" ]] || return 1
    rm -rf -- "$bundle"
  fi
}

verify_exact_bundle_layout() {
  local bundle="$1"
  local entry
  local relative

  [[ -d "$bundle" && ! -L "$bundle" ]] || return 1
  if [[ -d "$bundle/Contents/_CodeSignature" \
    && ! -f "$bundle/Contents/_CodeSignature/CodeResources" ]]; then
    return 1
  fi
  if find -P "$bundle" -type l -print -quit | grep -q .; then
    return 1
  fi
  if find -P "$bundle" ! -type d ! -type f ! -type l -print -quit | grep -q .; then
    return 1
  fi

  while IFS= read -r entry; do
    relative="${entry#"$bundle"}"
    relative="${relative#/}"
    case "$relative" in
      ''|Contents|Contents/MacOS|Contents/Frameworks|Contents/Resources|Contents/_CodeSignature) ;;
      *) return 1 ;;
    esac
  done < <(find -P "$bundle" -type d -print)

  while IFS= read -r entry; do
    relative="${entry#"$bundle"/}"
    case "$relative" in
      'Contents/Info.plist'|\
      'Contents/MacOS/AI Daily Signal'|\
      'Contents/Frameworks/libsignal_ffi.dylib'|\
      'Contents/Resources/AppIcon.png'|\
      'Contents/_CodeSignature/CodeResources') ;;
      *) return 1 ;;
    esac
  done < <(find -P "$bundle" -type f -print)

  return 0
}

verify_macho_contract() {
  local executable="$1"
  local dylib="$2"
  local executable_dependencies
  local dylib_dependencies
  local dependency
  local ffi_count=0
  local first_dylib_entry=1
  local executable_rpaths
  local dylib_rpaths

  [[ "$(lipo -archs "$executable" 2>/dev/null)" == 'arm64' ]] || return 1
  [[ "$(lipo -archs "$dylib" 2>/dev/null)" == 'arm64' ]] || return 1
  file "$executable" | grep -Fq 'Mach-O 64-bit executable arm64' || return 1
  file "$dylib" | grep -Fq 'Mach-O 64-bit dynamically linked shared library arm64' || return 1

  executable_dependencies="$(otool -L "$executable")" || return 1
  while IFS= read -r dependency; do
    [[ -n "$dependency" ]] || continue
    if [[ "$dependency" == '@rpath/libsignal_ffi.dylib' ]]; then
      ffi_count=$((ffi_count + 1))
    else
      case "$dependency" in
        /System/Library/*|/usr/lib/*) ;;
        *) return 1 ;;
      esac
    fi
  done < <(printf '%s\n' "$executable_dependencies" | tail -n +2 | awk '{ print $1 }')
  [[ "$ffi_count" -eq 1 ]] || return 1

  [[ "$(otool -D "$dylib" 2>/dev/null | tail -n 1)" == '@rpath/libsignal_ffi.dylib' ]] \
    || return 1
  dylib_dependencies="$(otool -L "$dylib")" || return 1
  while IFS= read -r dependency; do
    [[ -n "$dependency" ]] || continue
    if [[ "$first_dylib_entry" -eq 1 ]]; then
      first_dylib_entry=0
      [[ "$dependency" == '@rpath/libsignal_ffi.dylib' ]] || return 1
    else
      case "$dependency" in
        /System/Library/*|/usr/lib/*) ;;
        *) return 1 ;;
      esac
    fi
  done < <(printf '%s\n' "$dylib_dependencies" | tail -n +2 | awk '{ print $1 }')
  [[ "$first_dylib_entry" -eq 0 ]] || return 1

  executable_rpaths="$(otool -l "$executable" \
    | awk '$1 == "cmd" && $2 == "LC_RPATH" { getline; getline; print $2 }')" || return 1
  [[ "$executable_rpaths" == '@executable_path/../Frameworks' ]] || return 1
  dylib_rpaths="$(otool -l "$dylib" \
    | awk '$1 == "cmd" && $2 == "LC_RPATH" { getline; getline; print $2 }')" || return 1
  [[ -z "$dylib_rpaths" ]] || return 1
}
