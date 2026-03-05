#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CRATE="platform-bridge"
FEATURES="${FEATURES:-llama-backend,mlx-backend}"
IOS_DEPLOYMENT_TARGET="${IOS_DEPLOYMENT_TARGET:-13.0}"
TARGETS=(
  "aarch64-apple-ios"
  "aarch64-apple-ios-sim"
  "x86_64-apple-ios"
)

HEADER_OUT="$ROOT_DIR/bindings/ios/MobileAIRuntime.h"
MODULEMAP_OUT="$ROOT_DIR/bindings/ios/module.modulemap"
XCFRAMEWORK_OUT="$ROOT_DIR/bindings/ios/MobileAIRuntime.xcframework"
SIM_UNIVERSAL_DIR="$ROOT_DIR/target/ios-simulator-universal/release"
SIM_UNIVERSAL_LIB="$SIM_UNIVERSAL_DIR/libplatform_bridge.a"
DEVICE_LIB="$ROOT_DIR/target/aarch64-apple-ios/release/libplatform_bridge_packaged.a"
SIM_ARM64_LIB="$ROOT_DIR/target/aarch64-apple-ios-sim/release/libplatform_bridge_packaged.a"
SIM_X86_64_LIB="$ROOT_DIR/target/x86_64-apple-ios/release/libplatform_bridge_packaged.a"
SIM_SLICE_DIR="ios-arm64_x86_64-simulator"
DEVICE_SLICE_DIR="ios-arm64"

mkdir -p "$ROOT_DIR/bindings/ios"

ensure_target() {
  local target="$1"
  if rustup target list --installed | grep -q "^${target}$"; then
    return 0
  fi

  echo "Installing rust target: $target"
  if ! rustup target add "$target"; then
    echo "error: failed to install rust target '$target'." >&2
    echo "Check network access, then run: rustup target add $target" >&2
    exit 1
  fi
}

for target in "${TARGETS[@]}"; do
  ensure_target "$target"
  if [[ "$target" == "aarch64-apple-ios" ]]; then
    deploy_env="IPHONEOS_DEPLOYMENT_TARGET"
  else
    deploy_env="IPHONESIMULATOR_DEPLOYMENT_TARGET"
  fi

  env \
    "$deploy_env=$IOS_DEPLOYMENT_TARGET" \
    CMAKE_OSX_DEPLOYMENT_TARGET="$IOS_DEPLOYMENT_TARGET" \
    CFLAGS="-fno-stack-check ${CFLAGS:-}" \
    CXXFLAGS="-fno-stack-check ${CXXFLAGS:-}" \
    cargo build \
      --release \
      --target "$target" \
      -p "$CRATE" \
      --no-default-features \
      --features "$FEATURES"
done

latest_llama_build_dir() {
  local target="$1"
  local build_root="$ROOT_DIR/target/$target/release/build"
  local newest_dir=""
  local newest_mtime=0
  local candidate
  local mtime

  shopt -s nullglob
  for candidate in "$build_root"/llama-cpp-sys-2-*; do
    [[ -d "$candidate/out/build" ]] || continue
    mtime="$(stat -f %m "$candidate" 2>/dev/null || echo 0)"
    if (( mtime > newest_mtime )); then
      newest_mtime="$mtime"
      newest_dir="$candidate"
    fi
  done
  shopt -u nullglob

  if [[ -n "$newest_dir" ]]; then
    printf '%s\n' "$newest_dir"
  fi
}

package_target_archive() {
  local target="$1"
  local base_lib="$ROOT_DIR/target/$target/release/libplatform_bridge.a"
  local packaged_lib="$ROOT_DIR/target/$target/release/libplatform_bridge_packaged.a"
  local llama_dir
  local httplib_lib=""

  if [[ ! -f "$base_lib" ]]; then
    echo "error: missing base archive for $target at $base_lib" >&2
    exit 1
  fi

  llama_dir="$(latest_llama_build_dir "$target" || true)"
  if [[ -n "$llama_dir" ]]; then
    httplib_lib="$llama_dir/out/build/vendor/cpp-httplib/libcpp-httplib.a"
  fi

  if [[ ! -f "$httplib_lib" ]]; then
    cp "$base_lib" "$packaged_lib"
    return 0
  fi

  echo "Packaging $target archive with cpp-httplib from: $httplib_lib"
  libtool -static -o "$packaged_lib" "$base_lib" "$httplib_lib"
}

for target in "${TARGETS[@]}"; do
  package_target_archive "$target"
done

if command -v cbindgen >/dev/null 2>&1; then
  cbindgen \
    --config "$ROOT_DIR/crates/platform-bridge/cbindgen.toml" \
    --crate "$CRATE" \
    --output "$HEADER_OUT"
else
  if [[ ! -f "$HEADER_OUT" ]]; then
    echo "error: cbindgen is not installed and $HEADER_OUT does not exist" >&2
    echo "install with: cargo install cbindgen" >&2
    exit 1
  fi
  echo "reusing existing header: $HEADER_OUT"
fi

cat > "$MODULEMAP_OUT" <<'MMEOF'
module MobileAIRuntimeFFI {
  header "MobileAIRuntime.h"
  export *
}
MMEOF

mkdir -p "$SIM_UNIVERSAL_DIR"
lipo -create \
  "$SIM_ARM64_LIB" \
  "$SIM_X86_64_LIB" \
  -output "$SIM_UNIVERSAL_LIB"

copy_headers() {
  local headers_dir="$1"
  mkdir -p "$headers_dir"
  cp "$HEADER_OUT" "$headers_dir/MobileAIRuntime.h"
  cp "$MODULEMAP_OUT" "$headers_dir/module.modulemap"
  if [[ -f "$ROOT_DIR/bindings/ios/MobileAIRuntime.swift" ]]; then
    cp "$ROOT_DIR/bindings/ios/MobileAIRuntime.swift" "$headers_dir/MobileAIRuntime.swift"
  fi
  if [[ -f "$ROOT_DIR/bindings/ios/README.md" ]]; then
    cp "$ROOT_DIR/bindings/ios/README.md" "$headers_dir/README.md"
  fi
}

write_xcframework_plist() {
  cat > "$XCFRAMEWORK_OUT/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "https://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>AvailableLibraries</key>
  <array>
    <dict>
      <key>BinaryPath</key>
      <string>libplatform_bridge.a</string>
      <key>HeadersPath</key>
      <string>Headers</string>
      <key>LibraryIdentifier</key>
      <string>$SIM_SLICE_DIR</string>
      <key>LibraryPath</key>
      <string>libplatform_bridge.a</string>
      <key>SupportedArchitectures</key>
      <array>
        <string>arm64</string>
        <string>x86_64</string>
      </array>
      <key>SupportedPlatform</key>
      <string>ios</string>
      <key>SupportedPlatformVariant</key>
      <string>simulator</string>
    </dict>
    <dict>
      <key>BinaryPath</key>
      <string>libplatform_bridge.a</string>
      <key>HeadersPath</key>
      <string>Headers</string>
      <key>LibraryIdentifier</key>
      <string>$DEVICE_SLICE_DIR</string>
      <key>LibraryPath</key>
      <string>libplatform_bridge.a</string>
      <key>SupportedArchitectures</key>
      <array>
        <string>arm64</string>
      </array>
      <key>SupportedPlatform</key>
      <string>ios</string>
    </dict>
  </array>
  <key>CFBundlePackageType</key>
  <string>XFWK</string>
  <key>XCFrameworkFormatVersion</key>
  <string>1.0</string>
</dict>
</plist>
PLIST
}

build_xcframework_manually() {
  rm -rf "$XCFRAMEWORK_OUT"
  mkdir -p "$XCFRAMEWORK_OUT/$SIM_SLICE_DIR" "$XCFRAMEWORK_OUT/$DEVICE_SLICE_DIR"
  cp "$SIM_UNIVERSAL_LIB" "$XCFRAMEWORK_OUT/$SIM_SLICE_DIR/libplatform_bridge.a"
  cp "$DEVICE_LIB" "$XCFRAMEWORK_OUT/$DEVICE_SLICE_DIR/libplatform_bridge.a"
  copy_headers "$XCFRAMEWORK_OUT/$SIM_SLICE_DIR/Headers"
  copy_headers "$XCFRAMEWORK_OUT/$DEVICE_SLICE_DIR/Headers"
  write_xcframework_plist
}

validate_xcframework() {
  [[ -f "$XCFRAMEWORK_OUT/Info.plist" ]] || return 1
  [[ -f "$XCFRAMEWORK_OUT/$SIM_SLICE_DIR/libplatform_bridge.a" ]] || return 1
  [[ -f "$XCFRAMEWORK_OUT/$DEVICE_SLICE_DIR/libplatform_bridge.a" ]] || return 1
  return 0
}

rm -rf "$XCFRAMEWORK_OUT"
if command -v xcodebuild >/dev/null 2>&1; then
  if xcodebuild -create-xcframework \
    -library "$DEVICE_LIB" \
      -headers "$ROOT_DIR/bindings/ios" \
    -library "$SIM_UNIVERSAL_LIB" \
      -headers "$ROOT_DIR/bindings/ios" \
    -output "$XCFRAMEWORK_OUT"; then
    if ! validate_xcframework; then
      echo "warning: xcodebuild produced incomplete xcframework; applying manual packaging fallback" >&2
      build_xcframework_manually
    fi
  else
    echo "warning: xcodebuild -create-xcframework failed; applying manual packaging fallback" >&2
    build_xcframework_manually
  fi
else
  echo "warning: xcodebuild not found; applying manual packaging fallback" >&2
  build_xcframework_manually
fi

if ! validate_xcframework; then
  echo "error: failed to create a valid xcframework at $XCFRAMEWORK_OUT" >&2
  exit 1
fi

echo "Built $XCFRAMEWORK_OUT"
if [[ -d "$ROOT_DIR/flutter/ios/Runner.xcodeproj" ]]; then
  echo "Next step for Flutter iOS:"
  echo "  scripts/flutter-link-ios.sh"
fi
