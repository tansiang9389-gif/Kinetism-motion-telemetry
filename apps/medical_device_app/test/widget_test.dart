import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:medical_device_app/app_design.dart';
import 'package:medical_device_app/main.dart';

void main() {
  testWidgets('IdpApp hosts an injected home in the Kinetism dark theme', (
    WidgetTester tester,
  ) async {
    await tester.pumpWidget(
      const IdpApp(home: Scaffold(body: Text('Injected home'))),
    );

    expect(find.text('Injected home'), findsOneWidget);
    final app = tester.widget<MaterialApp>(find.byType(MaterialApp));
    expect(app.title, 'Kinetism');
    expect(app.debugShowCheckedModeBanner, isFalse);
    expect(app.theme?.brightness, Brightness.dark);
    expect(app.theme?.scaffoldBackgroundColor, KinetismTheme.background);
    expect(find.byType(AppStartup), findsNothing);
    expect(tester.takeException(), isNull);
  });

  testWidgets('IdpApp opens on the startup screen by default', (
    WidgetTester tester,
  ) async {
    await tester.pumpWidget(const IdpApp());

    expect(find.byType(AppStartup), findsOneWidget);
    expect(find.text('Starting Kinetism'), findsOneWidget);
    expect(tester.takeException(), isNull);
  });
}
