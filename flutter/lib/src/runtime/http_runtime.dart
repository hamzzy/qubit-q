import 'dart:async';
import 'dart:convert';

import 'package:http/http.dart' as http;

import 'models.dart';

/// HTTP-based runtime client that talks to the `mai serve` HTTP server.
/// Drop-in replacement for the FFI [MaiRuntime] when native linking is unavailable.
class HttpMaiRuntime {
  static const String _streamErrorPrefix = '__MAI_ERROR__:';

  HttpMaiRuntime({
    required this.baseUrl,
    this.apiKey,
  });

  final String baseUrl;
  final String? apiKey;
  bool _disposed = false;
  int _nextCompletionId = 1;
  final Map<int, _HttpCompletion> _completions = <int, _HttpCompletion>{};

  bool get isDisposed => _disposed;

  void dispose() {
    for (final id in _completions.keys.toList(growable: false)) {
      _closeCompletion(id);
    }
    _disposed = true;
  }

  // ── Health ──────────────────────────────────────────────────────────────

  Future<bool> healthCheck() async {
    try {
      final response = await _get('/health');
      return response.statusCode == 200;
    } catch (_) {
      return false;
    }
  }

  // ── Models ──────────────────────────────────────────────────────────────

  Future<void> loadModel(String modelId) async {
    // The HTTP server auto-loads on first chat; this is a compatibility shim.
    // If a dedicated load endpoint is added later, wire it here.
  }

  Future<void> unloadModel() async {
    // Compatibility shim — server manages model lifecycle.
  }

  // ── Chat ────────────────────────────────────────────────────────────────

  Future<MaiCompletion> streamCompletion(String prompt, {required String modelId}) async {
    _ensureNotDisposed();

    final controller = StreamController<String>();
    final completionId = _nextCompletionId++;
    final body = jsonEncode(<String, dynamic>{
      'model': modelId,
      'messages': [
        {'role': 'user', 'content': prompt},
      ],
      'stream': true,
    });

    final request = http.Request('POST', Uri.parse('$baseUrl/v1/chat/completions'));
    request.headers['Content-Type'] = 'application/json';
    _applyAuth(request.headers);
    request.body = body;

    final client = http.Client();
    _completions[completionId] = _HttpCompletion(client, controller);
    controller.onCancel = () {
      cancelCompletion(completionId);
    };

    // Fire off the request, streaming SSE events back.
    http.StreamedResponse response;
    try {
      response = await client.send(request);
    } catch (e) {
      _closeCompletion(completionId);
      throw MaiRuntimeException(-3, 'HTTP stream request failed: $e');
    }

    if (response.statusCode != 200) {
      final body = await response.stream.bytesToString();
      _closeCompletion(completionId);
      throw MaiRuntimeException(-3, 'Chat completion HTTP ${response.statusCode}: $body');
    }

    // Parse SSE lines
    final subscription = response.stream
        .transform(const Utf8Decoder(allowMalformed: true))
        .transform(const LineSplitter())
        .listen(
      (line) {
        if (!line.startsWith('data: ')) return;
        final payload = line.substring(6).trim();
        if (payload == '[DONE]') {
          _closeCompletion(completionId);
          return;
        }
        try {
          final json = jsonDecode(payload) as Map<String, dynamic>;
          final choices = json['choices'] as List<dynamic>?;
          if (choices != null && choices.isNotEmpty) {
            final delta = (choices[0] as Map<String, dynamic>)['delta'] as Map<String, dynamic>?;
            final content = delta?['content'] as String?;
            if (content != null && !controller.isClosed) {
              if (content.startsWith(_streamErrorPrefix)) {
                controller.addError(
                  MaiRuntimeException(-3, content.substring(_streamErrorPrefix.length)),
                );
                _closeCompletion(completionId);
                return;
              }
              controller.add(content);
            }
          }
        } catch (_) {
          // Skip malformed SSE lines
        }
      },
      onError: (Object err) {
        if (!controller.isClosed) {
          controller.addError(err);
        }
        _closeCompletion(completionId);
      },
      onDone: () {
        _closeCompletion(completionId);
      },
    );
    _completions[completionId]?.subscription = subscription;

    return MaiCompletion(completionId: completionId, stream: controller.stream);
  }

  int cancelCompletion(int completionId) {
    if (!_completions.containsKey(completionId)) {
      return -4;
    }
    _closeCompletion(completionId);
    return 0;
  }

  // ── Device Profile ──────────────────────────────────────────────────────

  Future<DeviceProfile> deviceProfile() async {
    _ensureNotDisposed();
    final response = await _get('/health');
    final json = _decodeJson(response);
    // The health endpoint includes device info. Build a profile from available data.
    return DeviceProfile(
      totalRamBytes: (json['ram_total_bytes'] as num?)?.toInt() ?? 0,
      freeRamBytes: (json['ram_free_bytes'] as num?)?.toInt() ?? 0,
      cpuCores: (json['cpu_cores'] as num?)?.toInt() ?? 0,
      cpuArch: (json['cpu_arch'] as String?) ?? 'unknown',
      hasGpu: (json['has_gpu'] as bool?) ?? false,
      gpuType: (json['gpu_type'] as String?) ?? 'none',
      platform: (json['platform'] as String?) ?? 'server',
      batteryLevel: null,
      isCharging: false,
      availableStorageBytes: 0,
      benchmarkTokensPerSec: null,
    );
  }

  // ── Downloads ───────────────────────────────────────────────────────────

  Future<String> startDownload(DownloadRequest request) async {
    _ensureNotDisposed();
    final response = await _post('/v1/models/download', request.toJson());
    final json = _decodeJson(response);
    return json['job_id'] as String;
  }

  Future<DownloadJob> downloadStatus(String jobId) async {
    _ensureNotDisposed();
    final response = await _get('/v1/models/downloads/$jobId');
    return DownloadJob.fromJson(_decodeJson(response));
  }

  Future<List<DownloadJob>> downloadList() async {
    _ensureNotDisposed();
    final response = await _get('/v1/models/downloads');
    final json = _decodeJson(response);
    final data = (json['data'] as List<dynamic>? ?? const <dynamic>[])
        .cast<Map<String, dynamic>>();
    return data.map(DownloadJob.fromJson).toList(growable: false);
  }

  Future<String> retryDownload(String jobId) async {
    _ensureNotDisposed();
    final response = await _post('/v1/models/downloads/$jobId/retry', <String, dynamic>{});
    final json = _decodeJson(response);
    final job = json['job'] as Map<String, dynamic>;
    return job['job_id'] as String;
  }

  Future<void> cancelDownload(String jobId) async {
    _ensureNotDisposed();
    await _post('/v1/models/downloads/$jobId/cancel', <String, dynamic>{});
  }

  Future<void> deleteDownload(String jobId, {bool deleteFile = false}) async {
    _ensureNotDisposed();
    final uri = Uri.parse('$baseUrl/v1/models/downloads/$jobId')
        .replace(queryParameters: <String, String>{
      'delete_file': deleteFile ? 'true' : 'false',
    });
    final headers = <String, String>{};
    _applyAuth(headers);
    final response = await http.delete(uri, headers: headers);
    if (response.statusCode >= 400) {
      throw MaiRuntimeException(-3, 'HTTP ${response.statusCode}: ${response.body}');
    }
  }

  // ── Catalog ─────────────────────────────────────────────────────────────

  Future<List<CatalogModel>> modelCatalog() async {
    _ensureNotDisposed();
    final response = await _get('/v1/models/catalog');
    final json = _decodeJson(response);
    final models = json['models'] as Map<String, dynamic>?;
    if (models == null) return const <CatalogModel>[];
    final list = (models['models'] as List<dynamic>? ?? const <dynamic>[])
        .cast<Map<String, dynamic>>();
    return list.map(CatalogModel.fromJson).toList(growable: false);
  }

  // ── Hub Search ──────────────────────────────────────────────────────────

  Future<List<HubModel>> searchHubModels(HubSearchRequest request) async {
    _ensureNotDisposed();
    final response = await _post('/v1/models/hub/search', request.toJson());
    final json = _decodeJson(response);
    final data = (json['data'] as List<dynamic>? ?? const <dynamic>[])
        .cast<Map<String, dynamic>>();
    return data.map(HubModel.fromJson).toList(growable: false);
  }

  // ── Metrics ─────────────────────────────────────────────────────────────

  Future<RuntimeMetrics> metrics() async {
    _ensureNotDisposed();
    final response = await _get('/metrics');
    // Parse Prometheus text format into RuntimeMetrics
    final body = response.body;
    return _parsePrometheusMetrics(body);
  }

  // ── Internal helpers ────────────────────────────────────────────────────

  Future<http.Response> _get(String path) async {
    final uri = Uri.parse('$baseUrl$path');
    final headers = <String, String>{};
    _applyAuth(headers);
    final response = await http.get(uri, headers: headers);
    if (response.statusCode >= 400) {
      throw MaiRuntimeException(-3, 'HTTP ${response.statusCode}: ${response.body}');
    }
    return response;
  }

  Future<http.Response> _post(String path, Map<String, dynamic> body) async {
    final uri = Uri.parse('$baseUrl$path');
    final headers = <String, String>{'Content-Type': 'application/json'};
    _applyAuth(headers);
    final response = await http.post(uri, headers: headers, body: jsonEncode(body));
    if (response.statusCode >= 400) {
      throw MaiRuntimeException(-3, 'HTTP ${response.statusCode}: ${response.body}');
    }
    return response;
  }

  void _applyAuth(Map<String, String> headers) {
    if (apiKey != null && apiKey!.isNotEmpty) {
      headers['Authorization'] = 'Bearer $apiKey';
    }
  }

  Map<String, dynamic> _decodeJson(http.Response response) {
    return jsonDecode(response.body) as Map<String, dynamic>;
  }

  RuntimeMetrics _parsePrometheusMetrics(String body) {
    int extractInt(String name) {
      final pattern = RegExp('$name (\\d+)');
      final match = pattern.firstMatch(body);
      return match != null ? int.tryParse(match.group(1)!) ?? 0 : 0;
    }

    return RuntimeMetrics(
      inferenceTotal: extractInt('mai_inference_total'),
      inferenceErrorsTotal: extractInt('mai_inference_errors_total'),
      activeStreams: extractInt('mai_active_streams'),
      downloadsStartedTotal: extractInt('mai_downloads_started_total'),
      downloadsCompletedTotal: extractInt('mai_downloads_completed_total'),
      downloadsFailedTotal: extractInt('mai_downloads_failed_total'),
      downloadsActive: extractInt('mai_downloads_active'),
      downloadBytesTotal: extractInt('mai_download_bytes_total'),
      ramTotalBytes: extractInt('mai_ram_total_bytes'),
      ramFreeBytes: extractInt('mai_ram_free_bytes'),
    );
  }

  void _ensureNotDisposed() {
    if (_disposed) {
      throw StateError('HTTP MAI runtime has already been disposed');
    }
  }

  void _closeCompletion(int completionId) {
    final completion = _completions.remove(completionId);
    if (completion == null) return;

    completion.subscription?.cancel();
    completion.client.close();
    if (!completion.controller.isClosed) {
      completion.controller.close();
    }
  }
}

class _HttpCompletion {
  _HttpCompletion(this.client, this.controller);

  final http.Client client;
  final StreamController<String> controller;
  StreamSubscription<String>? subscription;
}
