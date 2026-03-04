# MAI Flutter Client

Flutter app for Mobile AI Runtime using direct Dart FFI to `platform-bridge` C ABI.

## Scope

- Targets: iOS + Android
- Runtime bridge: direct Dart FFI (`libplatform_bridge.so` / iOS process symbols)
- Existing SwiftUI app remains side-by-side and unchanged

## Prerequisites

- Flutter SDK on `PATH`
- Rust toolchain
- Android NDK (`ANDROID_NDK_HOME`) for Android builds
- Xcode command line tools for iOS builds
- `cargo-ndk` for Android Rust artifacts (`cargo install cargo-ndk`)

## Setup

From repo root:

```bash
scripts/setup-flutter-app.sh
scripts/build-flutter-ffi.sh
cd flutter
flutter pub get
```

## Run

```bash
cd flutter
flutter run -d ios
# or
flutter run -d android
```

## App Structure

- `lib/src/ffi/`: raw C ABI bindings
- `lib/src/runtime/`: safe runtime wrapper + Riverpod state controller
- `lib/src/features/chat/`: streaming chat UI
- `lib/src/features/models/`: model load/unload + device profile
- `lib/src/features/downloads/`: download start/progress/retry UX
- `lib/src/features/observability/`: runtime metrics dashboard
- `lib/src/shared/`: formatting helpers

## Notes

- App IDs are configured to `com.qubit.mai` by `scripts/setup-flutter-app.sh`.
- iOS FFI symbols are loaded with `DynamicLibrary.process()`.
- Android FFI library is loaded via `DynamicLibrary.open('libplatform_bridge.so')`.
