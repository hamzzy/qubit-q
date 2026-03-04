import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mai_flutter/src/features/downloads/downloads_page.dart';
import 'package:mai_flutter/src/runtime/models.dart';

void main() {
  testWidgets('download list renders failed job with retry button', (tester) async {
    String? retriedJob;

    final job = DownloadJob(
      jobId: 'dl-123',
      modelId: 'tinyllama-1b-q4',
      modelName: 'TinyLlama Q4',
      quant: 'Q4KM',
      source: 'file:///tmp/in.gguf',
      destinationPath: '/tmp/out.gguf',
      status: 'failed',
      resumedFromBytes: 256,
      downloadedBytes: 1024,
      totalBytes: 4096,
      progressPct: 25,
      retries: 1,
      createdAt: DateTime.now().toUtc().toIso8601String(),
      updatedAt: DateTime.now().toUtc().toIso8601String(),
      completedAt: DateTime.now().toUtc().toIso8601String(),
      error: 'network timeout',
    );

    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: DownloadJobsList(
            jobs: <DownloadJob>[job],
            onRetry: (jobId) async {
              retriedJob = jobId;
            },
            onCancel: (_) async {},
            onDelete: (_, {deleteFile = false}) async {},
          ),
        ),
      ),
    );

    expect(find.text('failed'), findsOneWidget);
    expect(find.byKey(const ValueKey<String>('retry-dl-123')), findsOneWidget);

    await tester.tap(find.byKey(const ValueKey<String>('retry-dl-123')));
    await tester.pump();

    expect(retriedJob, 'dl-123');
  });
}
