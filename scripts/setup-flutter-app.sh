#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
FLUTTER_DIR="$ROOT_DIR/flutter"

if ! command -v flutter >/dev/null 2>&1; then
  echo "error: flutter CLI is required" >&2
  echo "install Flutter, then re-run this script" >&2
  exit 1
fi

mkdir -p "$FLUTTER_DIR"

(
  cd "$FLUTTER_DIR"
  if [[ ! -d ios || ! -d android ]]; then
    flutter create . --platforms=ios,android
  fi
)

ANDROID_GRADLE="$FLUTTER_DIR/android/app/build.gradle"
if [[ -f "$ANDROID_GRADLE" ]]; then
  perl -0pi -e 's/applicationId\s+"[^"]+"/applicationId "com.qubit.mai"/g' "$ANDROID_GRADLE"
fi

IOS_PBXPROJ="$FLUTTER_DIR/ios/Runner.xcodeproj/project.pbxproj"
if [[ -f "$IOS_PBXPROJ" ]]; then
  perl -0pi -e 's/PRODUCT_BUNDLE_IDENTIFIER = [^;]+;/PRODUCT_BUNDLE_IDENTIFIER = com.qubit.mai;/g' "$IOS_PBXPROJ"
fi

(
  cd "$FLUTTER_DIR"
  flutter pub get
)

echo "Flutter app setup complete in $FLUTTER_DIR"
echo "Android applicationId and iOS bundle id configured as com.qubit.mai"
