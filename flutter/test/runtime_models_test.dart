import 'package:flutter_test/flutter_test.dart';
import 'package:mai_flutter/src/runtime/models.dart';

void main() {
  test('download request validates exactly one source', () {
    final validPath = DownloadRequest(
      sourcePath: '/tmp/model.gguf',
      sourceUrl: null,
      destinationPath: '/tmp/out.gguf',
      id: 'm1',
      name: 'Model 1',
      quant: 'Q4KM',
    );

    final validUrl = DownloadRequest(
      sourcePath: null,
      sourceUrl: 'https://example.com/model.gguf',
      destinationPath: '/tmp/out.gguf',
      id: 'm1',
      name: 'Model 1',
      quant: 'Q4KM',
    );

    final invalidBoth = DownloadRequest(
      sourcePath: '/tmp/model.gguf',
      sourceUrl: 'https://example.com/model.gguf',
      destinationPath: '/tmp/out.gguf',
      id: 'm1',
      name: 'Model 1',
      quant: 'Q4KM',
    );

    final invalidNone = DownloadRequest(
      sourcePath: null,
      sourceUrl: null,
      destinationPath: '/tmp/out.gguf',
      id: 'm1',
      name: 'Model 1',
      quant: 'Q4KM',
    );

    expect(validPath.hasExactlyOneSource, isTrue);
    expect(validUrl.hasExactlyOneSource, isTrue);
    expect(invalidBoth.hasExactlyOneSource, isFalse);
    expect(invalidNone.hasExactlyOneSource, isFalse);
  });

  test('runtime metrics parses json payload', () {
    final metrics = RuntimeMetrics.fromJson(<String, dynamic>{
      'inference_total': 8,
      'inference_errors_total': 1,
      'active_streams': 0,
      'downloads_started_total': 5,
      'downloads_completed_total': 4,
      'downloads_failed_total': 1,
      'downloads_active': 0,
      'download_bytes_total': 2048,
      'ram_total_bytes': 4096,
      'ram_free_bytes': 1024,
    });

    expect(metrics.inferenceTotal, 8);
    expect(metrics.downloadsCompletedTotal, 4);
    expect(metrics.ramFreeBytes, 1024);
  });
}
