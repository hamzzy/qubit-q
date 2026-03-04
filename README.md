# MAI (Mobile AI Runtime)

Production-oriented local LLM runtime for mobile and edge, implemented in Rust with a Flutter client.

MAI is designed as an on-device inference platform, not just a UI wrapper. It provides:
- A Rust runtime with model registry, memory guardrails, and runtime metrics
- An OpenAI-compatible HTTP surface (`/v1/chat/completions`, `/v1/models`, `/v1/embeddings`)
- A C ABI bridge for mobile embedding (`platform-bridge`)
- A Flutter app for chat, model browsing/downloads, and observability

## Architecture

### Core crates
- `crates/runtime-core`: runtime orchestration and config
- `crates/inference-engine`: backend abstraction + backend implementations
- `crates/model-manager`: catalog/registry, download, validation
- `crates/memory-guard`: RAM protection and throttling logic
- `crates/device-profiler`: hardware capability detection + recommendations
- `crates/http-server`: OpenAI-like API + metrics + download endpoints
- `crates/platform-bridge`: C ABI exposed to mobile apps
- `crates/mai`: CLI entrypoint

### Mobile client
- `flutter/`: Flutter app using Dart FFI into `platform-bridge`

## Quick Start (Mock Backend)

This is the fastest path to run the stack without local model setup.

### 1) Prerequisites
- Rust stable toolchain
- `cargo`
- Flutter SDK (for mobile client)
- Android NDK + `cargo-ndk` (for Android FFI builds)
- Xcode CLT (for iOS)

### 2) Build workspace
```bash
cargo build --workspace --features mock-backend
```

### 3) Run HTTP server
```bash
cargo run -p mai --features mock-backend -- serve --port 11434 --api-key test-key
```

### 4) Verify server
```bash
curl -s http://127.0.0.1:11434/health
curl -s -H "Authorization: Bearer test-key" http://127.0.0.1:11434/v1/models
```

## CLI Usage

### List commands
```bash
cargo run -p mai -- --help
```

### Common flows
```bash
# Profile device and capabilities
cargo run -p mai --features mock-backend -- profile

# List registered models
cargo run -p mai --features mock-backend -- models

# Serve OpenAI-compatible API
cargo run -p mai --features mock-backend -- serve --port 11434 --api-key test-key
```

## HTTP API Endpoints

Public:
- `GET /health`

Protected when API key is configured:
- `GET /metrics`
- `GET /v1/models`
- `GET /v1/models/catalog`
- `POST /v1/models/download`
- `GET /v1/models/downloads`
- `GET /v1/models/downloads/:job_id`
- `POST /v1/models/downloads/:job_id/retry`
- `POST /v1/models/downloads/:job_id/cancel`
- `DELETE /v1/models/downloads/:job_id`
- `POST /v1/models/hub/search`
- `POST /v1/chat/completions`
- `POST /v1/embeddings`

## Flutter App

### Setup
From repo root:
```bash
scripts/setup-flutter-app.sh
scripts/build-flutter-ffi.sh
cd flutter
flutter pub get
```

### Run
```bash
cd flutter
flutter run -d ios
# or
flutter run -d android
```

### Flutter feature map
- `lib/src/features/chat/`: streaming chat UX
- `lib/src/features/models/`: hub search + load/unload + downloads
- `lib/src/features/downloads/`: download queue/progress/retry/cancel
- `lib/src/features/observability/`: runtime + device metrics
- `lib/src/runtime/`: runtime controller/provider and bridge integration

## Useful Scripts

- `scripts/build-ios.sh`: build iOS bridge artifacts
- `scripts/flutter-link-ios.sh`: link iOS artifacts into Flutter
- `scripts/build-android.sh`: build Android JNI libs (`libplatform_bridge.so`)
- `scripts/build-flutter-ffi.sh`: all FFI build steps
- `scripts/smoke-http-lan.sh`: quick API smoke test

## Configuration

Runtime defaults come from `RuntimeConfig` and are created under `~/.mai/`:
- `~/.mai/models`
- `~/.mai/cache`
- `~/.mai/logs`

Optional environment variables commonly used by server binaries:
- `MAI_BACKEND=auto|mock|llama|mlx`
- `MAI_API_KEY=<token>`
- `MAI_HTTP_PORT=<port>`
- `MAI_HTTP_LAN=1` (bind all interfaces)
- `MAI_HTTP_TLS_CERT=/path/cert.pem`
- `MAI_HTTP_TLS_KEY=/path/key.pem`
- `MAI_AFRICA_MODE=1`

## Development Notes

- The repository may contain large generated artifacts for local testing (Flutter and native outputs).
- Keep model files (`*.gguf`) out of git; they are ignored.
- Use `cargo test --workspace` and `flutter test` before shipping.

## License

No license file is currently declared in this repository. Add one before external distribution.
