import 'dart:async';
import 'dart:convert';
import 'dart:ffi';
import 'dart:io';

import 'package:ffi/ffi.dart';

import '../ffi/mai_bindings.dart';
import 'models.dart';

class MaiRuntime {
  static const String _streamErrorPrefix = '__MAI_ERROR__:';

  MaiRuntime._(this._bindings, this._handle);

  final MaiBindings _bindings;
  Pointer<RuntimeHandle> _handle;
  final Map<int, _ActiveCompletion> _activeCompletions =
      <int, _ActiveCompletion>{};

  static Future<MaiRuntime> create({Map<String, dynamic>? config}) async {
    final library = _openLibrary();
    final bindings = MaiBindings(library);

    Pointer<Utf8> configPtr = nullptr;
    if (config != null && config.isNotEmpty) {
      configPtr = jsonEncode(config).toNativeUtf8();
    }

    try {
      final handle = bindings.maiRuntimeInit(configPtr);
      if (handle.address == 0) {
        final detail = _readLastNativeErrorFromBindings(bindings);
        throw MaiRuntimeException(
          -3,
          detail ?? 'Failed to initialize runtime handle',
        );
      }

      return MaiRuntime._(bindings, handle);
    } finally {
      if (configPtr.address != 0) {
        calloc.free(configPtr);
      }
    }
  }

  static DynamicLibrary _openLibrary() {
    if (Platform.isAndroid) {
      return DynamicLibrary.open('libplatform_bridge.so');
    }

    if (Platform.isIOS) {
      return _openAppleLibraryWithFallbacks();
    }

    if (Platform.isMacOS) {
      return DynamicLibrary.process();
    }

    throw UnsupportedError(
        'MAI runtime is only supported on iOS/Android in this client');
  }

  static DynamicLibrary _openAppleLibraryWithFallbacks() {
    final candidates = <_DynamicLibraryCandidate>[
      const _DynamicLibraryCandidate(
        name: 'process',
        open: DynamicLibrary.process,
      ),
      const _DynamicLibraryCandidate(
        name: 'executable',
        open: DynamicLibrary.executable,
      ),
    ];

    final executableDir = File(Platform.resolvedExecutable).parent.path;
    final debugDylibPath = '$executableDir/Runner.debug.dylib';
    if (File(debugDylibPath).existsSync()) {
      candidates.add(
        _DynamicLibraryCandidate(
          name: debugDylibPath,
          open: () => DynamicLibrary.open(debugDylibPath),
        ),
      );
    }

    final frameworkPath = '$executableDir/Frameworks/App.framework/App';
    if (File(frameworkPath).existsSync()) {
      candidates.add(
        _DynamicLibraryCandidate(
          name: frameworkPath,
          open: () => DynamicLibrary.open(frameworkPath),
        ),
      );
    }

    final errors = <String>[];
    for (final candidate in candidates) {
      try {
        final lib = candidate.open();
        lib.lookup<
            NativeFunction<Pointer<RuntimeHandle> Function(Pointer<Utf8>)>>(
          'mai_runtime_init',
        );
        return lib;
      } catch (e) {
        errors.add('${candidate.name}: $e');
      }
    }

    throw ArgumentError(
      'Failed to locate native runtime symbol `mai_runtime_init`. Tried: ${errors.join(' | ')}',
    );
  }

  bool get isDisposed => _handle.address == 0;

  void dispose() {
    if (_handle.address == 0) {
      return;
    }

    final completions = _activeCompletions.values.toList(growable: false);
    for (final completion in completions) {
      completion.close();
    }
    _activeCompletions.clear();

    _bindings.maiRuntimeDestroy(_handle);
    _handle = nullptr;
  }

  Future<void> loadModel(String modelId) async {
    _ensureNotDisposed();
    final modelIdPtr = modelId.toNativeUtf8();
    try {
      final code = _bindings.maiLoadModel(_handle, modelIdPtr);
      _throwIfErr(code, 'Failed to load model $modelId');
    } finally {
      calloc.free(modelIdPtr);
    }
  }

  Future<void> unloadModel() async {
    _ensureNotDisposed();
    final code = _bindings.maiUnloadModel(_handle);
    _throwIfErr(code, 'Failed to unload model');
  }

  Future<MaiCompletion> streamCompletion(
    String prompt, {
    GenerationOptions options = const GenerationOptions(),
  }) async {
    _ensureNotDisposed();

    final controller = StreamController<String>();
    final completionIdPtr = calloc<Uint64>();
    final promptPtr = prompt.toNativeUtf8();
    final paramsPtr = jsonEncode(options.toJson()).toNativeUtf8();

    var completionId = 0;
    late final NativeCallable<TokenCallbackNative> callback;

    callback = NativeCallable<TokenCallbackNative>.listener(
      (Pointer<Utf8> tokenPtr, Pointer<Void> _) {
        if (tokenPtr.address == 0) {
          final active = _activeCompletions.remove(completionId);
          active?.close();
          return;
        }

        try {
          final token = _sanitizeToken(_readUtf8Lossy(tokenPtr));
          if (token.startsWith(_streamErrorPrefix)) {
            if (!controller.isClosed) {
              controller.addError(
                MaiRuntimeException(
                    -3, token.substring(_streamErrorPrefix.length)),
              );
            }
            cancelCompletion(completionId);
            return;
          }
          if (!controller.isClosed && token.isNotEmpty) {
            controller.add(token);
          }
        } catch (e, st) {
          if (!controller.isClosed) {
            controller.addError(e, st);
          }
        } finally {
          // Rust now transfers string ownership for each token callback.
          _bindings.maiFreeString(tokenPtr);
        }
      },
    );

    final withParams = _bindings.maiChatCompletionWithParams;
    final code = withParams != null
        ? withParams(
            _handle,
            promptPtr,
            paramsPtr,
            callback.nativeFunction,
            nullptr,
            completionIdPtr,
          )
        : _bindings.maiChatCompletion(
            _handle,
            promptPtr,
            callback.nativeFunction,
            nullptr,
            completionIdPtr,
          );

    calloc.free(promptPtr);
    calloc.free(paramsPtr);

    if (code != 0) {
      callback.close();
      calloc.free(completionIdPtr);
      await controller.close();
      _throwIfErr(code, 'Failed to start completion');
    }

    completionId = completionIdPtr.value;
    calloc.free(completionIdPtr);

    controller.onCancel = () {
      if (completionId != 0) {
        cancelCompletion(completionId);
      }
    };

    _activeCompletions[completionId] = _ActiveCompletion(controller, callback);
    return MaiCompletion(completionId: completionId, stream: controller.stream);
  }

  int cancelCompletion(int completionId) {
    _ensureNotDisposed();
    final code = _bindings.maiCancelCompletion(_handle, completionId);

    // Keep callback metadata alive until native emits terminal null token.
    // Closing callback immediately can race with in-flight native callbacks.
    final active = _activeCompletions[completionId];
    if (active != null && !active.controller.isClosed) {
      unawaited(active.controller.close());
    }

    return code;
  }

  Future<DeviceProfile> deviceProfile() async {
    _ensureNotDisposed();
    final json = _readOwnedJsonString(_bindings.maiDeviceProfileJson(_handle));
    return DeviceProfile.fromJson(jsonDecode(json) as Map<String, dynamic>);
  }

  Future<String> startDownload(DownloadRequest request) async {
    _ensureNotDisposed();

    if (!request.hasAtMostOneSource) {
      throw MaiRuntimeException(
          -3, 'Download request cannot set both source_path and source_url');
    }

    final requestPtr = jsonEncode(request.toJson()).toNativeUtf8();
    final outJobId = calloc<Pointer<Utf8>>();

    try {
      final code = _bindings.maiDownloadStart(_handle, requestPtr, outJobId);
      _throwIfErr(code, 'Failed to start download');

      final ptr = outJobId.value;
      if (ptr.address == 0) {
        throw MaiRuntimeException(
            -3, 'Native download start returned empty job id');
      }

      final value = ptr.toDartString();
      _bindings.maiFreeString(ptr);
      return value;
    } finally {
      calloc.free(requestPtr);
      calloc.free(outJobId);
    }
  }

  Future<DownloadJob> downloadStatus(String jobId) async {
    _ensureNotDisposed();

    final idPtr = jobId.toNativeUtf8();
    try {
      final ptr = _bindings.maiDownloadStatusJson(_handle, idPtr);
      if (ptr.address == 0) {
        throw MaiRuntimeException(-4, 'Download job not found: $jobId');
      }
      final json = _readOwnedJsonString(ptr);
      return DownloadJob.fromJson(jsonDecode(json) as Map<String, dynamic>);
    } finally {
      calloc.free(idPtr);
    }
  }

  Future<List<DownloadJob>> downloadList() async {
    _ensureNotDisposed();
    final json = _readOwnedJsonString(_bindings.maiDownloadListJson(_handle));
    final decoded = jsonDecode(json) as Map<String, dynamic>;
    final raw = (decoded['data'] as List<dynamic>? ?? const <dynamic>[])
        .cast<Map<String, dynamic>>();
    return raw.map(DownloadJob.fromJson).toList(growable: false);
  }

  Future<String> retryDownload(String jobId) async {
    _ensureNotDisposed();

    final idPtr = jobId.toNativeUtf8();
    final outJobId = calloc<Pointer<Utf8>>();

    try {
      final code = _bindings.maiDownloadRetry(_handle, idPtr, outJobId);
      _throwIfErr(code, 'Failed to retry download job $jobId');

      final ptr = outJobId.value;
      if (ptr.address == 0) {
        throw MaiRuntimeException(-3, 'Native retry returned empty job id');
      }

      final value = ptr.toDartString();
      _bindings.maiFreeString(ptr);
      return value;
    } finally {
      calloc.free(idPtr);
      calloc.free(outJobId);
    }
  }

  Future<void> cancelDownload(String jobId) async {
    _ensureNotDisposed();
    final idPtr = jobId.toNativeUtf8();
    try {
      final code = _bindings.maiDownloadCancel(_handle, idPtr);
      _throwIfErr(code, 'Failed to cancel download job $jobId');
    } finally {
      calloc.free(idPtr);
    }
  }

  Future<void> deleteDownload(String jobId, {bool deleteFile = false}) async {
    _ensureNotDisposed();
    final idPtr = jobId.toNativeUtf8();
    try {
      final code = _bindings.maiDownloadDelete(_handle, idPtr, deleteFile);
      _throwIfErr(code, 'Failed to delete download job $jobId');
    } finally {
      calloc.free(idPtr);
    }
  }

  Future<RuntimeMetrics> metrics() async {
    _ensureNotDisposed();
    final json = _readOwnedJsonString(_bindings.maiMetricsJson(_handle));
    return RuntimeMetrics.fromJson(jsonDecode(json) as Map<String, dynamic>);
  }

  Future<List<CatalogModel>> modelCatalog() async {
    _ensureNotDisposed();
    final json = _readOwnedJsonString(_bindings.maiModelCatalogJson(_handle));
    final decoded = jsonDecode(json) as Map<String, dynamic>;
    final raw = (decoded['models'] as List<dynamic>? ?? const <dynamic>[])
        .cast<Map<String, dynamic>>();
    return raw.map(CatalogModel.fromJson).toList(growable: false);
  }

  Future<List<HubModel>> searchHubModels(HubSearchRequest request) async {
    _ensureNotDisposed();
    final requestPtr = jsonEncode(request.toJson()).toNativeUtf8();
    try {
      final json = _readOwnedJsonString(
        _bindings.maiHubSearchModelsJson(_handle, requestPtr),
      );
      final decoded = jsonDecode(json) as Map<String, dynamic>;
      final raw = (decoded['data'] as List<dynamic>? ?? const <dynamic>[])
          .cast<Map<String, dynamic>>();
      return raw.map(HubModel.fromJson).toList(growable: false);
    } finally {
      calloc.free(requestPtr);
    }
  }

  String _readOwnedJsonString(Pointer<Utf8> ptr) {
    if (ptr.address == 0) {
      final detail = _readLastNativeError();
      throw MaiRuntimeException(
        -3,
        detail ?? 'Native function returned null JSON pointer',
      );
    }

    try {
      return ptr.toDartString();
    } finally {
      _bindings.maiFreeString(ptr);
    }
  }

  void _throwIfErr(int code, String fallbackMessage) {
    if (code == 0) {
      return;
    }

    final detail = _readLastNativeError();
    throw MaiRuntimeException(
      code,
      detail ?? _errorMessage(code) ?? fallbackMessage,
    );
  }

  void _ensureNotDisposed() {
    if (_handle.address == 0) {
      throw StateError('MAI runtime has already been disposed');
    }
  }

  static String? _errorMessage(int code) {
    switch (code) {
      case -1:
        return 'Native call received a null pointer';
      case -2:
        return 'Native call received invalid UTF-8 input';
      case -3:
        return 'Runtime operation failed';
      case -4:
        return 'Requested item not found';
      default:
        return null;
    }
  }

  static String? _readLastNativeErrorFromBindings(MaiBindings bindings) {
    final ptr = bindings.maiLastErrorMessage();
    if (ptr.address == 0) {
      return null;
    }
    try {
      final message = ptr.toDartString().trim();
      return message.isEmpty ? null : message;
    } finally {
      bindings.maiFreeString(ptr);
    }
  }

  String? _readLastNativeError() => _readLastNativeErrorFromBindings(_bindings);

  static String _readUtf8Lossy(Pointer<Utf8> ptr) {
    final bytes = <int>[];
    var offset = 0;
    final raw = ptr.cast<Uint8>();
    while (true) {
      final value = raw[offset];
      if (value == 0) {
        break;
      }
      bytes.add(value);
      offset += 1;
    }
    return utf8.decode(bytes, allowMalformed: true);
  }

  static String _sanitizeToken(String token) {
    if (token.isEmpty) return token;
    return token.replaceAll(
      RegExp(r'[\x00-\x08\x0B\x0C\x0E-\x1F]'),
      '',
    );
  }
}

class _ActiveCompletion {
  _ActiveCompletion(this.controller, this.callback);

  final StreamController<String> controller;
  final NativeCallable<TokenCallbackNative> callback;

  void close() {
    if (!controller.isClosed) {
      unawaited(controller.close());
    }
    scheduleMicrotask(() {
      try {
        callback.close();
      } catch (_) {
        // Best-effort close; callback may already be closed.
      }
    });
  }
}

class _DynamicLibraryCandidate {
  const _DynamicLibraryCandidate({
    required this.name,
    required this.open,
  });

  final String name;
  final DynamicLibrary Function() open;
}
