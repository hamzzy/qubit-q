#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CRATE="platform-bridge"
FEATURES="${FEATURES:-llama-backend}"  # Android does not support MLX (Apple Silicon only)
OUT_DIR="$ROOT_DIR/flutter/android/app/src/main/jniLibs"

if [[ ! -d "$ROOT_DIR/flutter" ]]; then
  echo "error: flutter app directory not found at $ROOT_DIR/flutter" >&2
  exit 1
fi

if [[ ! -d "$ROOT_DIR/flutter/android/app" ]]; then
  echo "error: Flutter Android runner is missing" >&2
  echo "run scripts/setup-flutter-app.sh first" >&2
  exit 1
fi

if [[ -z "${ANDROID_NDK_HOME:-}" ]]; then
  echo "error: ANDROID_NDK_HOME is not set" >&2
  exit 1
fi

if [[ ! -d "$ANDROID_NDK_HOME" ]]; then
  echo "error: ANDROID_NDK_HOME does not exist: $ANDROID_NDK_HOME" >&2
  exit 1
fi

if ! command -v cargo-ndk >/dev/null 2>&1; then
  echo "error: cargo-ndk is required. Install with: cargo install cargo-ndk" >&2
  exit 1
fi

ensure_target() {
  local target="$1"
  if rustup target list --installed | grep -q "^${target}$"; then
    return 0
  fi

  echo "Installing rust target: $target"
  rustup target add "$target"
}

ensure_target "aarch64-linux-android"
ensure_target "armv7-linux-androideabi"
ensure_target "x86_64-linux-android"

mkdir -p "$OUT_DIR"

cargo ndk \
  --platform 24 \
  -t arm64-v8a \
  -t armeabi-v7a \
  -t x86_64 \
  -o "$OUT_DIR" \
  build \
  -p "$CRATE" \
  --release \
  --no-default-features \
  --features "$FEATURES"

echo "Built Android JNI libs into: $OUT_DIR"
find "$OUT_DIR" -name "libplatform_bridge.so" -print
