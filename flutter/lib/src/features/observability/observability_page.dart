import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../runtime/runtime_provider.dart';
import '../../shared/formatters.dart';
import '../../shared/theme.dart';

class ObservabilityPage extends ConsumerWidget {
  const ObservabilityPage({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final runtime = ref.watch(runtimeControllerProvider);
    final metrics = runtime.metrics;
    final profile = runtime.deviceProfile;
    final hasMetrics = metrics != null;

    return ListView(
      padding: const EdgeInsets.all(16),
      children: [
        // Header
        Row(
          children: [
            const Icon(Icons.monitor_heart_rounded, size: 22, color: LuminaColors.accent),
            const SizedBox(width: 10),
            Text('Engine Stats', style: Theme.of(context).textTheme.titleLarge),
            const Spacer(),
            IconButton(
              onPressed: runtime.initialized ? runtime.refreshMetrics : null,
              icon: const Icon(Icons.refresh_rounded, size: 20),
              visualDensity: VisualDensity.compact,
            ),
          ],
        ),
        const SizedBox(height: 16),

        // RAM card — prominent
        if (hasMetrics) ...[
          _RamCard(
            ramPct: runtime.ramUsagePct,
            totalBytes: metrics!.ramTotalBytes,
            freeBytes: metrics!.ramFreeBytes,
          ),
          const SizedBox(height: 16),
        ] else ...[
          Card(
            child: Padding(
              padding: const EdgeInsets.all(14),
              child: Row(
                children: [
                  Icon(
                    Icons.monitor_heart_outlined,
                    size: 18,
                    color: LuminaColors.white60.withValues(alpha: 0.7),
                  ),
                  const SizedBox(width: 10),
                  const Expanded(
                    child: Text(
                      'No runtime metrics yet. Start runtime and tap refresh.',
                      style: TextStyle(color: LuminaColors.white60),
                    ),
                  ),
                ],
              ),
            ),
          ),
          const SizedBox(height: 16),
        ],

        // Device profile card
        if (profile != null) ...[
          const _SectionTitle('Device Profile'),
          const SizedBox(height: 8),
          Card(
            child: Padding(
              padding: const EdgeInsets.all(14),
              child: Column(
                children: [
                  _ProfileRow(Icons.developer_board_rounded, 'CPU', '${profile.cpuCores} cores \u00b7 ${profile.cpuArch}'),
                  _ProfileRow(Icons.memory_rounded, 'RAM', '${formatBytes(profile.totalRamBytes)} total \u00b7 ${formatBytes(profile.freeRamBytes)} free'),
                  _ProfileRow(
                    Icons.videogame_asset_rounded,
                    'GPU',
                    profile.hasGpu ? profile.gpuType : 'None',
                  ),
                  _ProfileRow(Icons.phone_android_rounded, 'Platform', profile.platform),
                  if (profile.batteryLevel != null)
                    _ProfileRow(
                      profile.isCharging ? Icons.battery_charging_full_rounded : Icons.battery_std_rounded,
                      'Battery',
                      '${(profile.batteryLevel! * 100).toStringAsFixed(0)}%${profile.isCharging ? " (Charging)" : ""}',
                    ),
                  _ProfileRow(Icons.storage_rounded, 'Storage', formatBytes(profile.availableStorageBytes)),
                  if (profile.benchmarkTokensPerSec != null)
                    _ProfileRow(Icons.speed_rounded, 'Benchmark', '${profile.benchmarkTokensPerSec!.toStringAsFixed(1)} T/s'),
                ],
              ),
            ),
          ),
          const SizedBox(height: 16),
        ],

        // Error + debug console
        const _SectionTitle('Debug'),
        const SizedBox(height: 8),
        if (runtime.lastErrorDetails != null) ...[
          Card(
            child: Padding(
              padding: const EdgeInsets.all(12),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Row(
                    children: [
                      const Icon(Icons.error_outline_rounded, size: 18, color: LuminaColors.red),
                      const SizedBox(width: 8),
                      const Expanded(
                        child: Text(
                          'Last Error',
                          style: TextStyle(fontWeight: FontWeight.w700),
                        ),
                      ),
                      IconButton(
                        tooltip: 'Copy error',
                        onPressed: () async {
                          await Clipboard.setData(
                            ClipboardData(text: runtime.lastErrorDetails!),
                          );
                          if (!context.mounted) return;
                          ScaffoldMessenger.of(context).showSnackBar(
                            const SnackBar(content: Text('Error copied')),
                          );
                        },
                        icon: const Icon(Icons.copy_rounded, size: 18),
                        visualDensity: VisualDensity.compact,
                      ),
                    ],
                  ),
                  const SizedBox(height: 8),
                  SelectableText(
                    runtime.lastErrorDetails!,
                    style: const TextStyle(
                      fontSize: 12,
                      color: LuminaColors.white87,
                      height: 1.35,
                    ),
                  ),
                ],
              ),
            ),
          ),
          const SizedBox(height: 10),
        ],
        Card(
          child: Padding(
            padding: const EdgeInsets.all(12),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                const Text('Runtime Log', style: TextStyle(fontWeight: FontWeight.w700)),
                const SizedBox(height: 8),
                if (runtime.debugEntries.isEmpty)
                  const Text('No events yet', style: TextStyle(color: LuminaColors.white60))
                else
                  ...runtime.debugEntries.take(20).map(
                        (entry) => Padding(
                          padding: const EdgeInsets.only(bottom: 6),
                          child: Text(
                            '[${entry.timestamp.toIso8601String()}] ${entry.level.toUpperCase()} ${entry.message}',
                            style: const TextStyle(
                              fontSize: 11,
                              color: LuminaColors.white60,
                              height: 1.3,
                            ),
                          ),
                        ),
                      ),
              ],
            ),
          ),
        ),
        const SizedBox(height: 16),

        // Inference metrics
        if (hasMetrics) ...[
          const _SectionTitle('Inference'),
          const SizedBox(height: 8),
          Row(
            children: [
              Expanded(child: _StatCard(label: 'Completions', value: '${metrics!.inferenceTotal}', icon: Icons.chat_rounded, color: LuminaColors.accent)),
              const SizedBox(width: 10),
              Expanded(child: _StatCard(label: 'Active', value: '${metrics!.activeStreams}', icon: Icons.stream_rounded, color: LuminaColors.emerald)),
              const SizedBox(width: 10),
              Expanded(child: _StatCard(label: 'Errors', value: '${metrics!.inferenceErrorsTotal}', icon: Icons.error_outline_rounded, color: LuminaColors.red)),
            ],
          ),
          const SizedBox(height: 10),
          Row(
            children: [
              Expanded(
                child: _StatCard(
                  label: 'Est. T/s',
                  value: runtime.estimatedTokensPerSec?.toStringAsFixed(1) ?? '--',
                  icon: Icons.speed_rounded,
                  color: LuminaColors.emerald,
                ),
              ),
              const SizedBox(width: 10),
              Expanded(
                child: _StatCard(
                  label: 'Model',
                  value: runtime.selectedModelId,
                  icon: Icons.smart_toy_rounded,
                  color: LuminaColors.accentLight,
                  small: true,
                ),
              ),
            ],
          ),
          const SizedBox(height: 16),
        ],

        // Downloads metrics
        if (hasMetrics) ...[
          const _SectionTitle('Downloads'),
          const SizedBox(height: 8),
          Row(
            children: [
              Expanded(child: _StatCard(label: 'Started', value: '${metrics!.downloadsStartedTotal}', icon: Icons.download_rounded, color: LuminaColors.accent)),
              const SizedBox(width: 10),
              Expanded(child: _StatCard(label: 'Done', value: '${metrics!.downloadsCompletedTotal}', icon: Icons.check_circle_rounded, color: LuminaColors.emerald)),
              const SizedBox(width: 10),
              Expanded(child: _StatCard(label: 'Failed', value: '${metrics!.downloadsFailedTotal}', icon: Icons.cancel_rounded, color: LuminaColors.red)),
            ],
          ),
          const SizedBox(height: 10),
          Row(
            children: [
              Expanded(child: _StatCard(label: 'Active', value: '${metrics!.downloadsActive}', icon: Icons.sync_rounded, color: LuminaColors.amber)),
              const SizedBox(width: 10),
              Expanded(child: _StatCard(label: 'Downloaded', value: formatBytes(metrics!.downloadBytesTotal), icon: Icons.data_usage_rounded, color: LuminaColors.accentLight)),
            ],
          ),
        ],
      ],
    );
  }
}

class _SectionTitle extends StatelessWidget {
  const _SectionTitle(this.title);
  final String title;

  @override
  Widget build(BuildContext context) {
    return Text(
      title,
      style: const TextStyle(
        fontWeight: FontWeight.w700,
        fontSize: 14,
        color: LuminaColors.white60,
        letterSpacing: 0.5,
      ),
    );
  }
}

class _RamCard extends StatelessWidget {
  const _RamCard({
    required this.ramPct,
    required this.totalBytes,
    required this.freeBytes,
  });

  final double ramPct;
  final int totalBytes;
  final int freeBytes;

  @override
  Widget build(BuildContext context) {
    final color = ramColor(ramPct);
    final usedBytes = totalBytes - freeBytes;

    return Card(
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Column(
          children: [
            Row(
              children: [
                Icon(Icons.memory_rounded, size: 20, color: color),
                const SizedBox(width: 10),
                const Text('Memory Usage', style: TextStyle(fontWeight: FontWeight.w700)),
                const Spacer(),
                Text(
                  '${(ramPct * 100).toStringAsFixed(1)}%',
                  style: TextStyle(fontWeight: FontWeight.w800, fontSize: 22, color: color),
                ),
              ],
            ),
            const SizedBox(height: 12),
            ClipRRect(
              borderRadius: BorderRadius.circular(999),
              child: LinearProgressIndicator(
                value: ramPct,
                minHeight: 8,
                backgroundColor: Colors.white.withValues(alpha: 0.06),
                valueColor: AlwaysStoppedAnimation<Color>(color),
              ),
            ),
            const SizedBox(height: 10),
            Row(
              mainAxisAlignment: MainAxisAlignment.spaceBetween,
              children: [
                Text('Used: ${formatBytes(usedBytes)}', style: const TextStyle(fontSize: 12, color: LuminaColors.white60)),
                Text('Free: ${formatBytes(freeBytes)}', style: const TextStyle(fontSize: 12, color: LuminaColors.white60)),
                Text('Total: ${formatBytes(totalBytes)}', style: const TextStyle(fontSize: 12, color: LuminaColors.white60)),
              ],
            ),
          ],
        ),
      ),
    );
  }
}

class _StatCard extends StatelessWidget {
  const _StatCard({
    required this.label,
    required this.value,
    required this.icon,
    required this.color,
    this.small = false,
  });

  final String label;
  final String value;
  final IconData icon;
  final Color color;
  final bool small;

  @override
  Widget build(BuildContext context) {
    return Card(
      child: Padding(
        padding: const EdgeInsets.all(12),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              children: [
                Icon(icon, size: 14, color: color),
                const SizedBox(width: 6),
                Text(label, style: const TextStyle(fontSize: 11, color: LuminaColors.white60)),
              ],
            ),
            const SizedBox(height: 8),
            Text(
              value,
              style: TextStyle(
                fontWeight: FontWeight.w700,
                fontSize: small ? 13 : 18,
              ),
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
            ),
          ],
        ),
      ),
    );
  }
}

class _ProfileRow extends StatelessWidget {
  const _ProfileRow(this.icon, this.label, this.value);
  final IconData icon;
  final String label;
  final String value;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 4),
      child: Row(
        children: [
          Icon(icon, size: 16, color: LuminaColors.white60),
          const SizedBox(width: 10),
          SizedBox(
            width: 70,
            child: Text(label, style: const TextStyle(fontSize: 13, color: LuminaColors.white60)),
          ),
          Expanded(
            child: Text(value, style: const TextStyle(fontSize: 13, fontWeight: FontWeight.w600)),
          ),
        ],
      ),
    );
  }
}
