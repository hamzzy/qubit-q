#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
FEATURES="${FEATURES:-mock-backend}"
export FEATURES

if [[ (! -d "$ROOT_DIR/flutter/ios/Runner.xcodeproj" || ! -d "$ROOT_DIR/flutter/android/app") && -x "$ROOT_DIR/scripts/setup-flutter-app.sh" ]]; then
  if command -v flutter >/dev/null 2>&1; then
    "$ROOT_DIR/scripts/setup-flutter-app.sh"
  else
    echo "warning: Flutter runner projects not found and flutter CLI is not installed" >&2
    echo "run scripts/setup-flutter-app.sh after installing Flutter" >&2
  fi
fi

"$ROOT_DIR/scripts/build-ios.sh"
"$ROOT_DIR/scripts/flutter-link-ios.sh"
"$ROOT_DIR/scripts/build-android.sh"

if command -v dart >/dev/null 2>&1; then
  if [[ -f "$ROOT_DIR/flutter/pubspec.yaml" ]]; then
    (
      cd "$ROOT_DIR/flutter"
      if grep -q "ffigen" pubspec.yaml; then
        echo "Running ffigen..."
        dart run ffigen --config ffigen.yaml || true
      fi
    )
  fi
fi

echo "Flutter FFI artifacts built with features: $FEATURES"
