import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:mai_flutter/main.dart';

void main() {
  testWidgets('App shell renders', (WidgetTester tester) async {
    await tester.pumpWidget(
      const ProviderScope(
        child: MaiFlutterApp(),
      ),
    );

    expect(find.text('MAI RUNTIME'), findsOneWidget);
    expect(find.text('Home'), findsOneWidget);
    expect(find.text('Models'), findsOneWidget);
    expect(find.text('Stats'), findsOneWidget);

    // Allow async init to settle before test teardown disposes the controller.
    await tester.pumpAndSettle(const Duration(milliseconds: 100));
  });
}
