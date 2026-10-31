import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:medical_device_app/add_exercise.dart';
import 'package:medical_device_app/app_design.dart';
import 'package:medical_device_app/prescriptions.dart';

const goalError = 'Enter a whole-number goal greater than zero.';
const videoError = 'Enter a full HTTP or HTTPS video URL without credentials.';
const peakError = 'Peak must be below resting angle for this tracker.';

Future<void> pumpNarrow(
  WidgetTester tester,
  Widget screen,
  double scale,
) async {
  tester.view.physicalSize = const Size(320, 820);
  tester.view.devicePixelRatio = 1;
  addTearDown(tester.view.resetPhysicalSize);
  addTearDown(tester.view.resetDevicePixelRatio);
  await tester.pumpWidget(
    MaterialApp(
      theme: KinetismTheme.dark,
      builder: (context, child) => MediaQuery(
        data: MediaQuery.of(
          context,
        ).copyWith(textScaler: TextScaler.linear(scale)),
        child: child!,
      ),
      home: screen,
    ),
  );
  await tester.pump(const Duration(milliseconds: 500));
}

void expectUnclipped(WidgetTester tester, String message) {
  final finder = find.text(message);
  expect(finder, findsOneWidget);
  final paragraph = tester.renderObject<RenderParagraph>(finder);
  expect(
    paragraph.didExceedMaxLines,
    isFalse,
    reason:
        '"$message" must remain readable '
        '(${paragraph.maxLines} lines, ${paragraph.size.width}px wide).',
  );
}

Future<void> enterField(WidgetTester tester, String label, String value) async {
  final field = find.byKey(ValueKey(label));
  await tester.ensureVisible(field);
  await tester.enterText(field, value);
  await tester.pump();
}

void main() {
  for (final scale in [1.0, 2.0]) {
    testWidgets('Goal validation is not clipped at ${scale}x text', (
      tester,
    ) async {
      var saves = 0;
      await pumpNarrow(
        tester,
        PrescriptionsScreen(
          loadGoals: () => Stream.value(const [
            ExerciseGoal(
              id: 'synthetic-goal',
              name: 'Elbow flexion',
              target: 10,
            ),
          ]),
          loadReminderStatus: () async => false,
          changeReminder: (_) async {},
          saveGoal: (_, _) async {
            saves++;
          },
        ),
        scale,
      );
      final edit = find.byIcon(Icons.edit_outlined);
      await tester.scrollUntilVisible(
        edit,
        160,
        scrollable: find.byType(Scrollable).first,
      );
      await tester.pump(const Duration(milliseconds: 300));
      await tester.tap(edit);
      await tester.pump(const Duration(milliseconds: 500));
      await tester.enterText(find.byType(TextFormField), '0');
      await tester.tap(find.text('Save goal'));
      await tester.pump(const Duration(milliseconds: 300));
      await tester.ensureVisible(find.text(goalError));
      await tester.pump();

      expectUnclipped(tester, goalError);
      expect(saves, 0);
      expect(tester.takeException(), isNull);
      await tester.tap(find.text('Cancel'));
      await tester.pump(const Duration(milliseconds: 500));
      expect(tester.takeException(), isNull);
    });

    testWidgets('Recipe validation is not clipped at ${scale}x text', (
      tester,
    ) async {
      var saves = 0;
      await pumpNarrow(
        tester,
        AddExerciseScreen(
          saveExercise: (_) async {
            saves++;
          },
        ),
        scale,
      );
      await enterField(tester, 'Resting angle (degrees)', '70');
      await enterField(tester, 'Peak angle (degrees)', '160');
      await enterField(tester, 'Daily repetition goal', '0');
      await enterField(tester, 'Demonstration video URL', 'not-a-url');
      final save = find.text('Save exercise');
      await tester.ensureVisible(save);
      await tester.tap(save);
      await tester.pump(const Duration(milliseconds: 300));

      for (final message in [goalError, videoError, peakError]) {
        await tester.ensureVisible(find.text(message));
        await tester.pump();
        expectUnclipped(tester, message);
      }
      expect(saves, 0);
      expect(tester.takeException(), isNull);
    });
  }
}
