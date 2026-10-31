import 'dart:async';
import 'dart:math' as math;

import 'package:flutter/material.dart';
import 'package:vector_math/vector_math_64.dart' as vector;

import 'add_exercise.dart';
import 'app_design.dart';
import 'arm_tracker_view.dart';
import 'clinical_workflow.dart';
import 'dashboard.dart';
import 'past_reports.dart';
import 'prescriptions.dart';
import 'select_exercise.dart';
import 'session_log.dart';

void main() => runApp(const KinetismPreviewApp());

class KinetismPreviewApp extends StatefulWidget {
  const KinetismPreviewApp({super.key});

  @override
  State<KinetismPreviewApp> createState() => _KinetismPreviewAppState();
}

class _KinetismPreviewAppState extends State<KinetismPreviewApp> {
  final _navigator = GlobalKey<NavigatorState>();
  final _exerciseChanges = StreamController<List<ExerciseSummary>>.broadcast();
  final _sessionChanges = StreamController<List<SessionLog>>.broadcast();
  final _goalChanges = StreamController<List<ExerciseGoal>>.broadcast();
  final _now = DateTime(2026, 10, 31, 12);
  late final KinetismClinicalController _clinicalController;
  late final List<SessionLog> _logs;
  final _exercises = <ExerciseSummary>[
    const ExerciseSummary(
      id: 'demo-elbow',
      name: 'Elbow flexion',
      joint: 'Elbow',
    ),
    const ExerciseSummary(
      id: 'demo-wrist',
      name: 'Wrist extension',
      joint: 'Wrist',
    ),
    const ExerciseSummary(
      id: 'demo-shoulder',
      name: 'Shoulder abduction',
      joint: 'Shoulder',
    ),
  ];
  final _goals = <ExerciseGoal>[
    const ExerciseGoal(id: 'demo-elbow', name: 'Elbow flexion', target: 10),
    const ExerciseGoal(id: 'demo-wrist', name: 'Wrist extension', target: 15),
  ];
  bool _reminder = false;
  String _selectedExercise = 'Elbow flexion';

  @override
  void initState() {
    super.initState();
    _clinicalController = KinetismClinicalController(isPreview: true);
    _logs = [
      _log(
        'demo-session-1',
        'Elbow flexion',
        12,
        _now.subtract(const Duration(hours: 1)),
      ),
      _log(
        'demo-session-2',
        'Wrist extension',
        8,
        _now.subtract(const Duration(hours: 2)),
      ),
      _log(
        'demo-session-3',
        'Elbow flexion',
        10,
        _now.subtract(const Duration(days: 1)),
      ),
      _log(
        'demo-session-4',
        'Shoulder abduction',
        6,
        _now.subtract(const Duration(days: 2)),
      ),
      _log(
        'demo-session-5',
        'Elbow flexion',
        0,
        _now.subtract(const Duration(days: 3)),
      ),
      SessionLog.fromMap('demo-incomplete', {
        'exerciseName': 'Legacy sample with missing fields',
      }),
    ];
  }

  SessionLog _log(String id, String name, int count, DateTime date) =>
      SessionLog.fromMap(id, {
        'exerciseName': name,
        'patientId': 'synthetic-id-001',
        'repsCompleted': count,
        'date': date,
      });

  Stream<List<ExerciseSummary>> _loadExercises() async* {
    yield List.of(_exercises);
    yield* _exerciseChanges.stream;
  }

  Stream<List<SessionLog>> _loadSessions() async* {
    yield List.of(_logs);
    yield* _sessionChanges.stream;
  }

  Stream<List<ExerciseGoal>> _loadGoals() async* {
    yield List.of(_goals);
    yield* _goalChanges.stream;
  }

  @override
  void dispose() {
    _clinicalController.dispose();
    _exerciseChanges.close();
    _sessionChanges.close();
    _goalChanges.close();
    super.dispose();
  }

  void _notice(String message) {
    final context = _navigator.currentContext!;
    ScaffoldMessenger.of(
      context,
    ).showSnackBar(SnackBar(content: Text(message)));
  }

  void _handlePreviewRoute(String route) {
    switch (route) {
      case '/clinical/patients':
        _clinicalController.goToStep(ClinicalStep.selectPatient);
        _navigator.currentState!.pushNamed('/');
      case '/clinical/hub':
        if (_clinicalController.selectedPatient == null) {
          _clinicalController.selectPatient(_clinicalController.patients.first);
        } else {
          _clinicalController.goToStep(ClinicalStep.patientHub);
        }
        _navigator.currentState!.pushNamed('/');
      case '/clinical/body-part':
        if (_clinicalController.selectedPatient == null) {
          _clinicalController.selectPatient(_clinicalController.patients.first);
        }
        _clinicalController.openMeasureSection();
        _navigator.currentState!.pushNamed('/');
      case '/clinical/anthropometrics':
        if (_clinicalController.selectedPatient == null) {
          _clinicalController.selectPatient(_clinicalController.patients.first);
        }
        _clinicalController.selectBodyPart('Arm');
        _navigator.currentState!.pushNamed('/');
      case '/clinical/measure':
        if (_clinicalController.selectedPatient == null) {
          _clinicalController.selectPatient(_clinicalController.patients.first);
        }
        _clinicalController.goToStep(ClinicalStep.activeMeasurement);
        _navigator.currentState!.pushNamed('/');
      case '/clinical/patient-log':
        if (_clinicalController.selectedPatient == null) {
          _clinicalController.selectPatient(_clinicalController.patients.first);
        }
        _clinicalController.openPatientRecordLog();
        _navigator.currentState!.pushNamed('/');
      case '/clinical/overall-log':
        _clinicalController.openOverallRecordLog();
        _navigator.currentState!.pushNamed('/');
      default:
        _navigator.currentState!.pushNamed(route);
    }
  }

  @override
  Widget build(BuildContext context) {
    const screens = <String, String>{
      '/clinical/patients': '1. Select patient',
      '/clinical/hub': '2. Measure or Record log',
      '/clinical/body-part': '3. Select body part (Arm)',
      '/clinical/anthropometrics': '4. Arm measurements & guide',
      '/clinical/measure': '5–7. Measure & Record position',
      '/clinical/patient-log': '8. Patient record log',
      '/clinical/overall-log': '9. Overall record log',
      '/legacy-sessions': 'Session log',
      '/exercises': 'Start exercise',
      '/add': 'Add exercise',
      '/tracker': 'Arm tracker',
      '/reports': 'Report archive',
      '/goals': 'Goals & reminders',
    };
    return MaterialApp(
      navigatorKey: _navigator,
      title: 'Kinetism - synthetic preview',
      debugShowCheckedModeBanner: false,
      theme: KinetismTheme.dark,
      routes: _wrapRoutes({
        '/': (_) =>
            KinetismClinicalFlowScreen(controller: _clinicalController),
        '/legacy-sessions': (context) => _home(context),
        for (final state in ['empty', 'loading', 'error', 'cache', 'pending'])
          '/states/$state': (context) => _home(context, state: state),
        '/exercises': (context) => SelectExerciseScreen(
          loadExercises: _loadExercises,
          onSelect: (exercise) {
            _selectedExercise = exercise.name;
            Navigator.pushNamed(context, '/tracker');
          },
          onAddExercise: () => Navigator.pushNamed(context, '/add'),
        ),
        '/exercises/error': (_) => SelectExerciseScreen(
          loadExercises: () =>
              Stream.error(StateError('Synthetic load failure')),
          onAddExercise: () => _navigator.currentState!.pushNamed('/add'),
          onSelect: (_) => _notice('No sensor connection in preview.'),
        ),
        '/add': (_) => AddExerciseScreen(
          savedMessage:
              'Saved only in this temporary preview. No cloud upload.',
          saveExercise: (recipe) async {
            final name = recipe['name'];
            final joint = recipe['targetJoint'];
            final goal = recipe['dailyGoal'];
            if (name is! String || joint is! String || goal is! int) {
              throw const FormatException('Preview recipe fields are invalid.');
            }
            _exercises.add(
              ExerciseSummary(
                id: 'demo-exercise-${_exercises.length}',
                name: name,
                joint: joint,
              ),
            );
            _exerciseChanges.add(List.of(_exercises));
            _goals.add(
              ExerciseGoal(
                id: _exercises.last.id,
                name: _exercises.last.name,
                target: goal,
              ),
            );
            _goalChanges.add(List.of(_goals));
          },
        ),
        '/tracker': (context) => ArmTrackerView(
          exerciseName: _selectedExercise,
          status: 'Synthetic sample, not a live connection',
          angle: 82.4,
          repetitions: 12,
          upper: vector.Quaternion.identity(),
          forearm: vector.Quaternion.axisAngle(
            vector.Vector3(0, 0, 1),
            (180 - 82.4) * math.pi / 180,
          ),
          feedback:
              'Preview pose only. No movement or strength assessment performed.',
          feedbackColor: KinetismTheme.accent,
          videoMessage: 'No external video is loaded in the local preview.',
          onOpenLog: () => Navigator.pushNamed(context, '/'),
          onConnect: () =>
              _notice('Preview does not scan for Bluetooth devices.'),
          onSave: () {
            _logs.insert(
              0,
              _log('demo-capture-${_logs.length}', _selectedExercise, 12, _now),
            );
            _sessionChanges.add(List.of(_logs));
            _notice('Synthetic repetitions added to temporary preview only.');
            Navigator.pop(context);
          },
        ),
        '/reports': (context) => PastReportsScreen(
          loadReports: () => Stream.value([
            ArchivedReport(
              date: _now.subtract(const Duration(days: 1)),
              analysis:
                  'Synthetic report example. Only repetition counts are available. '
                  'No strength, force or MMT result has been calculated.',
            ),
            const ArchivedReport(
              analysis: 'Synthetic report missing its date.',
            ),
          ]),
          onOpen: (report) => showDialog<void>(
            context: context,
            builder: (_) => AlertDialog(
              title: const Text('Synthetic report example'),
              content: Text(report.analysis!),
              actions: [
                TextButton(
                  onPressed: () => Navigator.pop(context),
                  child: const Text('Close'),
                ),
              ],
            ),
          ),
        ),
        '/reports/error': (_) => PastReportsScreen(
          loadReports: () => Stream.error(StateError('Synthetic load failure')),
          onOpen: (_) => _notice('No report service is connected.'),
        ),
        '/goals': (_) => PrescriptionsScreen(
          loadGoals: _loadGoals,
          loadReminderStatus: () async => _reminder,
          changeReminder: (value) async => _reminder = value,
          saveGoal: (id, target) async {
            final index = _goals.indexWhere((goal) => goal.id == id);
            final previous = _goals[index];
            _goals[index] = ExerciseGoal(
              id: id,
              name: previous.name,
              target: target,
            );
            _goalChanges.add(List.of(_goals));
          },
        ),
      }, screens),
    );
  }

  Map<String, WidgetBuilder> _wrapRoutes(
    Map<String, WidgetBuilder> routes,
    Map<String, String> screens,
  ) {
    return routes.map(
      (name, builder) => MapEntry(
        name,
        (context) => Material(
          color: KinetismTheme.surface,
          child: Column(
            children: [
              SafeArea(
                bottom: false,
                child: Padding(
                  padding: const EdgeInsets.symmetric(
                    horizontal: 16,
                    vertical: 8,
                  ),
                  child: Row(
                    children: [
                      const Expanded(
                        child: Text(
                          'Preview: synthetic data. No cloud, AI, notifications or sensors.',
                          style: TextStyle(color: KinetismTheme.secondaryText),
                        ),
                      ),
                      PopupMenuButton<String>(
                        tooltip: 'Preview screens',
                        onSelected: _handlePreviewRoute,
                        itemBuilder: (_) => screens.entries
                            .map(
                              (entry) => PopupMenuItem(
                                value: entry.key,
                                child: Text(entry.value),
                              ),
                            )
                            .toList(),
                      ),
                    ],
                  ),
                ),
              ),
              Expanded(child: builder(context)),
            ],
          ),
        ),
      ),
    );
  }

  Widget _home(BuildContext context, {String? state}) {
    return Scaffold(
      appBar: AppBar(title: const KinetismBrand()),
      body: StreamBuilder<List<SessionLog>>(
        stream: _loadSessions(),
        initialData: _logs,
        builder: (context, snapshot) => SessionLogView(
          logs: state == 'empty' || state == 'loading' || state == 'error'
              ? const []
              : snapshot.data ?? _logs,
          loading: state == 'loading',
          errorMessage: state == 'error'
              ? 'Synthetic connection failure. No actual cloud request was made.'
              : null,
          fromCache: state == 'cache',
          pendingWrites: state == 'pending',
          now: _now,
          preview: true,
          onRetry: () => Navigator.pushNamed(context, '/'),
          onStartExercise: () => Navigator.pushNamed(context, '/exercises'),
          onReport: (_) => _notice(
            'No AI or report generation service is called in preview.',
          ),
        ),
      ),
    );
  }
}
