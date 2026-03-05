import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../runtime/models.dart';
import '../../runtime/runtime_provider.dart';
import '../../shared/formatters.dart';
import '../../shared/theme.dart';

class ModelsPage extends ConsumerStatefulWidget {
  const ModelsPage({super.key});

  @override
  ConsumerState<ModelsPage> createState() => _ModelsPageState();
}

class _ModelsPageState extends ConsumerState<ModelsPage> {
  late final TextEditingController _searchController;

  @override
  void initState() {
    super.initState();
    _searchController = TextEditingController(
      text: ref.read(runtimeControllerProvider).hubSearchQuery,
    );
  }

  @override
  void dispose() {
    _searchController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final runtime = ref.watch(runtimeControllerProvider);

    return RefreshIndicator(
      onRefresh: () async {
        await runtime.refreshHubModels();
        await runtime.refreshCatalog();
      },
      child: ListView(
        padding: const EdgeInsets.all(16),
        children: [
          // Section header
          Row(
            children: [
              const Icon(Icons.hub_rounded,
                  size: 22, color: LuminaColors.accent),
              const SizedBox(width: 10),
              Text('Model Hub', style: Theme.of(context).textTheme.titleLarge),
            ],
          ),
          const SizedBox(height: 6),
          const Text(
            'Browse Hugging Face GGUF models. Download, then run locally.',
            style: TextStyle(color: LuminaColors.white60, fontSize: 13),
          ),
          const SizedBox(height: 14),

          // Search bar
          Row(
            children: [
              Expanded(
                child: TextField(
                  controller: _searchController,
                  decoration: InputDecoration(
                    hintText: 'Search: tinyllama, mistral, phi...',
                    prefixIcon: const Icon(Icons.search_rounded, size: 20),
                    suffixIcon: _searchController.text.isNotEmpty
                        ? IconButton(
                            icon: const Icon(Icons.clear_rounded, size: 18),
                            onPressed: () {
                              _searchController.clear();
                              setState(() {});
                            },
                          )
                        : null,
                  ),
                  onChanged: (_) => setState(() {}),
                  onSubmitted: (v) => runtime.searchHubModels(v),
                ),
              ),
              const SizedBox(width: 8),
              FilledButton(
                onPressed: () =>
                    runtime.searchHubModels(_searchController.text),
                style: FilledButton.styleFrom(
                  padding:
                      const EdgeInsets.symmetric(horizontal: 16, vertical: 14),
                ),
                child: const Icon(Icons.search_rounded, size: 20),
              ),
            ],
          ),
          const SizedBox(height: 10),

          // Filter info row
          Row(
            children: [
              Container(
                padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 3),
                decoration: BoxDecoration(
                  color: LuminaColors.emerald.withValues(alpha: 0.10),
                  borderRadius: BorderRadius.circular(6),
                ),
                child: const Row(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    Icon(Icons.verified_rounded,
                        size: 13, color: LuminaColors.emerald),
                    SizedBox(width: 4),
                    Text('GGUF + MLX',
                        style: TextStyle(
                            fontSize: 11, color: LuminaColors.emerald)),
                  ],
                ),
              ),
              const Spacer(),
              Text(
                '${runtime.hubModels.length} models',
                style:
                    const TextStyle(fontSize: 12, color: LuminaColors.white60),
              ),
            ],
          ),
          const SizedBox(height: 16),

          // Hub models
          if (runtime.hubSearchLoading)
            const Padding(
              padding: EdgeInsets.symmetric(vertical: 24),
              child: Center(child: CircularProgressIndicator()),
            )
          else if (runtime.hubModels.isEmpty)
            _EmptyHubCard(
              onSearch: () => runtime.searchHubModels(_searchController.text),
            )
          else
            ...runtime.hubModels.map(
              (model) => Padding(
                padding: const EdgeInsets.only(bottom: 12),
                child: _HubModelCard(
                  model: model,
                  active: runtime.selectedModelId == model.id,
                  busy: runtime.isModelBusy,
                  totalRamBytes: runtime.deviceProfile?.totalRamBytes,
                  onDownload: (file) => runtime.downloadHubFile(model, file),
                  onLoad: () => runtime.loadModel(model.id),
                  onUnload: runtime.unloadModel,
                ),
              ),
            ),

          // Downloaded / Curated section
          const SizedBox(height: 8),
          Row(
            children: [
              const Icon(Icons.folder_rounded,
                  size: 18, color: LuminaColors.white60),
              const SizedBox(width: 8),
              Text('Downloaded / Curated',
                  style: Theme.of(context).textTheme.titleMedium),
            ],
          ),
          const SizedBox(height: 10),
          if (runtime.catalog.isEmpty)
            const Card(
              child: Padding(
                padding: EdgeInsets.all(16),
                child: Text('No local models yet. Download from the Hub above.',
                    style: TextStyle(color: LuminaColors.white60)),
              ),
            )
          else
            ...runtime.catalog.map(
              (model) => Card(
                child: ListTile(
                  leading: Container(
                    width: 40,
                    height: 40,
                    decoration: BoxDecoration(
                      color: LuminaColors.accent.withValues(alpha: 0.12),
                      borderRadius: BorderRadius.circular(10),
                    ),
                    child: const Icon(Icons.smart_toy_rounded,
                        size: 20, color: LuminaColors.accentLight),
                  ),
                  title: Text(model.name,
                      style: const TextStyle(fontWeight: FontWeight.w600)),
                  subtitle: Text(
                    '${model.id} \u00b7 ${model.quantization} \u00b7 ${model.backend.toUpperCase()}',
                    style: const TextStyle(
                        fontSize: 12, color: LuminaColors.white60),
                  ),
                  trailing: Text(
                    model.sizeBytes == null
                        ? '--'
                        : formatBytes(model.sizeBytes!),
                    style: const TextStyle(fontWeight: FontWeight.w600),
                  ),
                ),
              ),
            ),
        ],
      ),
    );
  }
}

class _EmptyHubCard extends StatelessWidget {
  const _EmptyHubCard({required this.onSearch});
  final VoidCallback onSearch;

  @override
  Widget build(BuildContext context) {
    return Card(
      child: Padding(
        padding: const EdgeInsets.all(20),
        child: Column(
          children: [
            Icon(Icons.cloud_download_rounded,
                size: 40, color: LuminaColors.white60.withValues(alpha: 0.5)),
            const SizedBox(height: 12),
            const Text(
              'No hub models loaded yet',
              style: TextStyle(fontWeight: FontWeight.w700),
            ),
            const SizedBox(height: 6),
            const Text(
              'Tap Search to browse available Hugging Face GGUF models.',
              textAlign: TextAlign.center,
              style: TextStyle(color: LuminaColors.white60, fontSize: 13),
            ),
            const SizedBox(height: 14),
            FilledButton.icon(
              onPressed: onSearch,
              icon: const Icon(Icons.search_rounded),
              label: const Text('Search Models'),
            ),
          ],
        ),
      ),
    );
  }
}

class _HubModelCard extends StatelessWidget {
  const _HubModelCard({
    required this.model,
    required this.active,
    required this.busy,
    required this.onDownload,
    required this.onLoad,
    required this.onUnload,
    this.totalRamBytes,
  });

  final HubModel model;
  final bool active;
  final bool busy;
  final int? totalRamBytes;
  final Future<void> Function(HubModelFile file) onDownload;
  final Future<void> Function() onLoad;
  final Future<void> Function() onUnload;

  int? _totalGgufSizeBytes(List<HubModelFile> files) {
    var total = 0;
    var seenSizedFile = false;
    for (final file in files) {
      final size = file.sizeBytes;
      if (size == null) continue;
      total += size;
      seenSizedFile = true;
    }
    return seenSizedFile ? total : null;
  }

  /// Estimate RAM usage as ~2x file size. Returns a fit color.
  Color _ramFitColor(int? sizeBytes) {
    if (sizeBytes == null || totalRamBytes == null || totalRamBytes == 0) {
      return LuminaColors.white60;
    }
    final estimatedRam = sizeBytes * 2;
    final ratio = estimatedRam / totalRamBytes!;
    if (ratio < 0.5) return LuminaColors.emerald;
    if (ratio < 0.8) return LuminaColors.amber;
    return LuminaColors.red;
  }

  @override
  Widget build(BuildContext context) {
    final files = model.ggufFiles;
    final totalSizeBytes = _totalGgufSizeBytes(files);
    final shouldScrollFiles = files.length > 3;
    final filesHeight = (files.length * 74).clamp(120, 260).toDouble();

    return Card(
      shape: active
          ? RoundedRectangleBorder(
              borderRadius: BorderRadius.circular(16),
              side: const BorderSide(color: LuminaColors.emerald, width: 1.5),
            )
          : null,
      child: Padding(
        padding: const EdgeInsets.all(14),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            // Title row
            Row(
              children: [
                Expanded(
                  child: Text(
                    model.id,
                    style: const TextStyle(
                        fontWeight: FontWeight.w700, fontSize: 14),
                  ),
                ),
                if (active) ...[
                  Container(
                    padding:
                        const EdgeInsets.symmetric(horizontal: 10, vertical: 4),
                    decoration: BoxDecoration(
                      color: LuminaColors.emerald.withValues(alpha: 0.15),
                      borderRadius: BorderRadius.circular(999),
                      border: Border.all(
                          color: LuminaColors.emerald.withValues(alpha: 0.4)),
                    ),
                    child: const Row(
                      mainAxisSize: MainAxisSize.min,
                      children: [
                        Icon(Icons.circle,
                            size: 6, color: LuminaColors.emerald),
                        SizedBox(width: 4),
                        Text(
                          'Active',
                          style: TextStyle(
                            color: LuminaColors.emerald,
                            fontWeight: FontWeight.w700,
                            fontSize: 11,
                          ),
                        ),
                      ],
                    ),
                  ),
                ],
              ],
            ),
            const SizedBox(height: 8),

            // Tags
            Wrap(
              spacing: 6,
              runSpacing: 6,
              children: [
                _InfoChip(
                    icon: Icons.download_rounded, text: '${model.downloads}'),
                _InfoChip(icon: Icons.favorite_rounded, text: '${model.likes}'),
                _InfoChip(
                  icon: Icons.storage_rounded,
                  text: totalSizeBytes == null
                      ? 'size: --'
                      : 'size: ${formatBytes(totalSizeBytes)}',
                ),
                ...model.tags.take(3).map((t) => _InfoChip(text: t)),
              ],
            ),
            const SizedBox(height: 12),

            // GGUF files
            if (files.isEmpty)
              const Text('No GGUF files found.',
                  style: TextStyle(color: LuminaColors.white60))
            else
              SizedBox(
                height: shouldScrollFiles ? filesHeight : null,
                child: Scrollbar(
                  child: ListView.separated(
                    primary: false,
                    shrinkWrap: true,
                    physics: shouldScrollFiles
                        ? const AlwaysScrollableScrollPhysics()
                        : const NeverScrollableScrollPhysics(),
                    itemCount: files.length,
                    separatorBuilder: (context, index) =>
                        const SizedBox(height: 6),
                    itemBuilder: (context, index) {
                      final file = files[index];
                      return Container(
                        padding: const EdgeInsets.symmetric(
                            horizontal: 10, vertical: 8),
                        decoration:
                            glassDecoration(fillOpacity: 0.03, radius: 10),
                        child: Column(
                          crossAxisAlignment: CrossAxisAlignment.start,
                          children: [
                            Row(
                              children: [
                                const Icon(Icons.description_rounded,
                                    size: 16, color: LuminaColors.white60),
                                const SizedBox(width: 8),
                                Expanded(
                                  child: Column(
                                    crossAxisAlignment:
                                        CrossAxisAlignment.start,
                                    children: [
                                      Row(
                                        children: [
                                          Text(
                                            file.isMlx
                                                ? 'MLX'
                                                : (file.quantization ?? 'GGUF'),
                                            style: const TextStyle(
                                                fontWeight: FontWeight.w600,
                                                fontSize: 13),
                                          ),
                                          if (file.isMlx) ...[
                                            const SizedBox(width: 6),
                                            Container(
                                              padding:
                                                  const EdgeInsets.symmetric(
                                                      horizontal: 6,
                                                      vertical: 2),
                                              decoration: BoxDecoration(
                                                color: LuminaColors.accent
                                                    .withValues(alpha: 0.15),
                                                borderRadius:
                                                    BorderRadius.circular(4),
                                              ),
                                              child: const Text(
                                                'Apple Silicon',
                                                style: TextStyle(
                                                    fontSize: 9,
                                                    color:
                                                        LuminaColors.accentLight),
                                              ),
                                            ),
                                          ],
                                        ],
                                      ),
                                      Text(
                                        file.filename,
                                        maxLines: 1,
                                        overflow: TextOverflow.ellipsis,
                                        style: const TextStyle(
                                            fontSize: 11,
                                            color: LuminaColors.white60),
                                      ),
                                    ],
                                  ),
                                ),
                              ],
                            ),
                            const SizedBox(height: 8),
                            Row(
                              children: [
                                Text(
                                  file.sizeBytes == null
                                      ? '--'
                                      : formatBytes(file.sizeBytes!),
                                  style: TextStyle(
                                    fontSize: 12,
                                    fontWeight: FontWeight.w600,
                                    color: _ramFitColor(file.sizeBytes),
                                  ),
                                ),
                                const Spacer(),
                                SizedBox(
                                  height: 32,
                                  child: FilledButton(
                                    onPressed: () => onDownload(file),
                                    style: FilledButton.styleFrom(
                                      padding: const EdgeInsets.symmetric(
                                          horizontal: 12),
                                      textStyle: const TextStyle(fontSize: 12),
                                    ),
                                    child: const Icon(Icons.download_rounded,
                                        size: 16),
                                  ),
                                ),
                              ],
                            ),
                          ],
                        ),
                      );
                    },
                  ),
                ),
              ),
            const SizedBox(height: 10),

            // Action buttons
            Row(
              children: [
                OutlinedButton.icon(
                  onPressed: busy ? null : onLoad,
                  icon: const Icon(Icons.play_arrow_rounded, size: 18),
                  label: const Text('Load'),
                ),
                const SizedBox(width: 8),
                OutlinedButton.icon(
                  onPressed: busy || !active ? null : onUnload,
                  icon: const Icon(Icons.stop_rounded, size: 18),
                  label: const Text('Unload'),
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }
}

class _InfoChip extends StatelessWidget {
  const _InfoChip({this.icon, required this.text});
  final IconData? icon;
  final String text;

  @override
  Widget build(BuildContext context) {
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 4),
      decoration: BoxDecoration(
        color: Colors.white.withValues(alpha: 0.05),
        borderRadius: BorderRadius.circular(999),
        border: Border.all(color: Colors.white.withValues(alpha: 0.08)),
      ),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          if (icon != null) ...[
            Icon(icon, size: 12, color: LuminaColors.white60),
            const SizedBox(width: 4),
          ],
          Text(text,
              style:
                  const TextStyle(fontSize: 11, fontWeight: FontWeight.w600)),
        ],
      ),
    );
  }
}
