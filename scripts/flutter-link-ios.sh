#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
XCFRAMEWORK_SRC="$ROOT_DIR/bindings/ios/MobileAIRuntime.xcframework"
HEADER_SRC="$ROOT_DIR/bindings/ios/MobileAIRuntime.h"
DEST_DIR="$ROOT_DIR/flutter/ios/Runner/Frameworks"
DEST_XCFRAMEWORK="$DEST_DIR/MobileAIRuntime.xcframework"
DEST_HEADER_DIR="$ROOT_DIR/flutter/ios/Runner/BridgeHeaders"
FFI_XCCONFIG="$ROOT_DIR/flutter/ios/Flutter/MobileAIRuntimeFFI.xcconfig"
DEBUG_XCCONFIG="$ROOT_DIR/flutter/ios/Flutter/Debug.xcconfig"
RELEASE_XCCONFIG="$ROOT_DIR/flutter/ios/Flutter/Release.xcconfig"

if [[ ! -d "$XCFRAMEWORK_SRC" ]]; then
  echo "error: missing iOS xcframework at $XCFRAMEWORK_SRC" >&2
  echo "run scripts/build-ios.sh first" >&2
  exit 1
fi

if [[ ! -d "$ROOT_DIR/flutter/ios" ]]; then
  echo "error: flutter iOS project not found at $ROOT_DIR/flutter/ios" >&2
  echo "initialize flutter app first" >&2
  exit 1
fi

if [[ ! -d "$ROOT_DIR/flutter/ios/Runner.xcodeproj" ]]; then
  echo "error: Flutter iOS Runner project is missing" >&2
  echo "run scripts/setup-flutter-app.sh first" >&2
  exit 1
fi

mkdir -p "$DEST_DIR"
rm -rf "$DEST_XCFRAMEWORK"
cp -R "$XCFRAMEWORK_SRC" "$DEST_XCFRAMEWORK"

mkdir -p "$DEST_HEADER_DIR"
cp "$HEADER_SRC" "$DEST_HEADER_DIR/MobileAIRuntime.h"

cat > "$FFI_XCCONFIG" <<'EOF'
// Force-link Rust C ABI symbols so Dart FFI can resolve them via DynamicLibrary.process().
OTHER_LDFLAGS[sdk=iphonesimulator*]=$(inherited) -force_load $(SRCROOT)/Runner/Frameworks/MobileAIRuntime.xcframework/ios-arm64_x86_64-simulator/libplatform_bridge.a
OTHER_LDFLAGS[sdk=iphoneos*]=$(inherited) -force_load $(SRCROOT)/Runner/Frameworks/MobileAIRuntime.xcframework/ios-arm64/libplatform_bridge.a
EOF

ensure_include() {
  local file="$1"
  local include_line='#include "MobileAIRuntimeFFI.xcconfig"'

  if [[ ! -f "$file" ]]; then
    return
  fi
  if grep -Fq "$include_line" "$file"; then
    return
  fi
  printf '%s\n' "$include_line" >> "$file"
}

ensure_include "$DEBUG_XCCONFIG"
ensure_include "$RELEASE_XCCONFIG"

if [[ ! -f "$DEST_XCFRAMEWORK/ios-arm64/libplatform_bridge.a" ]]; then
  echo "warning: iOS device slice missing at $DEST_XCFRAMEWORK/ios-arm64/libplatform_bridge.a" >&2
  echo "         simulator builds can still work; rebuild with scripts/build-ios.sh for device support" >&2
fi

echo "Linked iOS runtime artifacts into Flutter runner:"
echo "  $DEST_XCFRAMEWORK"
echo "  $DEST_HEADER_DIR/MobileAIRuntime.h"
echo "  $FFI_XCCONFIG"
