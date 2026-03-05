import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../runtime/models.dart';
import '../../runtime/runtime_provider.dart';
import '../../shared/formatters.dart';
import '../../shared/theme.dart';

enum DownloadSourceKind { catalog, localPath, remoteUrl }

class DownloadsPage extends ConsumerStatefulWidget {
  const DownloadsPage({super.key});

  @override
  ConsumerState<DownloadsPage> createState() => _DownloadsPageState();
}

class _DownloadsPageState extends ConsumerState<DownloadsPage> {
  final TextEditingController _sourceController = TextEditingController();
  final TextEditingController _destinationController = TextEditingController();
  final TextEditingController _modelIdController =
      TextEditingController(text: 'tinyllama-1b-q4');
  final TextEditingController _modelNameController =
      TextEditingController(text: 'TinyLlama Q4');
  final TextEditingController _quantController =
      TextEditingController(text: 'Q4KM');

  DownloadSourceKind _sourceKind = DownloadSourceKind.catalog;
  String? _selectedCatalogModelId = 'tinyllama-1b-q4';

  @override
  void dispose() {
    _sourceController.dispose();
    _destinationController.dispose();
    _modelIdController.dispose();
    _modelNameController.dispose();
    _quantController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final runtime = ref.watch(runtimeControllerProvider);
    CatalogModel? selectedCatalog;
    if (_selectedCatalogModelId != null) {
      for (final model in runtime.catalog) {
        if (model.id == _selectedCatalogModelId) {
          selectedCatalog = model;
          break;
        }
      }
    }
    final selectedCatalogSizeBytes = selectedCatalog?.sizeBytes;
    final selectedCatalogSizeText = selectedCatalogSizeBytes == null
        ? ''
        : ' \u00b7 ${formatBytes(selectedCatalogSizeBytes)}';

    return LuminaBackdrop(
      child: Scaffold(
        backgroundColor: Colors.transparent,
        appBar: AppBar(
          title: const Text('Downloads'),
          actions: [
            IconButton(
              onPressed: runtime.initialized ? runtime.refreshDownloads : null,
              icon: const Icon(Icons.refresh_rounded, size: 22),
            ),
            const SizedBox(width: 4),
          ],
        ),
        body: Padding(
          padding: const EdgeInsets.fromLTRB(10, 0, 10, 10),
          child: ClipRRect(
            borderRadius: BorderRadius.circular(22),
            child: Container(
              decoration: BoxDecoration(
                gradient: LuminaGradients.card,
                border: Border.all(color: Colors.white.withValues(alpha: 0.08)),
              ),
              child: RefreshIndicator(
                onRefresh: runtime.refreshDownloads,
                child: ListView(
                  padding: const EdgeInsets.all(16),
                  children: [
                    // Source selector
                    SegmentedButton<DownloadSourceKind>(
                      segments: const [
                        ButtonSegment(
                          value: DownloadSourceKind.catalog,
                          label: Text('Catalog'),
                          icon: Icon(Icons.auto_awesome_rounded, size: 18),
                        ),
                        ButtonSegment(
                          value: DownloadSourceKind.localPath,
                          label: Text('Local'),
                          icon: Icon(Icons.folder_rounded, size: 18),
                        ),
                        ButtonSegment(
                          value: DownloadSourceKind.remoteUrl,
                          label: Text('URL'),
                          icon: Icon(Icons.language_rounded, size: 18),
                        ),
                      ],
                      selected: <DownloadSourceKind>{_sourceKind},
                      onSelectionChanged: (s) =>
                          setState(() => _sourceKind = s.first),
                    ),
                    const SizedBox(height: 16),

                    // Source inputs
                    if (_sourceKind == DownloadSourceKind.catalog) ...[
                      DropdownButtonFormField<String>(
                        initialValue:
                            _resolveSelectedCatalogId(runtime.catalog),
                        decoration:
                            const InputDecoration(labelText: 'Catalog Model'),
                        items: runtime.catalog
                            .map((m) => DropdownMenuItem(
                                  value: m.id,
                                  child: Text('${m.name} (${m.quantization})'),
                                ))
                            .toList(growable: false),
                        onChanged: runtime.catalog.isEmpty
                            ? null
                            : (v) {
                                if (v == null) return;
                                setState(() => _selectedCatalogModelId = v);
                                _applyCatalogSelection(runtime.catalog
                                    .firstWhere((m) => m.id == v));
                              },
                      ),
                      const SizedBox(height: 12),
                      // Model preview
                      Card(
                        child: Padding(
                          padding: const EdgeInsets.all(14),
                          child: Row(
                            children: [
                              Container(
                                width: 40,
                                height: 40,
                                decoration: BoxDecoration(
                                  color: LuminaColors.accent
                                      .withValues(alpha: 0.12),
                                  borderRadius: BorderRadius.circular(10),
                                ),
                                child: const Icon(Icons.smart_toy_rounded,
                                    size: 20, color: LuminaColors.accentLight),
                              ),
                              const SizedBox(width: 12),
                              Expanded(
                                child: Column(
                                  crossAxisAlignment: CrossAxisAlignment.start,
                                  children: [
                                    Text(_modelNameController.text,
                                        style: const TextStyle(
                                            fontWeight: FontWeight.w700)),
                                    const SizedBox(height: 2),
                                    Text(
                                      'ID: ${_modelIdController.text} \u00b7 ${_quantController.text}$selectedCatalogSizeText',
                                      style: const TextStyle(
                                          fontSize: 12,
                                          color: LuminaColors.white60),
                                    ),
                                  ],
                                ),
                              ),
                            ],
                          ),
                        ),
                      ),
                    ] else ...[
                      TextField(
                        controller: _sourceController,
                        decoration: InputDecoration(
                          labelText: _sourceKind == DownloadSourceKind.localPath
                              ? 'Source Path'
                              : 'Source URL',
                          hintText: _sourceKind == DownloadSourceKind.localPath
                              ? '/path/to/model.gguf'
                              : 'https://example.com/model.gguf',
                          prefixIcon: Icon(
                            _sourceKind == DownloadSourceKind.localPath
                                ? Icons.folder_rounded
                                : Icons.link_rounded,
                            size: 20,
                          ),
                        ),
                      ),
                      const SizedBox(height: 10),
                      TextField(
                        controller: _destinationController,
                        decoration: const InputDecoration(
                          labelText: 'Destination Path',
                          hintText: '/path/to/save/model.gguf',
                          prefixIcon: Icon(Icons.save_rounded, size: 20),
                        ),
                      ),
                      const SizedBox(height: 10),
                      Row(
                        children: [
                          Expanded(
                            child: TextField(
                              controller: _modelIdController,
                              decoration:
                                  const InputDecoration(labelText: 'Model ID'),
                            ),
                          ),
                          const SizedBox(width: 8),
                          Expanded(
                            child: TextField(
                              controller: _quantController,
                              decoration:
                                  const InputDecoration(labelText: 'Quant'),
                            ),
                          ),
                        ],
                      ),
                      const SizedBox(height: 10),
                      TextField(
                        controller: _modelNameController,
                        decoration:
                            const InputDecoration(labelText: 'Model Name'),
                      ),
                    ],
                    const SizedBox(height: 16),

                    // Action button
                    SizedBox(
                      width: double.infinity,
                      height: 48,
                      child: FilledButton.icon(
                        onPressed: runtime.initialized ? _startDownload : null,
                        icon: const Icon(Icons.download_rounded),
                        label: const Text('Start Download'),
                      ),
                    ),
                    const SizedBox(height: 24),

                    // Jobs list
                    Row(
                      children: [
                        const Icon(Icons.list_rounded,
                            size: 20, color: LuminaColors.white60),
                        const SizedBox(width: 8),
                        Text('Download Jobs',
                            style: Theme.of(context).textTheme.titleMedium),
                        const Spacer(),
                        Text(
                          '${runtime.downloads.length} jobs',
                          style: const TextStyle(
                              fontSize: 12, color: LuminaColors.white60),
                        ),
                      ],
                    ),
                    const SizedBox(height: 10),
                    if (runtime.downloads.isEmpty)
                      Card(
                        child: Padding(
                          padding: const EdgeInsets.all(20),
                          child: Column(
                            children: [
                              Icon(Icons.inbox_rounded,
                                  size: 36,
                                  color: LuminaColors.white60
                                      .withValues(alpha: 0.4)),
                              const SizedBox(height: 8),
                              const Text('No downloads yet',
                                  style:
                                      TextStyle(color: LuminaColors.white60)),
                            ],
                          ),
                        ),
                      )
                    else
                      ...runtime.downloads.map(
                        (job) => Padding(
                          padding: const EdgeInsets.only(bottom: 10),
                          child: _DownloadJobTile(
                            job: job,
                            onRetry: runtime.retryDownload,
                            onCancel: runtime.cancelDownload,
                            onDelete: runtime.deleteDownload,
                          ),
                        ),
                      ),
                  ],
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }

  Future<void> _startDownload() async {
    final runtime = ref.read(runtimeControllerProvider);
    CatalogModel? selectedCatalog;
    if (_sourceKind == DownloadSourceKind.catalog) {
      for (final model in runtime.catalog) {
        if (model.id == _selectedCatalogModelId) {
          selectedCatalog = model;
          break;
        }
      }
    }

    if (_sourceKind == DownloadSourceKind.catalog && selectedCatalog == null) {
      ScaffoldMessenger.of(context).showSnackBar(
        const SnackBar(
            content: Text('Catalog is empty. Add models or refresh runtime.')),
      );
      return;
    }

    final request = DownloadRequest(
      sourcePath: _sourceKind == DownloadSourceKind.localPath
          ? _sourceController.text.trim()
          : null,
      sourceUrl: _sourceKind == DownloadSourceKind.remoteUrl
          ? _sourceController.text.trim()
          : null,
      destinationPath: _sourceKind == DownloadSourceKind.catalog
          ? null
          : _destinationController.text.trim(),
      id: selectedCatalog?.id ?? _modelIdController.text.trim(),
      name: selectedCatalog?.name ?? _modelNameController.text.trim(),
      quant: selectedCatalog?.quantization ?? _quantController.text.trim(),
      backend: selectedCatalog?.backend,
    );

    if (!request.hasAtMostOneSource ||
        (_sourceKind != DownloadSourceKind.catalog &&
            !request.hasExactlyOneSource) ||
        (_sourceKind != DownloadSourceKind.catalog &&
            (request.destinationPath == null ||
                request.destinationPath!.isEmpty)) ||
        request.id.isEmpty) {
      ScaffoldMessenger.of(context).showSnackBar(
        const SnackBar(content: Text('Fill in all required fields.')),
      );
      return;
    }

    await runtime.startDownload(request);
  }

  String? _resolveSelectedCatalogId(List<CatalogModel> models) {
    if (models.isEmpty) return null;
    if (models.any((m) => m.id == _selectedCatalogModelId)) {
      return _selectedCatalogModelId;
    }
    final fallback = models.first.id;
    _selectedCatalogModelId = fallback;
    _applyCatalogSelection(models.first);
    return fallback;
  }

  void _applyCatalogSelection(CatalogModel model) {
    _modelIdController.text = model.id;
    _modelNameController.text = model.name;
    _quantController.text = model.quantization;
  }
}

/// Public widget for rendering a list of download jobs. Used by tests.
class DownloadJobsList extends StatelessWidget {
  const DownloadJobsList({
    super.key,
    required this.jobs,
    required this.onRetry,
    required this.onCancel,
    required this.onDelete,
  });

  final List<DownloadJob> jobs;
  final Future<void> Function(String jobId) onRetry;
  final Future<void> Function(String jobId) onCancel;
  final Future<void> Function(String jobId, {bool deleteFile}) onDelete;

  @override
  Widget build(BuildContext context) {
    if (jobs.isEmpty) {
      return Card(
        child: Padding(
          padding: const EdgeInsets.all(20),
          child: Column(
            children: [
              Icon(Icons.inbox_rounded,
                  size: 36, color: LuminaColors.white60.withValues(alpha: 0.4)),
              const SizedBox(height: 8),
              const Text('No downloads yet',
                  style: TextStyle(color: LuminaColors.white60)),
            ],
          ),
        ),
      );
    }
    return Column(
      children: jobs
          .map((job) => Padding(
                padding: const EdgeInsets.only(bottom: 10),
                child: _DownloadJobTile(
                  job: job,
                  onRetry: onRetry,
                  onCancel: onCancel,
                  onDelete: onDelete,
                ),
              ))
          .toList(growable: false),
    );
  }
}

class _DownloadJobTile extends StatelessWidget {
  const _DownloadJobTile({
    required this.job,
    required this.onRetry,
    required this.onCancel,
    required this.onDelete,
  });

  final DownloadJob job;
  final Future<void> Function(String jobId) onRetry;
  final Future<void> Function(String jobId) onCancel;
  final Future<void> Function(String jobId, {bool deleteFile}) onDelete;

  @override
  Widget build(BuildContext context) {
    final progress = ((job.progressPct ?? 0) / 100).clamp(0.0, 1.0);
    final (statusColor, statusIcon) = switch (job.status) {
      'succeeded' => (LuminaColors.emerald, Icons.check_circle_rounded),
      'failed' => (LuminaColors.red, Icons.cancel_rounded),
      'running' => (const Color(0xFF3B82F6), Icons.sync_rounded),
      _ => (LuminaColors.amber, Icons.schedule_rounded),
    };

    return Card(
      child: Padding(
        padding: const EdgeInsets.all(14),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            // Header
            Row(
              children: [
                Icon(statusIcon, size: 18, color: statusColor),
                const SizedBox(width: 8),
                Expanded(
                  child: Text(
                    '${job.modelName} (${job.quant})',
                    style: const TextStyle(fontWeight: FontWeight.w700),
                  ),
                ),
                Container(
                  key: ValueKey<String>('status-${job.jobId}'),
                  padding:
                      const EdgeInsets.symmetric(horizontal: 8, vertical: 3),
                  decoration: BoxDecoration(
                    color: statusColor.withValues(alpha: 0.12),
                    borderRadius: BorderRadius.circular(999),
                    border:
                        Border.all(color: statusColor.withValues(alpha: 0.4)),
                  ),
                  child: Text(
                    job.status,
                    style: TextStyle(
                        color: statusColor,
                        fontWeight: FontWeight.w600,
                        fontSize: 11),
                  ),
                ),
              ],
            ),
            const SizedBox(height: 10),

            // Progress bar
            ClipRRect(
              borderRadius: BorderRadius.circular(999),
              child: LinearProgressIndicator(
                value: job.status == 'succeeded' ? 1.0 : progress,
                minHeight: 6,
                backgroundColor: Colors.white.withValues(alpha: 0.06),
                valueColor: AlwaysStoppedAnimation<Color>(statusColor),
              ),
            ),
            const SizedBox(height: 8),

            // Progress text
            Row(
              children: [
                Text(
                  formatPercent(job.progressPct),
                  style: const TextStyle(
                      fontWeight: FontWeight.w600, fontSize: 13),
                ),
                const Spacer(),
                Flexible(
                  child: Text(
                    '${formatBytes(job.downloadedBytes)} / ${job.totalBytes == null ? "--" : formatBytes(job.totalBytes!)}',
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    textAlign: TextAlign.end,
                    style: const TextStyle(
                        fontSize: 12, color: LuminaColors.white60),
                  ),
                ),
              ],
            ),
            const SizedBox(height: 4),

            // Details
            Text(
              'Source: ${job.source}',
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              style: const TextStyle(fontSize: 11, color: LuminaColors.white60),
            ),
            Text(
              'Updated: ${formatTimestamp(job.updatedAt)}',
              style: const TextStyle(fontSize: 11, color: LuminaColors.white60),
            ),

            // Error
            if (job.error != null && job.error!.isNotEmpty) ...[
              const SizedBox(height: 6),
              Container(
                width: double.infinity,
                padding: const EdgeInsets.all(8),
                decoration: BoxDecoration(
                  color: LuminaColors.red.withValues(alpha: 0.08),
                  borderRadius: BorderRadius.circular(8),
                ),
                child: Text(
                  job.error!,
                  style: const TextStyle(fontSize: 12, color: LuminaColors.red),
                ),
              ),
            ],

            // Job actions
            if (job.status == 'failed' ||
                job.status == 'running' ||
                job.status == 'queued' ||
                job.status == 'succeeded' ||
                job.status == 'cancelled') ...[
              const SizedBox(height: 8),
              Wrap(
                spacing: 8,
                runSpacing: 8,
                children: [
                  if (job.status == 'failed')
                    OutlinedButton(
                      key: ValueKey<String>('retry-${job.jobId}'),
                      onPressed: () => onRetry(job.jobId),
                      child: const Text('Retry'),
                    ),
                  if (job.status == 'running' || job.status == 'queued')
                    OutlinedButton(
                      onPressed: () => onCancel(job.jobId),
                      child: const Text('Cancel'),
                    ),
                  OutlinedButton(
                    onPressed: () => onDelete(job.jobId),
                    child: const Text('Delete'),
                  ),
                ],
              ),
            ],
          ],
        ),
      ),
    );
  }
}
