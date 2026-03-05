import 'dart:async';
import 'dart:developer' as developer;
import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:path_provider/path_provider.dart';

import 'http_runtime.dart';
import 'mai_runtime.dart';
import 'models.dart';

enum ConnectionMode { ffi, http, disconnected }

class RuntimeController extends ChangeNotifier {
  static final RegExp _specialTokenPattern = RegExp(
    r'^\s*(<\|[^|]+?\|>|<｜[^｜]+｜>|<[^>\s]{1,48}>)\s*$',
  );

  RuntimeController({
    this.httpBaseUrl = 'http://localhost:11434',
  }) {
    unawaited(_init());
  }

  final String httpBaseUrl;

  MaiRuntime? _runtime;
  HttpMaiRuntime? _httpRuntime;
  StreamSubscription<String>? _streamSubscription;
  StreamSubscription<EventUpdate>? _eventSubscription;
  Timer? _pollTimer;
  Duration? _pollInterval;

  bool initializing = true;
  bool initialized = false;
  bool isModelBusy = false;
  bool isGenerating = false;
  String selectedModelId = 'tinyllama-1b-q4';
  String? loadedModelId;
  String lastPrompt = '';
  String lastResponse = '';
  String? lastError;
  String? lastErrorDetails;
  int? activeCompletionId;
  DeviceProfile? deviceProfile;
  RuntimeMetrics? metrics;
  List<CatalogModel> catalog = const <CatalogModel>[];
  List<HubModel> hubModels = const <HubModel>[];
  String hubSearchQuery = 'gguf';
  List<DownloadJob> downloads = const <DownloadJob>[];
  String systemPrompt = '';
  double temperature = 0.7;
  double topP = 0.9;
  int topK = 40;
  double repeatPenalty = 1.1;
  int maxOutputTokens = 256;
  int? generationSeed;
  int contextWindow = 2048;
  bool preferAccelerator = true;
  bool thermalGuardEnabled = true;
  bool backgroundProcessingEnabled = false;
  ConnectionMode connectionMode = ConnectionMode.disconnected;
  bool hubSearchLoading = false;
  List<RuntimeDebugEntry> debugEntries = const <RuntimeDebugEntry>[];

  bool _refreshing = false;
  bool _disposed = false;

  /// Safe notifyListeners that skips if already disposed.
  void _safeNotify() {
    if (!_disposed) notifyListeners();
  }

  void _appendDebug(String level, String message) {
    final entry = RuntimeDebugEntry(
      timestamp: DateTime.now(),
      level: level,
      message: message,
    );
    debugEntries = <RuntimeDebugEntry>[entry, ...debugEntries];
    if (debugEntries.length > 200) {
      debugEntries = debugEntries.sublist(0, 200);
    }

    final logLevel = switch (level) {
      'error' => 1000,
      'warn' => 900,
      _ => 800,
    };
    developer.log(message, name: 'mai.runtime', level: logLevel);
  }

  void _clearError() {
    lastError = null;
    lastErrorDetails = null;
  }

  void _setError(String context, Object error, [StackTrace? stackTrace]) {
    final message = '$context: $error';
    final stack = stackTrace == null ? '' : '\n$stackTrace';
    lastError = message;
    lastErrorDetails = '[${DateTime.now().toIso8601String()}] $message$stack';
    _appendDebug('error', lastErrorDetails!);
  }

  Future<void> _init() async {
    Object? ffiError;
    Object? httpError;

    // Try FFI first, fall back to HTTP.
    try {
      _runtime = await _createPreferredFfiRuntime();
      connectionMode = ConnectionMode.ffi;
      initialized = true;
      _clearError();
      _appendDebug('info', 'Connected via FFI runtime');
      await refreshAll();
      _ensurePolling();
    } catch (e, st) {
      ffiError = e;
      _setError('FFI init failed', e, st);

      // FFI unavailable — try HTTP.
      try {
        final httpRuntime = HttpMaiRuntime(baseUrl: httpBaseUrl);
        final healthy = await httpRuntime.healthCheck();
        if (healthy) {
          _httpRuntime = httpRuntime;
          connectionMode = ConnectionMode.http;
          initialized = true;
          lastError = 'Native runtime unavailable, using HTTP mode: $ffiError';
          _appendDebug('warn', lastError!);
          await refreshAll();
          _ensurePolling();
        } else {
          httpError = 'health check failed for $httpBaseUrl';
          connectionMode = ConnectionMode.disconnected;
          lastError = _connectionErrorMessage(ffiError, httpError);
          _appendDebug('error', lastError!);
        }
      } catch (e, st) {
        httpError = e;
        connectionMode = ConnectionMode.disconnected;
        _setError(_connectionErrorMessage(ffiError, httpError), e, st);
      }
    }

    initializing = false;
    _safeNotify();
  }

  /// Build a runtime config with paths that are writable inside the iOS/Android
  /// app sandbox.  On desktop/macOS the Rust default (~/.mai) is fine.
  Future<Map<String, dynamic>?> _buildRuntimeConfig() async {
    if (!Platform.isIOS && !Platform.isAndroid) {
      return null; // use Rust defaults
    }

    try {
      // Documents dir survives backups on iOS; Library/Application Support is
      // the right place for non-user-visible data, but requires entitlements on
      // some iOS versions.  Documents is safe for both.
      final docsDir = await getApplicationDocumentsDirectory();
      final supportDir = await getApplicationSupportDirectory();

      final modelsDir = Directory('${docsDir.path}/mai/models');
      final cacheDir = Directory('${supportDir.path}/mai/cache');
      final logsDir = Directory('${supportDir.path}/mai/logs');

      // Keys must match the flattened PartialRuntimeConfig field names.
      return <String, dynamic>{
        'models_dir': modelsDir.path,
        'cache_dir': cacheDir.path,
        'logs_dir': logsDir.path,
      };
    } catch (e) {
      _appendDebug('warn', 'Could not resolve app directories: $e');
      return null;
    }
  }

  Future<MaiRuntime> _createPreferredFfiRuntime() async {
    final config = await _buildRuntimeConfig();
    _appendDebug('info', 'FFI runtime config: $config');
    return MaiRuntime.create(config: config);
  }

  String _connectionErrorMessage(Object? ffiError, Object? httpError) {
    if (ffiError == null) {
      return 'Cannot connect. Run `mai serve` to start the runtime server.';
    }

    final httpDetail =
        httpError == null ? 'HTTP endpoint not reachable' : '$httpError';
    return 'Native runtime init failed: $ffiError\n'
        'HTTP fallback also failed: $httpDetail\n'
        'Run `mai serve` for HTTP mode, or fix iOS FFI linking.';
  }

  /// Retry connecting (useful when user starts the server after app launch).
  Future<void> reconnect() async {
    if (initialized) return;
    initializing = true;
    _clearError();
    _appendDebug('info', 'Reconnect requested');
    _safeNotify();
    await _init();
  }

  bool get _hasFfi => _runtime != null;
  bool get _hasHttp => _httpRuntime != null;

  // ── Refresh ───────────────────────────────────────────────────────────────

  Future<void> refreshAll() async {
    if (!initialized || _refreshing) return;

    _refreshing = true;
    try {
      await Future.wait(<Future<void>>[
        refreshProfile(),
        refreshMetrics(),
        refreshCatalog(),
        refreshHubModels(),
        refreshDownloads(),
      ]);
      _clearError();
    } catch (e, st) {
      _setError('Refresh failed', e, st);
    } finally {
      _refreshing = false;
      _safeNotify();
    }
  }

  Future<void> refreshProfile() async {
    if (_hasFfi) {
      deviceProfile = await _runtime!.deviceProfile();
    } else if (_hasHttp) {
      deviceProfile = await _httpRuntime!.deviceProfile();
    }
    _safeNotify();
  }

  Future<void> refreshMetrics() async {
    if (_hasFfi) {
      metrics = await _runtime!.metrics();
    } else if (_hasHttp) {
      try {
        metrics = await _httpRuntime!.metrics();
      } catch (_) {
        // Metrics endpoint may not expose all fields — tolerate failures.
      }
    }
    _safeNotify();
  }

  Future<void> refreshDownloads() async {
    if (_hasFfi) {
      downloads = await _runtime!.downloadList();
    } else if (_hasHttp) {
      downloads = await _httpRuntime!.downloadList();
    }
    _ensurePolling();
    _safeNotify();
  }

  Future<void> refreshCatalog() async {
    if (_hasFfi) {
      catalog = await _runtime!.modelCatalog();
    } else if (_hasHttp) {
      catalog = await _httpRuntime!.modelCatalog();
    }
    _safeNotify();
  }

  Future<void> refreshHubModels() async {
    hubSearchLoading = true;
    _safeNotify();
    try {
      final request =
          HubSearchRequest(query: hubSearchQuery, limit: 80, onlyGguf: true);
      if (_hasFfi) {
        hubModels = await _runtime!.searchHubModels(request);
      } else if (_hasHttp) {
        hubModels = await _httpRuntime!.searchHubModels(request);
      }
    } finally {
      hubSearchLoading = false;
      _safeNotify();
    }
  }

  Future<void> searchHubModels(String query) async {
    hubSearchQuery = query.trim().isEmpty ? 'gguf' : query.trim();
    await refreshHubModels();
  }

  // ── Model load / unload ───────────────────────────────────────────────────

  Future<void> loadModel(String modelId) async {
    if (!initialized || isModelBusy) return;

    isModelBusy = true;
    selectedModelId = modelId;
    _clearError();
    _safeNotify();

    try {
      if (_hasFfi) {
        await _runtime!.loadModel(modelId);
      } else if (_hasHttp) {
        await _httpRuntime!.loadModel(modelId);
      }
      loadedModelId = modelId;
      _appendDebug('info', 'Model loaded: $modelId');
    } catch (e, st) {
      _setError('Load model failed', e, st);
    } finally {
      isModelBusy = false;
      _safeNotify();
    }
  }

  Future<void> unloadModel() async {
    if (!initialized || isModelBusy) return;

    isModelBusy = true;
    _clearError();
    _safeNotify();

    try {
      if (_hasFfi) {
        await _runtime!.unloadModel();
      } else if (_hasHttp) {
        await _httpRuntime!.unloadModel();
      }
      loadedModelId = null;
      _appendDebug('info', 'Model unloaded');
    } catch (e, st) {
      _setError('Unload model failed', e, st);
    } finally {
      isModelBusy = false;
      _safeNotify();
    }
  }

  // ── Completion ────────────────────────────────────────────────────────────

  Future<void> startCompletion(String prompt) async {
    if (!initialized || prompt.trim().isEmpty || isGenerating) return;

    final promptText = prompt.trim();
    final effectivePrompt = systemPrompt.trim().isEmpty
        ? promptText
        : '[System]\n${systemPrompt.trim()}\n\n[User]\n$promptText';

    lastPrompt = promptText;
    lastResponse = '';
    _clearError();
    isGenerating = true;
    _appendDebug('info', 'Generation started with model=$selectedModelId');
    _safeNotify();

    try {
      if (_hasFfi && loadedModelId != selectedModelId) {
        await _runtime!.loadModel(selectedModelId);
        loadedModelId = selectedModelId;
      }

      MaiCompletion completion;
      if (_hasFfi) {
        completion = await _runtime!.streamCompletion(
          effectivePrompt,
          options: GenerationOptions(
            maxTokens: maxOutputTokens,
            temperature: temperature,
            topP: topP,
            topK: topK,
            repeatPenalty: repeatPenalty,
            seed: generationSeed,
          ),
        );
      } else if (_hasHttp) {
        loadedModelId = selectedModelId;
        completion = await _httpRuntime!.streamCompletion(
          effectivePrompt,
          modelId: selectedModelId,
          options: GenerationOptions(
            maxTokens: maxOutputTokens,
            temperature: temperature,
            topP: topP,
            topK: topK,
            repeatPenalty: repeatPenalty,
            seed: generationSeed,
          ),
        );
      } else {
        throw MaiRuntimeException(-3, 'No runtime available');
      }

      activeCompletionId = completion.completionId;
      _safeNotify();

      await _streamSubscription?.cancel();
      _streamSubscription = completion.stream.listen(
        (token) {
          final cleanToken = _sanitizeDisplayToken(token);
          if (cleanToken.isEmpty) {
            return;
          }
          lastResponse = '$lastResponse$cleanToken';
          _safeNotify();
        },
        onError: (Object err, StackTrace st) {
          _setError('Completion stream error', err, st);
          isGenerating = false;
          activeCompletionId = null;
          _safeNotify();
        },
        onDone: () {
          isGenerating = false;
          activeCompletionId = null;
          unawaited(refreshMetrics());
          if (lastResponse.isEmpty && lastError == null) {
            _appendDebug(
              'warn',
              'Generation ended without tokens (model=$selectedModelId)',
            );
          }
          _safeNotify();
        },
      );
    } catch (e, st) {
      _setError('Completion failed', e, st);
      isGenerating = false;
      activeCompletionId = null;
      _safeNotify();
    }
  }

  Future<void> cancelCompletion() async {
    if (activeCompletionId == null) return;

    final completionId = activeCompletionId!;
    if (_hasFfi) {
      _runtime!.cancelCompletion(completionId);
    } else if (_hasHttp) {
      _httpRuntime!.cancelCompletion(completionId);
    }
    _appendDebug('info', 'Generation cancelled (id=$completionId)');
    activeCompletionId = null;
    isGenerating = false;

    await _streamSubscription?.cancel();
    _streamSubscription = null;

    _safeNotify();
    unawaited(refreshMetrics());
  }

  // ── Downloads ─────────────────────────────────────────────────────────────

  Future<void> startDownload(DownloadRequest request) async {
    if (!initialized) return;

    _clearError();
    _safeNotify();

    try {
      if (_hasFfi) {
        await _runtime!.startDownload(request);
      } else if (_hasHttp) {
        await _httpRuntime!.startDownload(request);
      }
      await refreshDownloads();
      await refreshMetrics();
      _appendDebug('info', 'Download started for model=${request.id}');
    } catch (e, st) {
      _setError('Start download failed', e, st);
      _safeNotify();
    }
  }

  Future<void> downloadHubFile(HubModel model, HubModelFile file) async {
    final quant =
        (file.quantization == null || file.quantization!.trim().isEmpty)
            ? 'Q4_K_M'
            : file.quantization!;
    final request = DownloadRequest(
      sourcePath: null,
      sourceUrl: file.downloadUrl,
      destinationPath: null,
      id: model.id,
      name: model.id,
      quant: quant,
      backend: file.isMlx ? 'mlx' : 'llama',
    );
    await startDownload(request);
  }

  Future<void> retryDownload(String jobId) async {
    if (!initialized) return;

    _clearError();
    _safeNotify();

    try {
      if (_hasFfi) {
        await _runtime!.retryDownload(jobId);
      } else if (_hasHttp) {
        await _httpRuntime!.retryDownload(jobId);
      }
      await refreshDownloads();
      await refreshMetrics();
      _appendDebug('info', 'Download retried: $jobId');
    } catch (e, st) {
      _setError('Retry failed', e, st);
      _safeNotify();
    }
  }

  Future<void> cancelDownload(String jobId) async {
    if (!initialized) return;

    _clearError();
    _safeNotify();

    try {
      if (_hasFfi) {
        await _runtime!.cancelDownload(jobId);
      } else if (_hasHttp) {
        await _httpRuntime!.cancelDownload(jobId);
      }
      await refreshDownloads();
      await refreshMetrics();
      _appendDebug('info', 'Download cancelled: $jobId');
    } catch (e, st) {
      _setError('Cancel download failed', e, st);
      _safeNotify();
    }
  }

  Future<void> deleteDownload(String jobId, {bool deleteFile = false}) async {
    if (!initialized) return;

    _clearError();
    _safeNotify();

    try {
      if (_hasFfi) {
        await _runtime!.deleteDownload(jobId, deleteFile: deleteFile);
      } else if (_hasHttp) {
        await _httpRuntime!.deleteDownload(jobId, deleteFile: deleteFile);
      }
      await refreshDownloads();
      _appendDebug('info', 'Download deleted: $jobId');
    } catch (e, st) {
      _setError('Delete download failed', e, st);
      _safeNotify();
    }
  }

  List<String> get availableModelIds {
    // Only show models that have been successfully downloaded.
    final ids = <String>{};
    for (final job in downloads) {
      if (job.status == 'succeeded') {
        ids.add(job.modelId);
      }
    }
    // Always include the selected model so the dropdown doesn't break.
    if (ids.isEmpty) {
      ids.add(selectedModelId);
    }
    final list = ids.toList(growable: false)..sort();
    return list;
  }

  void selectModel(String modelId) {
    selectedModelId = modelId;
    _safeNotify();
  }

  bool get hasActiveDownloads =>
      downloads.any((job) => job.status == 'queued' || job.status == 'running');

  double get ramUsagePct {
    final total = metrics?.ramTotalBytes ?? 0;
    final free = metrics?.ramFreeBytes ?? 0;
    if (total <= 0) return 0;
    return ((total - free) / total).clamp(0.0, 1.0);
  }

  double? get estimatedTokensPerSec {
    final baseline = deviceProfile?.benchmarkTokensPerSec;
    if (baseline == null) return null;
    if (!isGenerating) return baseline;
    final pressurePenalty = 1.0 - (ramUsagePct * 0.35);
    final thermalPenalty = thermalGuardEnabled ? 0.9 : 1.0;
    return (baseline * pressurePenalty * thermalPenalty).clamp(0.1, baseline);
  }

  // ── Settings ──────────────────────────────────────────────────────────────

  void setSystemPrompt(String value) {
    systemPrompt = value;
    _safeNotify();
  }

  void setTemperature(double value) {
    temperature = value;
    _safeNotify();
  }

  void setContextWindow(int value) {
    contextWindow = value;
    _safeNotify();
  }

  void setTopP(double value) {
    topP = value;
    _safeNotify();
  }

  void setTopK(int value) {
    topK = value;
    _safeNotify();
  }

  void setRepeatPenalty(double value) {
    repeatPenalty = value;
    _safeNotify();
  }

  void setMaxOutputTokens(int value) {
    maxOutputTokens = value;
    _safeNotify();
  }

  void setGenerationSeed(int? value) {
    generationSeed = value;
    _safeNotify();
  }

  void setPreferAccelerator(bool value) {
    preferAccelerator = value;
    _safeNotify();
  }

  void setThermalGuardEnabled(bool value) {
    thermalGuardEnabled = value;
    _safeNotify();
  }

  void setBackgroundProcessingEnabled(bool value) {
    backgroundProcessingEnabled = value;
    _safeNotify();
  }

  String _sanitizeDisplayToken(String token) {
    final cleaned = token.replaceAll(
      RegExp(r'[\x00-\x08\x0B\x0C\x0E-\x1F]'),
      '',
    );
    if (_specialTokenPattern.hasMatch(cleaned)) {
      return '';
    }
    return cleaned;
  }

  // ── Event stream / Polling ────────────────────────────────────────────────

  void _ensurePolling() {
    // In HTTP mode, use SSE push instead of polling.
    if (_hasHttp && _eventSubscription == null) {
      _startEventStream();
      return;
    }

    // FFI mode: fall back to timer-based polling.
    final interval = hasActiveDownloads
        ? const Duration(seconds: 1)
        : const Duration(seconds: 5);

    if (_pollTimer != null &&
        _pollTimer!.isActive &&
        _pollInterval == interval) {
      return;
    }

    _pollTimer?.cancel();
    _pollInterval = interval;

    _pollTimer = Timer.periodic(interval, (_) async {
      if (!initialized) return;

      try {
        await refreshDownloads();
        await refreshMetrics();
      } catch (e, st) {
        _setError('Polling failed', e, st);
        _safeNotify();
      }
    });
  }

  void _startEventStream() {
    _eventSubscription?.cancel();
    _eventSubscription = _httpRuntime!.eventStream().listen(
      (update) {
        downloads = update.downloads;
        _safeNotify();
      },
      onError: (Object err) {
        _appendDebug('warn', 'Event stream error: $err, falling back to polling');
        _eventSubscription = null;
        // Fall back to polling on SSE failure.
        _startFallbackPolling();
      },
      onDone: () {
        _appendDebug('info', 'Event stream closed, reconnecting...');
        _eventSubscription = null;
        // Reconnect after a short delay.
        Future<void>.delayed(const Duration(seconds: 2), () {
          if (initialized && !_disposed && _hasHttp) {
            _startEventStream();
          }
        });
      },
    );
  }

  void _startFallbackPolling() {
    _pollTimer?.cancel();
    _pollInterval = const Duration(seconds: 3);
    _pollTimer = Timer.periodic(_pollInterval!, (_) async {
      if (!initialized) return;
      try {
        await refreshDownloads();
        await refreshMetrics();
      } catch (e, st) {
        _setError('Polling failed', e, st);
        _safeNotify();
      }
    });
  }

  @override
  void dispose() {
    if (_disposed) return;
    _disposed = true;

    _pollTimer?.cancel();
    _pollTimer = null;
    _pollInterval = null;

    unawaited(_streamSubscription?.cancel());
    _streamSubscription = null;

    unawaited(_eventSubscription?.cancel());
    _eventSubscription = null;

    _runtime?.dispose();
    _runtime = null;

    _httpRuntime?.dispose();
    _httpRuntime = null;

    super.dispose();
  }
}

@immutable
class RuntimeDebugEntry {
  const RuntimeDebugEntry({
    required this.timestamp,
    required this.level,
    required this.message,
  });

  final DateTime timestamp;
  final String level;
  final String message;
}
