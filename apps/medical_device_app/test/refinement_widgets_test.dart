import 'dart:async';

import 'package:firebase_core/firebase_core.dart';
import 'package:fl_chart/fl_chart.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:medical_device_app/add_exercise.dart';
import 'package:medical_device_app/app_design.dart';
import 'package:medical_device_app/arm_tracker_view.dart';
import 'package:medical_device_app/dashboard.dart';
import 'package:medical_device_app/main.dart' show AppStartup;
import 'package:medical_device_app/past_reports.dart';
import 'package:medical_device_app/prescriptions.dart';
import 'package:medical_device_app/preview.dart';
import 'package:medical_device_app/select_exercise.dart';
import 'package:medical_device_app/session_log.dart';
import 'package:vector_math/vector_math_64.dart' as vector;

final now = DateTime(2026, 10, 31, 12);
final records = [
  SessionLog.fromMap('elbow', {
    'exerciseName': 'Elbow flexion',
    'repsCompleted': 12,
    'date': now,
  }),
  SessionLog.fromMap('wrist', {
    'exerciseName': 'Wrist extension',
    'repsCompleted': 0,
    'date': now,
  }),
];

Future<void> pumpScreen(
  WidgetTester tester,
  Widget home, {
  Size size = const Size(375, 812),
  double scale = 1,
}) async {
  tester.view.physicalSize = size;
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
      home: home,
    ),
  );
  await tester.pump(const Duration(milliseconds: 500));
}

Widget logScreen({
  List<SessionLog>? logs,
  bool loading = false,
  String? error,
  bool cache = false,
  bool pending = false,
  VoidCallback? retry,
  ValueChanged<List<String>>? onReport,
}) {
  return Scaffold(
    body: SessionLogView(
      logs: logs ?? records,
      loading: loading,
      errorMessage: error,
      fromCache: cache,
      pendingWrites: pending,
      now: now,
      onStartExercise: () {},
      onRetry: retry ?? () {},
      onReport: onReport,
    ),
  );
}

Future<void> enterField(WidgetTester tester, String label, String value) async {
  final finder = find.byKey(ValueKey(label));
  await tester.ensureVisible(finder);
  await tester.enterText(finder, value);
}

Future<void> enterValidRecipe(WidgetTester tester) async {
  await enterField(tester, 'Exercise name', '  Elbow flexion ');
  await enterField(tester, 'Resting angle (degrees)', '160');
  await enterField(tester, 'Peak angle (degrees)', '60');
  await enterField(
    tester,
    'Demonstration video URL',
    'https://example.test/demo.mp4',
  );
  await tester.ensureVisible(find.text('Save exercise'));
}

Future<void> openEditor(WidgetTester tester, Widget editor) async {
  await pumpScreen(
    tester,
    Scaffold(
      body: Builder(
        builder: (context) => TextButton(
          onPressed: () => Navigator.push(
            context,
            MaterialPageRoute<void>(
              builder: (context) => Scaffold(
                body: Column(
                  children: [
                    const Text('Previous page'),
                    TextButton(
                      onPressed: () => Navigator.push(
                        context,
                        MaterialPageRoute<void>(builder: (_) => editor),
                      ),
                      child: const Text('Open editor'),
                    ),
                  ],
                ),
              ),
            ),
          ),
          child: const Text('Open previous page'),
        ),
      ),
    ),
  );
  await tester.tap(find.text('Open previous page'));
  await tester.pumpAndSettle();
  await tester.tap(find.text('Open editor'));
  await tester.pumpAndSettle();
}

void main() {
  testWidgets('Startup failure has a retry path without creating Firebase', (
    tester,
  ) async {
    var calls = 0;
    await pumpScreen(
      tester,
      AppStartup(
        initialize: () async {
          calls++;
          if (calls == 1) throw StateError('Synthetic startup failure');
        },
        child: const Scaffold(body: Text('Ready without services')),
      ),
    );
    expect(find.text('Could not start Kinetism'), findsOneWidget);
    await tester.tap(find.text('Retry startup'));
    await tester.pumpAndSettle();
    expect(calls, 2);
    expect(find.text('Ready without services'), findsOneWidget);
  });

  testWidgets('Loading and failure are not reported as an empty session log', (
    tester,
  ) async {
    await pumpScreen(tester, logScreen(logs: [], loading: true));
    expect(find.text('Loading sessions'), findsOneWidget);
    expect(find.text('No sessions yet'), findsNothing);
    expect(find.text('Start exercise'), findsOneWidget);
    await pumpScreen(tester, logScreen(logs: [], error: 'Synthetic failure'));
    expect(find.text('Session log unavailable'), findsOneWidget);
    expect(find.text('No sessions yet'), findsNothing);
    expect(find.text('Start exercise'), findsOneWidget);
  });

  testWidgets('Search and exercise filters can both be cleared', (
    tester,
  ) async {
    await pumpScreen(tester, logScreen());
    tester
        .widget<DropdownButtonFormField<String>>(
          find.byType(DropdownButtonFormField<String>),
        )
        .onChanged!('Elbow flexion');
    await tester.pump();
    await tester.enterText(find.byType(TextField), 'does not exist');
    await tester.pump();
    expect(find.text('No matching sessions'), findsOneWidget);
    await tester.ensureVisible(find.text('Clear filters').last);
    await tester.tap(find.text('Clear filters').last);
    await tester.pumpAndSettle();
    expect(find.text('No matching sessions'), findsNothing);
    final dropdown = tester.widget<DropdownButtonFormField<String>>(
      find.byType(DropdownButtonFormField<String>),
    );
    expect(dropdown.initialValue, isNull);
    expect(
      tester.widget<TextField>(find.byType(TextField)).controller!.text,
      isEmpty,
    );
  });

  testWidgets('Clearing a matching search preserves the selected exercise', (
    tester,
  ) async {
    await pumpScreen(tester, logScreen());
    tester
        .widget<DropdownButtonFormField<String>>(
          find.byType(DropdownButtonFormField<String>),
        )
        .onChanged!('Elbow flexion');
    await tester.pump();
    await tester.enterText(find.byType(TextField), 'elbow');
    await tester.pump();
    expect(find.text('No matching sessions'), findsNothing);
    expect(find.byTooltip('Clear search'), findsOneWidget);
    await tester.tap(find.byTooltip('Clear search'));
    await tester.pumpAndSettle();
    expect(
      tester.widget<TextField>(find.byType(TextField)).controller!.text,
      isEmpty,
    );
    expect(
      tester
          .widget<DropdownButtonFormField<String>>(
            find.byType(DropdownButtonFormField<String>),
          )
          .initialValue,
      'Elbow flexion',
    );
    expect(find.byTooltip('Clear search'), findsNothing);
    expect(tester.takeException(), isNull);
  });

  testWidgets('Removing a selected exercise resets the visible filter', (
    tester,
  ) async {
    await pumpScreen(tester, logScreen());
    tester
        .widget<DropdownButtonFormField<String>>(
          find.byType(DropdownButtonFormField<String>),
        )
        .onChanged!('Elbow flexion');
    await tester.pump();
    await pumpScreen(tester, logScreen(logs: [records.last]));
    expect(
      tester
          .widget<DropdownButtonFormField<String>>(
            find.byType(DropdownButtonFormField<String>),
          )
          .initialValue,
      isNull,
    );
    expect(find.text('No matching sessions'), findsNothing);
    expect(tester.takeException(), isNull);
  });

  testWidgets('Chart is bounded to seven actual records and accepts zero', (
    tester,
  ) async {
    final zeros = List.generate(
      10,
      (index) => SessionLog.fromMap('zero-$index', {
        'exerciseName': 'Zero-count exercise',
        'repsCompleted': 0,
        'date': now,
      }),
    );
    await pumpScreen(tester, logScreen(logs: zeros));
    expect(
      tester.widget<BarChart>(find.byType(BarChart)).data.barGroups,
      hasLength(7),
    );
    expect(tester.takeException(), isNull);
  });

  testWidgets(
    'Failure can retain records; cache and pending states are explicit',
    (tester) async {
      await pumpScreen(
        tester,
        logScreen(error: 'Synthetic failure', cache: true, pending: true),
      );
      expect(find.text('Synthetic failure'), findsOneWidget);
      final messages = tester
          .widgetList<NoticeBanner>(find.byType(NoticeBanner))
          .map((banner) => banner.message)
          .join('\n');
      expect(messages.toLowerCase(), contains('cache'));
      expect(messages.toLowerCase(), contains('confirmation'));
      expect(find.byType(BarChart), findsOneWidget);
    },
  );

  for (final size in [
    const Size(320, 640),
    const Size(375, 812),
    const Size(800, 1024),
    const Size(1440, 900),
  ]) {
    for (final scale in [1.0, 2.0]) {
      testWidgets('Session log fits ${size.width} px at text scale $scale', (
        tester,
      ) async {
        await pumpScreen(
          tester,
          logScreen(
            logs: [
              SessionLog.fromMap('long', {
                'exerciseName':
                    'Very long exercise name for a supervised rehabilitation movement',
                'patientId': 'synthetic-long-identifier-0001',
                'repsCompleted': 999999999,
                'date': now,
              }),
              SessionLog.fromMap('missing', {
                'exerciseName': 'Missing record fields',
              }),
            ],
          ),
          size: size,
          scale: scale,
        );
        await tester.drag(
          find.byType(CustomScrollView),
          const Offset(0, -1200),
        );
        await tester.pump();
        expect(tester.takeException(), isNull);
      });
    }
  }

  testWidgets('Exercise stream error is recoverable, not empty', (
    tester,
  ) async {
    var calls = 0;
    await pumpScreen(
      tester,
      SelectExerciseScreen(
        loadExercises: () {
          calls++;
          return calls == 1
              ? Stream.error(StateError('Synthetic failure'))
              : Stream.value([
                  const ExerciseSummary(
                    id: 'demo',
                    name: 'Elbow flexion',
                    joint: 'Elbow',
                  ),
                ]);
        },
        onSelect: (_) {},
        onAddExercise: () {},
      ),
    );
    expect(find.text('Exercises unavailable'), findsOneWidget);
    expect(find.text('No exercises yet'), findsNothing);
    await tester.tap(find.text('Retry loading'));
    await tester.pumpAndSettle();
    expect(calls, 2);
    expect(find.text('Elbow flexion'), findsOneWidget);
  });

  testWidgets('Archived report with missing date cannot open', (tester) async {
    var opened = false;
    await pumpScreen(
      tester,
      PastReportsScreen(
        loadReports: () => Stream.value([
          const ArchivedReport(analysis: 'Synthetic report with no date'),
        ]),
        onOpen: (_) => opened = true,
      ),
    );
    expect(find.text('Report date unavailable'), findsOneWidget);
    await tester.tap(find.text('Weekly report draft'));
    expect(opened, isFalse);
  });

  testWidgets('Invalid recipe does not invoke save', (tester) async {
    var saves = 0;
    await pumpScreen(
      tester,
      AddExerciseScreen(saveExercise: (_) async => saves++),
    );
    await enterField(tester, 'Exercise name', '  ');
    await enterField(tester, 'Resting angle (degrees)', 'NaN');
    await enterField(tester, 'Peak angle (degrees)', '190');
    await enterField(tester, 'Daily repetition goal', '-2');
    await enterField(tester, 'Demonstration video URL', 'not a URL');
    await tester.ensureVisible(find.text('Save exercise'));
    await tester.tap(find.text('Save exercise'));
    await tester.pump();
    expect(saves, 0);
    expect(
      find.text('Enter a whole-number goal greater than zero.'),
      findsOneWidget,
    );
    expect(tester.takeException(), isNull);
  });

  testWidgets(
    'Pending recipe save cannot be double-submitted; failure is visible',
    (tester) async {
      var saves = 0;
      Map<String, Object>? submitted;
      final completer = Completer<void>();
      await pumpScreen(
        tester,
        AddExerciseScreen(
          saveExercise: (recipe) {
            saves++;
            submitted = recipe;
            return completer.future;
          },
        ),
      );
      await enterField(tester, 'Exercise name', '  Elbow flexion ');
      await enterField(tester, 'Resting angle (degrees)', '160');
      await enterField(tester, 'Peak angle (degrees)', '60');
      await enterField(
        tester,
        'Demonstration video URL',
        'https://example.test/demo.mp4',
      );
      await tester.ensureVisible(find.text('Save exercise'));
      await tester.tap(find.text('Save exercise'));
      await tester.pump();
      expect(find.text('Waiting for cloud save'), findsOneWidget);
      expect(
        tester.widget<FilledButton>(find.byType(FilledButton)).onPressed,
        isNull,
      );
      expect(saves, 1);
      expect(submitted, {
        'name': 'Elbow flexion',
        'targetJoint': 'Elbow',
        'restingAngle': 160.0,
        'peakAngle': 60.0,
        'dailyGoal': 10,
        'videoUrl': 'https://example.test/demo.mp4',
      });
      completer.completeError(
        FirebaseException(plugin: 'test', code: 'permission-denied'),
      );
      await tester.pumpAndSettle();
      expect(find.textContaining('permission-denied'), findsOneWidget);
      expect(find.text('Save exercise'), findsOneWidget);
    },
  );

  testWidgets('Recipe completion after leaving does not pop another page', (
    tester,
  ) async {
    final completer = Completer<void>();
    await openEditor(
      tester,
      AddExerciseScreen(saveExercise: (_) => completer.future),
    );
    await enterValidRecipe(tester);
    await tester.tap(find.text('Save exercise'));
    await tester.pump();
    await tester.tap(find.byTooltip('Back'));
    await tester.pump(const Duration(milliseconds: 20));
    completer.complete();
    await tester.pumpAndSettle();
    expect(find.text('Previous page'), findsOneWidget);
    expect(tester.takeException(), isNull);
  });

  testWidgets(
    'Closed goal dialog cannot pop the goals page on save completion',
    (tester) async {
      final completer = Completer<void>();
      await openEditor(
        tester,
        PrescriptionsScreen(
          loadGoals: () => Stream.value([
            const ExerciseGoal(id: 'demo', name: 'Elbow flexion', target: 10),
          ]),
          loadReminderStatus: () async => false,
          changeReminder: (_) async {},
          saveGoal: (_, _) => completer.future,
        ),
      );
      await tester.tap(find.byTooltip('Edit exercise goal'));
      await tester.pumpAndSettle();
      await tester.enterText(find.byType(TextFormField), '20');
      await tester.tap(find.text('Save goal'));
      await tester.pump();
      await tester.tap(find.text('Close (save may continue)'));
      await tester.pump(const Duration(milliseconds: 20));
      completer.complete();
      await tester.pumpAndSettle();
      expect(find.text('Exercise goals & reminders'), findsOneWidget);
      expect(tester.takeException(), isNull);
    },
  );

  testWidgets('Reminder state is restored and a failed change retains it', (
    tester,
  ) async {
    final semantics = tester.ensureSemantics();
    try {
      final completer = Completer<void>();
      await pumpScreen(
        tester,
        PrescriptionsScreen(
          loadGoals: () => Stream.value([
            const ExerciseGoal(id: 'demo', name: 'Elbow flexion', target: 10),
          ]),
          loadReminderStatus: () async => true,
          changeReminder: (_) => completer.future,
          saveGoal: (_, _) async {},
        ),
      );
      expect(tester.widget<Switch>(find.byType(Switch)).value, isTrue);
      expect(
        tester.getSemantics(find.byType(Switch)).getSemanticsData().label,
        'Daily exercise reminder',
      );
      await tester.tap(find.byType(Switch));
      await tester.pump();
      expect(find.byType(Switch), findsNothing);
      completer.completeError(PlatformException(code: 'permission-denied'));
      await tester.pumpAndSettle();
      expect(tester.widget<Switch>(find.byType(Switch)).value, isTrue);
      expect(find.textContaining('not confirmed'), findsOneWidget);
    } finally {
      semantics.dispose();
    }
  });

  for (final angle in [null, 82.4]) {
    testWidgets('Tracker fits long names on a small phone with angle $angle', (
      tester,
    ) async {
      await pumpScreen(
        tester,
        ArmTrackerView(
          exerciseName: 'Long supervised arm rehabilitation exercise name',
          status: 'Synthetic status',
          angle: angle,
          repetitions: 12,
          upper: vector.Quaternion.identity(),
          forearm: vector.Quaternion.identity(),
          feedback: 'Synthetic feedback only.',
          feedbackColor: KinetismTheme.accent,
          onOpenLog: () {},
          onConnect: () {},
          onSave: () {},
        ),
        size: const Size(320, 640),
        scale: 2,
      );
      expect(
        tester.getTopLeft(find.text('Raw angle')).dy,
        lessThan(tester.getTopLeft(find.text('Movement demonstration')).dy),
      );
      await tester.drag(
        find
            .descendant(
              of: find.byType(ArmTrackerView),
              matching: find.byType(SingleChildScrollView),
            )
            .first,
        const Offset(0, -1600),
      );
      await tester.pump();
      expect(tester.takeException(), isNull);
    });
  }

  for (final surface in ['brand', 'exercises', 'reports', 'goals', 'error']) {
    testWidgets('$surface fits a narrow phone with enlarged text', (
      tester,
    ) async {
      final screen = switch (surface) {
        'brand' => Scaffold(
          appBar: AppBar(title: const KinetismBrand()),
          drawer: const Drawer(),
        ),
        'exercises' => SelectExerciseScreen(
          loadExercises: () => Stream.value([
            const ExerciseSummary(
              id: 'long',
              name: 'A long therapist-defined arm movement',
              joint: 'Elbow joint',
            ),
          ]),
          onSelect: (_) {},
        ),
        'reports' => PastReportsScreen(
          loadReports: () => Stream.value([
            ArchivedReport(date: now, analysis: 'Synthetic report'),
            const ArchivedReport(),
          ]),
          onOpen: (_) {},
        ),
        'goals' => PrescriptionsScreen(
          loadGoals: () => Stream.value([
            const ExerciseGoal(
              id: 'long',
              name: 'A long therapist-defined arm movement',
              target: 1000000,
            ),
          ]),
          loadReminderStatus: () async => false,
          changeReminder: (_) async {},
          saveGoal: (_, _) async {},
        ),
        _ => const Scaffold(
          body: StatusPanel(
            icon: Icons.cloud_off,
            title: 'Exercises unavailable',
            message:
                'Could not load exercises. Check the connection and database access, then retry.',
          ),
        ),
      };
      await pumpScreen(tester, screen, size: const Size(320, 640), scale: 2);
      expect(tester.takeException(), isNull);
    });
  }

  testWidgets('Local preview does not create a Firebase app', (tester) async {
    expect(Firebase.apps, isEmpty);
    await tester.pumpWidget(const KinetismPreviewApp());
    await tester.pumpAndSettle();
    expect(find.textContaining('Preview: synthetic data.'), findsOneWidget);
    expect(find.text('Select patient'), findsOneWidget);
    await tester.tap(find.text('Ahmad Razak'));
    await tester.pumpAndSettle();
    expect(find.text('Measure'), findsOneWidget);
    expect(find.text('Record log'), findsOneWidget);
    expect(Firebase.apps, isEmpty);
    expect(tester.takeException(), isNull);
  });

  testWidgets('Preview screen menu navigates below the overlay', (
    tester,
  ) async {
    tester.view.physicalSize = const Size(320, 640);
    tester.view.devicePixelRatio = 1;
    tester.platformDispatcher.textScaleFactorTestValue = 2;
    addTearDown(tester.view.resetPhysicalSize);
    addTearDown(tester.view.resetDevicePixelRatio);
    addTearDown(tester.platformDispatcher.clearTextScaleFactorTestValue);
    await tester.pumpWidget(const KinetismPreviewApp());
    await tester.pumpAndSettle();
    await tester.tap(find.byTooltip('Preview screens'));
    await tester.pumpAndSettle();
    await tester.ensureVisible(find.text('Report archive'));
    await tester.tap(find.text('Report archive'));
    await tester.pumpAndSettle();
    expect(find.widgetWithText(AppBar, 'Report archive'), findsOneWidget);
    expect(find.byType(PastReportsScreen), findsOneWidget);
    expect(Firebase.apps, isEmpty);
    expect(tester.takeException(), isNull);
  });
}
