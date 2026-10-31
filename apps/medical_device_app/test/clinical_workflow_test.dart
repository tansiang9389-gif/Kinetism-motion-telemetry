import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:medical_device_app/app_design.dart';
import 'package:medical_device_app/clinical_models.dart';
import 'package:medical_device_app/clinical_workflow.dart';

Future<void> pumpWorkflow(
  WidgetTester tester,
  KinetismClinicalController controller, {
  Size size = const Size(390, 844),
  double scale = 1.0,
}) async {
  tester.view.physicalSize = size;
  tester.view.devicePixelRatio = 1.0;
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
      home: KinetismClinicalFlowScreen(controller: controller),
    ),
  );
  await tester.pumpAndSettle();
}

void main() {
  test(
    'Anthropometric force calculation responds to bicep, forearm, arm length and angle',
    () {
      const smallerArm = ArmAnthropometrics(
        bicepCircumferenceCm: 24.0,
        forearmCircumferenceCm: 20.0,
        armLengthCm: 48.0,
      );
      const largerArm = ArmAnthropometrics(
        bicepCircumferenceCm: 34.0,
        forearmCircumferenceCm: 28.0,
        armLengthCm: 60.0,
      );

      final smallForce = smallerArm.calculateForceNewtons(angleDegrees: 110);
      final largeForce = largerArm.calculateForceNewtons(angleDegrees: 110);
      expect(largeForce, greaterThan(smallForce));

      final lowAngleForce = largerArm.calculateForceNewtons(angleDegrees: 25);
      final highAngleForce = largerArm.calculateForceNewtons(angleDegrees: 130);
      expect(highAngleForce, greaterThan(lowAngleForce));
    },
  );

  test('AI MMT grading evaluates 0 to 5 grades from angle and force', () {
    const arm = ArmAnthropometrics(
      bicepCircumferenceCm: 29.0,
      forearmCircumferenceCm: 24.5,
      armLengthCm: 55.0,
    );

    final zeroGrade = MmtAssessment.evaluate(
      angleDegrees: 4.0,
      forceNewtons: 0.5,
      anthropometrics: arm,
    );
    expect(zeroGrade.grade, 0);

    final poorGrade = MmtAssessment.evaluate(
      angleDegrees: 50.0,
      forceNewtons: arm.calculateForceNewtons(angleDegrees: 50.0),
      anthropometrics: arm,
    );
    expect(poorGrade.grade, 2);

    final goodGrade = MmtAssessment.evaluate(
      angleDegrees: 120.0,
      forceNewtons: arm.calculateForceNewtons(angleDegrees: 120.0),
      anthropometrics: arm,
    );
    expect(goodGrade.grade, greaterThanOrEqualTo(4));
  });

  testWidgets(
    'Complete 9-step physiotherapist workflow: patient -> measure -> arm -> anthropometrics -> record -> record again -> logs',
    (tester) async {
      final controller = KinetismClinicalController();
      await pumpWorkflow(tester, controller);

      // Step 1: Select patient (past patients, new patient, overall record log)
      expect(find.text('Select patient'), findsOneWidget);
      expect(find.text('New patient'), findsOneWidget);
      expect(find.textContaining('Overall record log'), findsOneWidget);
      expect(find.text('Ahmad Razak'), findsOneWidget);

      // Create a new patient and select them
      await tester.tap(find.text('New patient'));
      await tester.pumpAndSettle();
      await tester.enterText(
        find.byKey(const ValueKey('Patient name')),
        'Tan Mei Ling',
      );
      await tester.enterText(
        find.byKey(const ValueKey('Patient ID (optional)')),
        'PT-205',
      );
      await tester.tap(find.text('Create & select'));
      await tester.pumpAndSettle();

      // Step 2: Select 1 of 2 sections - Measure or Record log
      expect(find.text('Tan Mei Ling'), findsWidgets);
      expect(find.text('Measure'), findsOneWidget);
      expect(find.text('Record log'), findsOneWidget);
      await tester.ensureVisible(find.text('Open Measure'));
      await tester.tap(find.text('Open Measure'));
      await tester.pumpAndSettle();

      // Step 3: Select body part (Arm first)
      expect(find.text('Which body part are you measuring?'), findsOneWidget);
      expect(find.text('Arm'), findsOneWidget);
      await tester.ensureVisible(find.text('Arm'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Arm'));
      await tester.pumpAndSettle();

      // Step 4: Key in bicep circumference, forearm circumference, and length of arm with picture guide
      expect(find.text('Where to measure on the arm'), findsOneWidget);
      expect(find.byType(ArmMeasurementGuideCard), findsOneWidget);

      final bicepField = find.byKey(
        const ValueKey('Circumference of bicep (cm)'),
      );
      await tester.ensureVisible(bicepField);
      await tester.enterText(bicepField, '31.0');

      final forearmField = find.byKey(
        const ValueKey('Circumference of forearm (cm)'),
      );
      await tester.ensureVisible(forearmField);
      await tester.enterText(forearmField, '25.5');

      final lengthField = find.byKey(const ValueKey('Length of arm (cm)'));
      await tester.ensureVisible(lengthField);
      await tester.enterText(lengthField, '57.0');
      await tester.pump();

      // Step 5: Press Start measuring button
      final startBtn = find.text('Start measuring');
      await tester.ensureVisible(startBtn);
      await tester.tap(startBtn);
      await tester.pumpAndSettle();

      // Step 6: Physiotherapist asks patient to do movement and presses Record
      expect(find.text('Arm measurement'), findsOneWidget);
      expect(find.text('Angle'), findsOneWidget);
      expect(find.text('Force'), findsOneWidget);
      expect(find.text('AI MMT grading'), findsOneWidget);

      // Select 120° movement preset and press Record
      final presetChip = find.text('Target flexion (120°)');
      await tester.ensureVisible(presetChip);
      await tester.tap(presetChip);
      await tester.pumpAndSettle();

      final recordBtn = find.text('Record');
      await tester.ensureVisible(recordBtn);
      await tester.tap(recordBtn);
      await tester.pumpAndSettle();

      expect(
        find.text('Recorded attempt #1 saved to record log'),
        findsOneWidget,
      );
      expect(find.text('Record again'), findsWidgets);

      // Step 7: Press Record again and capture a second movement position (140°)
      await tester.ensureVisible(find.text('Record again').first);
      await tester.tap(find.text('Record again').first);
      await tester.pumpAndSettle();

      final fullChip = find.text('Full flexion (140°)');
      await tester.ensureVisible(fullChip);
      await tester.tap(fullChip);
      await tester.pumpAndSettle();

      await tester.ensureVisible(find.text('Record'));
      await tester.tap(find.text('Record'));
      await tester.pumpAndSettle();

      expect(
        find.text('Recorded attempt #2 saved to record log'),
        findsOneWidget,
      );

      // Step 8 & 9: Refer to patient record log and overall record log
      final openPatientLog = find.text('Open patient record log');
      await tester.ensureVisible(openPatientLog);
      await tester.tap(openPatientLog);
      await tester.pumpAndSettle();

      expect(find.text('Tan Mei Ling record log'), findsOneWidget);
      expect(
        find.text('Tan Mei Ling · Arm (Attempt #2)'),
        findsOneWidget,
      );
      expect(
        find.text('Tan Mei Ling · Arm (Attempt #1)'),
        findsOneWidget,
      );

      final switchOverall = find.text('Switch to overall record log');
      await tester.ensureVisible(switchOverall);
      await tester.tap(switchOverall);
      await tester.pumpAndSettle();

      expect(find.text('Overall record log'), findsWidgets);
      expect(
        find.text('Tan Mei Ling · Arm (Attempt #2)'),
        findsOneWidget,
      );
      expect(tester.takeException(), isNull);
    },
  );

  for (final step in ClinicalStep.values) {
    testWidgets(
      'Clinical step ${step.name} fits narrow 320px screen at 2.0x text scale',
      (tester) async {
        final controller = KinetismClinicalController();
        controller.selectPatient(controller.patients.first);
        controller.goToStep(step);
        await pumpWorkflow(
          tester,
          controller,
          size: const Size(320, 640),
          scale: 2.0,
        );
        expect(tester.takeException(), isNull);
      },
    );
  }
}
