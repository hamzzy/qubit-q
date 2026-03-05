import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'src/features/chat/chat_page.dart';
import 'src/features/downloads/downloads_page.dart';
import 'src/features/models/models_page.dart';
import 'src/features/observability/observability_page.dart';
import 'src/runtime/runtime_controller.dart';
import 'src/runtime/runtime_provider.dart';
import 'src/shared/theme.dart';

void main() {
  runApp(const ProviderScope(child: MaiFlutterApp()));
}

class MaiFlutterApp extends StatelessWidget {
  const MaiFlutterApp({super.key});

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      title: 'MAI',
      debugShowCheckedModeBanner: false,
      theme: ThemeData(
        brightness: Brightness.dark,
        scaffoldBackgroundColor: LuminaColors.bg,
        colorScheme: const ColorScheme.dark(
          primary: LuminaColors.accent,
          secondary: LuminaColors.accentLight,
          surface: LuminaColors.surface,
          onSurface: Colors.white,
          error: LuminaColors.red,
        ),
        textTheme: ThemeData.dark().textTheme.apply(
              bodyColor: LuminaColors.white87,
              displayColor: Colors.white,
              fontFamily: 'monospace',
            ),
        appBarTheme: const AppBarTheme(
          backgroundColor: Colors.transparent,
          surfaceTintColor: Colors.transparent,
          elevation: 0,
          centerTitle: true,
          titleTextStyle: TextStyle(
            fontWeight: FontWeight.w700,
            fontSize: 19,
            letterSpacing: 0.8,
            color: Colors.white,
            fontFamily: 'monospace',
          ),
        ),
        cardTheme: CardThemeData(
          margin: EdgeInsets.zero,
          elevation: 0,
          color: LuminaColors.surface.withValues(alpha: 0.80),
          shape: RoundedRectangleBorder(
            borderRadius: BorderRadius.circular(18),
            side: BorderSide(color: Colors.white.withValues(alpha: 0.10)),
          ),
        ),
        inputDecorationTheme: InputDecorationTheme(
          filled: true,
          fillColor: Colors.black.withValues(alpha: 0.34),
          border: OutlineInputBorder(
            borderRadius: BorderRadius.circular(14),
            borderSide: BorderSide(color: Colors.white.withValues(alpha: 0.14)),
          ),
          enabledBorder: OutlineInputBorder(
            borderRadius: BorderRadius.circular(14),
            borderSide: BorderSide(color: Colors.white.withValues(alpha: 0.14)),
          ),
          focusedBorder: const OutlineInputBorder(
            borderRadius: BorderRadius.all(Radius.circular(14)),
            borderSide: BorderSide(color: LuminaColors.accent, width: 1.2),
          ),
          contentPadding:
              const EdgeInsets.symmetric(horizontal: 16, vertical: 14),
        ),
        snackBarTheme: SnackBarThemeData(
          backgroundColor: LuminaColors.surfaceLight,
          shape:
              RoundedRectangleBorder(borderRadius: BorderRadius.circular(12)),
          behavior: SnackBarBehavior.floating,
        ),
        useMaterial3: true,
      ),
      home: const HomeShell(),
    );
  }
}

class HomeShell extends ConsumerStatefulWidget {
  const HomeShell({super.key});

  @override
  ConsumerState<HomeShell> createState() => _HomeShellState();
}

class _HomeShellState extends ConsumerState<HomeShell> {
  int _index = 0;

  static const _pages = [
    ChatPage(),
    ModelsPage(),
    ObservabilityPage(),
  ];

  @override
  Widget build(BuildContext context) {
    final controller = ref.watch(runtimeControllerProvider);
    final ramPct = controller.ramUsagePct;
    final isCompact = MediaQuery.sizeOf(context).width < 390;

    return LuminaBackdrop(
      child: Scaffold(
        backgroundColor: Colors.transparent,
        appBar: AppBar(
          leadingWidth: isCompact ? 50 : 54,
          leading: Padding(
            padding: const EdgeInsets.only(left: 12),
            child: Center(
              child: Container(
                width: 34,
                height: 34,
                decoration: BoxDecoration(
                  gradient: LuminaGradients.accent,
                  borderRadius: BorderRadius.circular(11),
                  boxShadow: [
                    BoxShadow(
                      color: LuminaColors.accent.withValues(alpha: 0.35),
                      blurRadius: 16,
                    ),
                  ],
                ),
                child: const Icon(Icons.auto_awesome_rounded,
                    size: 18, color: Colors.white),
              ),
            ),
          ),
          title: Text(isCompact ? 'MAI' : 'MAI RUNTIME'),
          actions: [
            if (!isCompact) _PrivacyBadge(),
            IconButton(
              visualDensity: VisualDensity.compact,
              tooltip: 'Download Models',
              onPressed: () => Navigator.of(context).push(
                MaterialPageRoute<void>(builder: (_) => const DownloadsPage()),
              ),
              icon: const Icon(Icons.download_rounded, size: 22),
            ),
            IconButton(
              visualDensity: VisualDensity.compact,
              onPressed: controller.initialized ? controller.refreshAll : null,
              icon: const Icon(Icons.refresh_rounded, size: 22),
            ),
            const SizedBox(width: 4),
          ],
        ),
        body: Column(
          children: [
            if (controller.initializing)
              const LinearProgressIndicator(minHeight: 2)
            else if (controller.connectionMode == ConnectionMode.disconnected)
              _DisconnectedBanner(onReconnect: controller.reconnect)
            else if (controller.lastError != null)
              _ErrorBanner(message: controller.lastError!),
            if (controller.initialized) _RamGauge(pct: ramPct),
            Expanded(
              child: Padding(
                padding: const EdgeInsets.fromLTRB(10, 2, 10, 0),
                child: ClipRRect(
                  borderRadius: BorderRadius.circular(22),
                  child: Container(
                    decoration: BoxDecoration(
                      gradient: LuminaGradients.card,
                      border: Border.all(
                          color: Colors.white.withValues(alpha: 0.08)),
                    ),
                    child: _pages[_index],
                  ),
                ),
              ),
            ),
          ],
        ),
        floatingActionButton: _index == 1
            ? FloatingActionButton(
                onPressed: () => Navigator.of(context).push(
                  MaterialPageRoute<void>(
                      builder: (_) => const DownloadsPage()),
                ),
                backgroundColor: LuminaColors.accent,
                foregroundColor: Colors.black,
                child: const Icon(Icons.add),
              )
            : null,
        bottomNavigationBar: SafeArea(
          top: false,
          minimum: const EdgeInsets.fromLTRB(10, 8, 10, 8),
          child: ClipRRect(
            borderRadius: BorderRadius.circular(18),
            child: NavigationBarTheme(
              data: NavigationBarThemeData(
                backgroundColor: Colors.black.withValues(alpha: 0.62),
                indicatorColor: LuminaColors.accent.withValues(alpha: 0.24),
                labelTextStyle: WidgetStateProperty.resolveWith((states) {
                  final selected = states.contains(WidgetState.selected);
                  return TextStyle(
                    fontSize: 11,
                    fontWeight: selected ? FontWeight.w700 : FontWeight.w500,
                    color:
                        selected ? LuminaColors.accent : LuminaColors.white60,
                  );
                }),
              ),
              child: NavigationBar(
                selectedIndex: _index,
                onDestinationSelected: (i) => setState(() => _index = i),
                destinations: const [
                  NavigationDestination(
                    icon: Icon(Icons.home_outlined),
                    selectedIcon: Icon(Icons.home_rounded),
                    label: 'Home',
                  ),
                  NavigationDestination(
                    icon: Icon(Icons.hub_outlined),
                    selectedIcon: Icon(Icons.hub_rounded),
                    label: 'Models',
                  ),
                  NavigationDestination(
                    icon: Icon(Icons.monitor_heart_outlined),
                    selectedIcon: Icon(Icons.monitor_heart_rounded),
                    label: 'Stats',
                  ),
                ],
              ),
            ),
          ),
        ),
      ),
    );
  }
}

class _PrivacyBadge extends ConsumerWidget {
  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final mode = ref.watch(runtimeControllerProvider).connectionMode;
    final (Color color, IconData icon, String label) = switch (mode) {
      ConnectionMode.ffi => (
          LuminaColors.emerald,
          Icons.shield_rounded,
          'On-device'
        ),
      ConnectionMode.http => (
          LuminaColors.accentLight,
          Icons.cloud_rounded,
          'HTTP'
        ),
      ConnectionMode.disconnected => (
          LuminaColors.amber,
          Icons.cloud_off_rounded,
          'Offline'
        ),
    };

    return Container(
      margin: const EdgeInsets.symmetric(vertical: 11),
      padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 5),
      decoration: BoxDecoration(
        color: Colors.black.withValues(alpha: 0.32),
        borderRadius: BorderRadius.circular(999),
        border: Border.all(color: color.withValues(alpha: 0.5)),
      ),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          Icon(icon, size: 14, color: color),
          const SizedBox(width: 4),
          Text(
            label,
            style: TextStyle(
                fontSize: 11, fontWeight: FontWeight.w600, color: color),
          ),
        ],
      ),
    );
  }
}

class _DisconnectedBanner extends StatelessWidget {
  const _DisconnectedBanner({required this.onReconnect});
  final Future<void> Function() onReconnect;

  @override
  Widget build(BuildContext context) {
    return Container(
      width: double.infinity,
      padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 10),
      decoration: BoxDecoration(
        color: Colors.black.withValues(alpha: 0.28),
        border: Border(
            bottom:
                BorderSide(color: LuminaColors.amber.withValues(alpha: 0.35))),
      ),
      child: Row(
        children: [
          const Icon(Icons.cloud_off_rounded,
              size: 16, color: LuminaColors.amber),
          const SizedBox(width: 8),
          const Expanded(
            child: Text(
              'Runtime disconnected. Run `mai serve` and retry.',
              style: TextStyle(fontSize: 13, color: LuminaColors.amber),
            ),
          ),
          const SizedBox(width: 8),
          SizedBox(
            height: 30,
            child: FilledButton(
              onPressed: onReconnect,
              style: FilledButton.styleFrom(
                backgroundColor: LuminaColors.amber,
                foregroundColor: Colors.black,
                padding: const EdgeInsets.symmetric(horizontal: 12),
                textStyle:
                    const TextStyle(fontSize: 12, fontWeight: FontWeight.w700),
              ),
              child: const Text('Retry'),
            ),
          ),
        ],
      ),
    );
  }
}

class _ErrorBanner extends StatelessWidget {
  const _ErrorBanner({required this.message});
  final String message;

  @override
  Widget build(BuildContext context) {
    return Container(
      width: double.infinity,
      padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 10),
      decoration: BoxDecoration(
        color: Colors.black.withValues(alpha: 0.28),
        border: Border(
            bottom:
                BorderSide(color: LuminaColors.red.withValues(alpha: 0.36))),
      ),
      child: Row(
        children: [
          const Icon(Icons.error_outline_rounded,
              size: 16, color: LuminaColors.red),
          const SizedBox(width: 8),
          Expanded(
            child: Text(
              message,
              style: const TextStyle(fontSize: 13, color: LuminaColors.red),
              maxLines: 4,
              overflow: TextOverflow.ellipsis,
            ),
          ),
        ],
      ),
    );
  }
}

class _RamGauge extends StatelessWidget {
  const _RamGauge({required this.pct});
  final double pct;

  @override
  Widget build(BuildContext context) {
    final color = ramColor(pct);
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 7),
      decoration: BoxDecoration(
        color: Colors.black.withValues(alpha: 0.24),
        border: Border(
            bottom: BorderSide(color: Colors.white.withValues(alpha: 0.06))),
      ),
      child: Row(
        children: [
          Icon(Icons.memory_rounded, size: 14, color: color),
          const SizedBox(width: 8),
          const Text('RAM',
              style: TextStyle(fontSize: 11, color: LuminaColors.white60)),
          const SizedBox(width: 10),
          Expanded(
            child: ClipRRect(
              borderRadius: BorderRadius.circular(999),
              child: LinearProgressIndicator(
                value: pct,
                minHeight: 4,
                backgroundColor: Colors.white.withValues(alpha: 0.08),
                valueColor: AlwaysStoppedAnimation<Color>(color),
              ),
            ),
          ),
          const SizedBox(width: 8),
          Text(
            '${(pct * 100).toStringAsFixed(0)}%',
            style: TextStyle(
                fontSize: 11, fontWeight: FontWeight.w700, color: color),
          ),
        ],
      ),
    );
  }
}
