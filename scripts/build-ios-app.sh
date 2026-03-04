#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PROJECT="$ROOT_DIR/ios/MobileAIRuntimeApp/MobileAIRuntimeApp.xcodeproj"
SCHEME="MobileAIRuntimeApp"
DESTINATION="${DESTINATION:-generic/platform=iOS Simulator}"
DERIVED_DATA="$ROOT_DIR/.xcode/DerivedData"

"$ROOT_DIR/scripts/build-ios.sh"

mkdir -p "$DERIVED_DATA"

xcodebuild \
  -project "$PROJECT" \
  -scheme "$SCHEME" \
  -configuration Debug \
  -destination "$DESTINATION" \
  -derivedDataPath "$DERIVED_DATA" \
  CODE_SIGNING_ALLOWED=NO \
  build
