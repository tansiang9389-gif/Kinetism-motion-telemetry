import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:medical_device_app/app_design.dart';
import 'package:medical_device_app/dashboard.dart';
import 'package:medical_device_app/session_log.dart';

void main() {
  testWidgets(
    'Data-quality notice omits zero issues without hiding missing dates',
    (tester) async {
      await tester.pumpWidget(
        MaterialApp(
          theme: KinetismTheme.dark,
          home: Scaffold(
            body: SessionLogView(
              logs: [
                SessionLog.fromMap('missing-date', {
                  'exerciseName': 'Synthetic curl',
                  'repsCompleted': 2,
                }),
              ],
              onStartExercise: () {},
              onRetry: () {},
              preview: true,
            ),
          ),
        ),
      );
      expect(
        find.textContaining('1 session date is unavailable.'),
        findsOneWidget,
      );
      expect(find.textContaining('0 session'), findsNothing);
      expect(
        find.textContaining('Missing values are not counted as zero or today.'),
        findsOneWidget,
      );
      expect(
        find.text('Synthetic records, not patient measurements.'),
        findsOneWidget,
      );
      expect(tester.takeException(), isNull);
    },
  );
}
