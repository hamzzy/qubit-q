import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'runtime_controller.dart';

final runtimeControllerProvider = ChangeNotifierProvider<RuntimeController>((ref) {
  final controller = RuntimeController();
  ref.onDispose(controller.dispose);
  return controller;
});
