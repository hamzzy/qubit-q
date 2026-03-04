import 'package:flutter/material.dart';

/// Neon dark tokens inspired by modern AI assistant UIs.
abstract final class LuminaColors {
  static const bg = Color(0xFF070A14);
  static const surface = Color(0xFF101624);
  static const surfaceLight = Color(0xFF172238);

  // Keep existing semantic names used across the app.
  static const accent = Color(0xFF34F58A);
  static const accentLight = Color(0xFF4E72FF);
  static const emerald = Color(0xFF2EF8A5);
  static const amber = Color(0xFFFFC857);
  static const red = Color(0xFFFF5A6E);

  static const white87 = Color(0xDEFFFFFF);
  static const white60 = Color(0x99FFFFFF);
  static const white12 = Color(0x1FFFFFFF);
  static const white06 = Color(0x0FFFFFFF);
}

abstract final class LuminaGradients {
  static const shell = LinearGradient(
    begin: Alignment.topLeft,
    end: Alignment.bottomRight,
    colors: [
      Color(0xFF3E4DFF),
      Color(0xFF1F7AFF),
      Color(0xFF22C793),
    ],
  );

  static const accent = LinearGradient(
    begin: Alignment.topLeft,
    end: Alignment.bottomRight,
    colors: [LuminaColors.accentLight, LuminaColors.accent],
  );

  static const card = LinearGradient(
    begin: Alignment.topLeft,
    end: Alignment.bottomRight,
    colors: [
      Color(0xFF11192B),
      Color(0xFF0E1422),
    ],
  );
}

Color ramColor(double pct) {
  if (pct >= 0.85) return LuminaColors.red;
  if (pct >= 0.70) return LuminaColors.amber;
  return LuminaColors.emerald;
}

BoxDecoration glassDecoration({
  double borderOpacity = 0.16,
  double fillOpacity = 0.50,
  double radius = 16,
}) {
  return BoxDecoration(
    gradient: LinearGradient(
      begin: Alignment.topLeft,
      end: Alignment.bottomRight,
      colors: [
        Colors.white.withValues(alpha: fillOpacity * 0.20),
        Colors.white.withValues(alpha: fillOpacity * 0.06),
      ],
    ),
    borderRadius: BorderRadius.circular(radius),
    border: Border.all(color: Colors.white.withValues(alpha: borderOpacity)),
  );
}

class LuminaBackdrop extends StatelessWidget {
  const LuminaBackdrop({super.key, required this.child});

  final Widget child;

  @override
  Widget build(BuildContext context) {
    return Stack(
      fit: StackFit.expand,
      children: [
        Container(
          decoration: const BoxDecoration(gradient: LuminaGradients.shell),
        ),
        Positioned(
          top: -120,
          left: -70,
          child: _GlowOrb(
            size: 260,
            color: LuminaColors.accent.withValues(alpha: 0.24),
          ),
        ),
        Positioned(
          top: 140,
          right: -80,
          child: _GlowOrb(
            size: 280,
            color: LuminaColors.accentLight.withValues(alpha: 0.20),
          ),
        ),
        Positioned(
          bottom: -120,
          left: 40,
          child: _GlowOrb(
            size: 320,
            color: const Color(0xFF0A0E19).withValues(alpha: 0.92),
          ),
        ),
        child,
      ],
    );
  }
}

class _GlowOrb extends StatelessWidget {
  const _GlowOrb({required this.size, required this.color});

  final double size;
  final Color color;

  @override
  Widget build(BuildContext context) {
    return IgnorePointer(
      child: Container(
        width: size,
        height: size,
        decoration: BoxDecoration(
          shape: BoxShape.circle,
          color: color,
          boxShadow: [
            BoxShadow(
              color: color,
              blurRadius: size * 0.4,
              spreadRadius: size * 0.08,
            ),
          ],
        ),
      ),
    );
  }
}
