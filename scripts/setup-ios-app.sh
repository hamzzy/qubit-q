#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

"$ROOT_DIR/scripts/build-ios.sh"

echo "iOS app scaffold is ready at: $ROOT_DIR/ios/MobileAIRuntimeApp"
echo "Open in Xcode: $ROOT_DIR/ios/MobileAIRuntimeApp/MobileAIRuntimeApp.xcodeproj"
