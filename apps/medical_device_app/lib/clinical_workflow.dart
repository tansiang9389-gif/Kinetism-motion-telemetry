import 'dart:async';
import 'dart:math' as math;

import 'package:flutter/material.dart';
import 'package:vector_math/vector_math_64.dart' as vector;

import 'app_design.dart';
import 'arm_tracker_view.dart' show ArmSkeletonPainter;
import 'clinical_models.dart';

enum ClinicalStep {
  selectPatient,
  patientHub,
  selectBodyPart,
  armAnthropometrics,
  activeMeasurement,
  patientRecordLog,
  overallRecordLog,
}

class KinetismClinicalController extends ChangeNotifier {
  final List<PatientProfile> _patients;
  final List<MeasurementRecord> _records;
  final Future<void> Function(PatientProfile patient)? onPersistPatient;
  final Future<void> Function(MeasurementRecord record)? onPersistRecord;
  final VoidCallback? onConnectSensors;
  final bool isPreview;

  ClinicalStep _step = ClinicalStep.selectPatient;
  PatientProfile? _selectedPatient;
  String _selectedBodyPart = 'Arm';
  ArmAnthropometrics _currentAnthropometrics = const ArmAnthropometrics(
    bicepCircumferenceCm: 29.0,
    forearmCircumferenceCm: 24.5,
    armLengthCm: 55.0,
  );

  double _liveAngleDegrees = 115.0;
  double _liveAccelMagnitudeG = 0.22;
  String _sensorStatus = 'Sensors worn & ready (Physio_A paired)';
  bool _hasCapturedPosition = false;
  MeasurementRecord? _lastCapturedRecord;
  final List<MeasurementRecord> _sessionCaptures = [];

  KinetismClinicalController({
    List<PatientProfile>? initialPatients,
    List<MeasurementRecord>? initialRecords,
    this.onPersistPatient,
    this.onPersistRecord,
    this.onConnectSensors,
    this.isPreview = false,
  }) : _patients = List<PatientProfile>.from(
         initialPatients ?? _defaultPatients(),
       ),
       _records = List<MeasurementRecord>.from(
         initialRecords ?? _defaultRecords(),
       );

  ClinicalStep get step => _step;
  List<PatientProfile> get patients => List.unmodifiable(_patients);
  List<MeasurementRecord> get allRecords => List.unmodifiable(_records);
  PatientProfile? get selectedPatient => _selectedPatient;
  String get selectedBodyPart => _selectedBodyPart;
  ArmAnthropometrics get currentAnthropometrics => _currentAnthropometrics;
  double get liveAngleDegrees => _liveAngleDegrees;
  double get liveAccelMagnitudeG => _liveAccelMagnitudeG;
  String get sensorStatus => _sensorStatus;
  bool get hasCapturedPosition => _hasCapturedPosition;
  MeasurementRecord? get lastCapturedRecord => _lastCapturedRecord;
  List<MeasurementRecord> get sessionCaptures =>
      List.unmodifiable(_sessionCaptures);

  double get liveForceNewtons => _currentAnthropometrics.calculateForceNewtons(
    angleDegrees: _liveAngleDegrees,
    accelMagnitudeG: _liveAccelMagnitudeG,
  );

  MmtAssessment get liveMmtAssessment => MmtAssessment.evaluate(
    angleDegrees: _liveAngleDegrees,
    forceNewtons: liveForceNewtons,
    anthropometrics: _currentAnthropometrics,
  );

  List<MeasurementRecord> recordsForPatient(String patientId) {
    return _records.where((r) => r.patientId == patientId).toList();
  }

  void syncExternalData({
    List<PatientProfile>? patients,
    List<MeasurementRecord>? records,
  }) {
    if (patients != null && patients.isNotEmpty) {
      for (final incoming in patients) {
        final index = _patients.indexWhere((p) => p.id == incoming.id);
        if (index >= 0) {
          _patients[index] = incoming;
        } else {
          _patients.insert(0, incoming);
        }
      }
      if (_selectedPatient != null) {
        final updated = _patients
            .where((p) => p.id == _selectedPatient!.id)
            .firstOrNull;
        if (updated != null) _selectedPatient = updated;
      }
    }
    if (records != null && records.isNotEmpty) {
      for (final incoming in records) {
        final exists = _records.any((r) => r.id == incoming.id);
        if (!exists) {
          _records.add(incoming);
        }
      }
      _records.sort((a, b) {
        final aTime = a.recordedAt ?? DateTime.fromMillisecondsSinceEpoch(0);
        final bTime = b.recordedAt ?? DateTime.fromMillisecondsSinceEpoch(0);
        return bTime.compareTo(aTime);
      });
    }
    notifyListeners();
  }

  void goToStep(ClinicalStep nextStep) {
    _step = nextStep;
    notifyListeners();
  }

  void selectPatient(PatientProfile patient) {
    _selectedPatient = patient;
    if (patient.armMeasurements != null) {
      _currentAnthropometrics = patient.armMeasurements!;
    }
    _step = ClinicalStep.patientHub;
    notifyListeners();
  }

  Future<PatientProfile> createAndSelectPatient({
    required String name,
    String? customCode,
  }) async {
    final cleanName = name.trim();
    final nextNumber = _patients.length + 101;
    final cleanCode = (customCode != null && customCode.trim().isNotEmpty)
        ? customCode.trim()
        : 'PT-$nextNumber';
    final id = 'patient-${DateTime.now().millisecondsSinceEpoch}';
    final patient = PatientProfile(
      id: id,
      name: cleanName,
      code: cleanCode,
      updatedAt: DateTime.now(),
    );
    _patients.insert(0, patient);
    _selectedPatient = patient;
    _step = ClinicalStep.patientHub;
    notifyListeners();
    if (onPersistPatient != null) {
      await onPersistPatient!(patient);
    }
    return patient;
  }

  void openMeasureSection() {
    _step = ClinicalStep.selectBodyPart;
    notifyListeners();
  }

  void openPatientRecordLog() {
    _step = ClinicalStep.patientRecordLog;
    notifyListeners();
  }

  void openOverallRecordLog() {
    _step = ClinicalStep.overallRecordLog;
    notifyListeners();
  }

  void selectBodyPart(String bodyPart) {
    _selectedBodyPart = bodyPart;
    if (_selectedPatient?.armMeasurements != null) {
      _currentAnthropometrics = _selectedPatient!.armMeasurements!;
    }
    _step = ClinicalStep.armAnthropometrics;
    notifyListeners();
  }

  Future<void> saveAnthropometricsAndStartMeasuring(
    ArmAnthropometrics anthropometrics,
  ) async {
    _currentAnthropometrics = anthropometrics;
    if (_selectedPatient != null) {
      final updated = _selectedPatient!.copyWith(
        armMeasurements: anthropometrics,
        updatedAt: DateTime.now(),
      );
      _selectedPatient = updated;
      final idx = _patients.indexWhere((p) => p.id == updated.id);
      if (idx >= 0) {
        _patients[idx] = updated;
      }
      if (onPersistPatient != null) {
        await onPersistPatient!(updated);
      }
    }
    _hasCapturedPosition = false;
    _lastCapturedRecord = null;
    _sessionCaptures.clear();
    _step = ClinicalStep.activeMeasurement;
    notifyListeners();
  }

  void updateLiveSensorPose({
    required double angleDegrees,
    double? accelMagnitudeG,
    String? status,
  }) {
    _liveAngleDegrees = angleDegrees.clamp(0.0, 180.0);
    if (accelMagnitudeG != null && accelMagnitudeG.isFinite) {
      _liveAccelMagnitudeG = accelMagnitudeG.clamp(0.0, 5.0);
    }
    if (status != null) {
      _sensorStatus = status;
    }
    notifyListeners();
  }

  Future<MeasurementRecord> recordCurrentPosition() async {
    final patient =
        _selectedPatient ??
        const PatientProfile(
          id: 'patient-default',
          name: 'Walk-in Patient',
          code: 'PT-100',
        );
    final angle = double.parse(_liveAngleDegrees.toStringAsFixed(1));
    final force = liveForceNewtons;
    final mmt = liveMmtAssessment;
    final existingCount = recordsForPatient(patient.id).length;
    final record = MeasurementRecord(
      id: 'rec-${DateTime.now().microsecondsSinceEpoch}-${_records.length}',
      patientId: patient.id,
      patientName: patient.name,
      bodyPart: _selectedBodyPart,
      bicepCircumferenceCm: _currentAnthropometrics.bicepCircumferenceCm,
      forearmCircumferenceCm: _currentAnthropometrics.forearmCircumferenceCm,
      armLengthCm: _currentAnthropometrics.armLengthCm,
      angleDegrees: angle,
      forceNewtons: force,
      mmtGrade: mmt.grade,
      mmtTitle: mmt.title,
      mmtRationale: mmt.rationale,
      attemptNumber: existingCount + 1,
      recordedAt: DateTime.now(),
    );
    _records.insert(0, record);
    _sessionCaptures.insert(0, record);
    _lastCapturedRecord = record;
    _hasCapturedPosition = true;
    notifyListeners();
    if (onPersistRecord != null) {
      await onPersistRecord!(record);
    }
    return record;
  }

  void prepareRecordAgain() {
    _hasCapturedPosition = false;
    notifyListeners();
  }

  static List<PatientProfile> _defaultPatients() {
    final now = DateTime(2026, 10, 31, 14, 30);
    return [
      PatientProfile(
        id: 'patient-ahmad',
        name: 'Ahmad Razak',
        code: 'PT-101',
        armMeasurements: const ArmAnthropometrics(
          bicepCircumferenceCm: 30.5,
          forearmCircumferenceCm: 25.0,
          armLengthCm: 56.0,
        ),
        updatedAt: now.subtract(const Duration(hours: 2)),
      ),
      PatientProfile(
        id: 'patient-siti',
        name: 'Siti Aminah',
        code: 'PT-102',
        armMeasurements: const ArmAnthropometrics(
          bicepCircumferenceCm: 27.0,
          forearmCircumferenceCm: 22.5,
          armLengthCm: 52.0,
        ),
        updatedAt: now.subtract(const Duration(days: 1)),
      ),
      PatientProfile(
        id: 'patient-lim',
        name: 'Lim Wei Jie',
        code: 'PT-103',
        armMeasurements: const ArmAnthropometrics(
          bicepCircumferenceCm: 32.0,
          forearmCircumferenceCm: 26.5,
          armLengthCm: 58.0,
        ),
        updatedAt: now.subtract(const Duration(days: 2)),
      ),
    ];
  }

  static List<MeasurementRecord> _defaultRecords() {
    final now = DateTime(2026, 10, 31, 14, 30);
    const ahmadArm = ArmAnthropometrics(
      bicepCircumferenceCm: 30.5,
      forearmCircumferenceCm: 25.0,
      armLengthCm: 56.0,
    );
    const sitiArm = ArmAnthropometrics(
      bicepCircumferenceCm: 27.0,
      forearmCircumferenceCm: 22.5,
      armLengthCm: 52.0,
    );
    const limArm = ArmAnthropometrics(
      bicepCircumferenceCm: 32.0,
      forearmCircumferenceCm: 26.5,
      armLengthCm: 58.0,
    );

    final f1 = ahmadArm.calculateForceNewtons(angleDegrees: 122.0);
    final m1 = MmtAssessment.evaluate(
      angleDegrees: 122.0,
      forceNewtons: f1,
      anthropometrics: ahmadArm,
    );

    final f2 = sitiArm.calculateForceNewtons(angleDegrees: 94.0);
    final m2 = MmtAssessment.evaluate(
      angleDegrees: 94.0,
      forceNewtons: f2,
      anthropometrics: sitiArm,
    );

    final f3 = limArm.calculateForceNewtons(angleDegrees: 134.0);
    final m3 = MmtAssessment.evaluate(
      angleDegrees: 134.0,
      forceNewtons: f3,
      anthropometrics: limArm,
    );

    return [
      MeasurementRecord(
        id: 'rec-seed-1',
        patientId: 'patient-ahmad',
        patientName: 'Ahmad Razak',
        bodyPart: 'Arm',
        bicepCircumferenceCm: ahmadArm.bicepCircumferenceCm,
        forearmCircumferenceCm: ahmadArm.forearmCircumferenceCm,
        armLengthCm: ahmadArm.armLengthCm,
        angleDegrees: 122.0,
        forceNewtons: f1,
        mmtGrade: m1.grade,
        mmtTitle: m1.title,
        mmtRationale: m1.rationale,
        attemptNumber: 1,
        recordedAt: now.subtract(const Duration(hours: 2)),
      ),
      MeasurementRecord(
        id: 'rec-seed-2',
        patientId: 'patient-siti',
        patientName: 'Siti Aminah',
        bodyPart: 'Arm',
        bicepCircumferenceCm: sitiArm.bicepCircumferenceCm,
        forearmCircumferenceCm: sitiArm.forearmCircumferenceCm,
        armLengthCm: sitiArm.armLengthCm,
        angleDegrees: 94.0,
        forceNewtons: f2,
        mmtGrade: m2.grade,
        mmtTitle: m2.title,
        mmtRationale: m2.rationale,
        attemptNumber: 1,
        recordedAt: now.subtract(const Duration(days: 1)),
      ),
      MeasurementRecord(
        id: 'rec-seed-3',
        patientId: 'patient-lim',
        patientName: 'Lim Wei Jie',
        bodyPart: 'Arm',
        bicepCircumferenceCm: limArm.bicepCircumferenceCm,
        forearmCircumferenceCm: limArm.forearmCircumferenceCm,
        armLengthCm: limArm.armLengthCm,
        angleDegrees: 134.0,
        forceNewtons: f3,
        mmtGrade: m3.grade,
        mmtTitle: m3.title,
        mmtRationale: m3.rationale,
        attemptNumber: 1,
        recordedAt: now.subtract(const Duration(days: 2)),
      ),
    ];
  }
}

class KinetismClinicalFlowScreen extends StatefulWidget {
  final KinetismClinicalController controller;
  final Widget? drawer;

  const KinetismClinicalFlowScreen({
    super.key,
    required this.controller,
    this.drawer,
  });

  @override
  State<KinetismClinicalFlowScreen> createState() =>
      _KinetismClinicalFlowScreenState();
}

class _KinetismClinicalFlowScreenState
    extends State<KinetismClinicalFlowScreen> {
  KinetismClinicalController get _controller => widget.controller;

  @override
  void initState() {
    super.initState();
    _controller.addListener(_onChanged);
  }

  @override
  void didUpdateWidget(covariant KinetismClinicalFlowScreen oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.controller != widget.controller) {
      oldWidget.controller.removeListener(_onChanged);
      widget.controller.addListener(_onChanged);
    }
  }

  @override
  void dispose() {
    _controller.removeListener(_onChanged);
    super.dispose();
  }

  void _onChanged() {
    if (mounted) setState(() {});
  }

  void _handleBack() {
    switch (_controller.step) {
      case ClinicalStep.selectPatient:
        break;
      case ClinicalStep.patientHub:
        _controller.goToStep(ClinicalStep.selectPatient);
      case ClinicalStep.selectBodyPart:
        _controller.goToStep(ClinicalStep.patientHub);
      case ClinicalStep.armAnthropometrics:
        _controller.goToStep(ClinicalStep.selectBodyPart);
      case ClinicalStep.activeMeasurement:
        _controller.goToStep(ClinicalStep.armAnthropometrics);
      case ClinicalStep.patientRecordLog:
        _controller.goToStep(
          _controller.selectedPatient != null
              ? ClinicalStep.patientHub
              : ClinicalStep.selectPatient,
        );
      case ClinicalStep.overallRecordLog:
        _controller.goToStep(
          _controller.selectedPatient != null
              ? ClinicalStep.patientHub
              : ClinicalStep.selectPatient,
        );
    }
  }

  String _stepTitle() {
    return switch (_controller.step) {
      ClinicalStep.selectPatient => 'Kinetism',
      ClinicalStep.patientHub =>
        _controller.selectedPatient?.name ?? 'Patient section',
      ClinicalStep.selectBodyPart => 'Select body part',
      ClinicalStep.armAnthropometrics => 'Arm measurements',
      ClinicalStep.activeMeasurement => 'Measure arm',
      ClinicalStep.patientRecordLog => 'Record log',
      ClinicalStep.overallRecordLog => 'Overall record log',
    };
  }

  @override
  Widget build(BuildContext context) {
    final isRoot = _controller.step == ClinicalStep.selectPatient;
    return Scaffold(
      drawer: isRoot ? widget.drawer : null,
      appBar: AppBar(
        leading: isRoot
            ? null
            : IconButton(
                tooltip: 'Back',
                icon: const Icon(Icons.arrow_back),
                onPressed: _handleBack,
              ),
        title: isRoot ? const KinetismBrand() : Text(_stepTitle()),
        actions: [
          if (_controller.step != ClinicalStep.overallRecordLog)
            IconButton(
              tooltip: 'Overall record log',
              icon: const Icon(Icons.library_books_outlined),
              onPressed: _controller.openOverallRecordLog,
            ),
          if (!isRoot)
            IconButton(
              tooltip: 'Select patient',
              icon: const Icon(Icons.people_outline),
              onPressed: () =>
                  _controller.goToStep(ClinicalStep.selectPatient),
            ),
        ],
      ),
      body: SafeArea(
        child: Align(
          alignment: Alignment.topCenter,
          child: ConstrainedBox(
            constraints: const BoxConstraints(maxWidth: 1080),
            child: KeyedSubtree(
              key: ValueKey(_controller.step),
              child: switch (_controller.step) {
                ClinicalStep.selectPatient => _PatientSelectionView(
                  controller: _controller,
                ),
                ClinicalStep.patientHub => _PatientHubView(
                  controller: _controller,
                ),
                ClinicalStep.selectBodyPart => _BodyPartSelectionView(
                  controller: _controller,
                ),
                ClinicalStep.armAnthropometrics => _ArmAnthropometricsView(
                  controller: _controller,
                ),
                ClinicalStep.activeMeasurement => _ActiveMeasurementView(
                  controller: _controller,
                ),
                ClinicalStep.patientRecordLog => _RecordLogView(
                  controller: _controller,
                  overall: false,
                ),
                ClinicalStep.overallRecordLog => _RecordLogView(
                  controller: _controller,
                  overall: true,
                ),
              },
            ),
          ),
        ),
      ),
    );
  }
}

class _StepProgressHeader extends StatelessWidget {
  final int stepNumber;
  final String label;
  final String title;
  final String subtitle;

  const _StepProgressHeader({
    required this.stepNumber,
    required this.label,
    required this.title,
    required this.subtitle,
  });

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Container(
          padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 4),
          decoration: BoxDecoration(
            color: KinetismTheme.accent.withValues(alpha: 0.14),
            borderRadius: BorderRadius.circular(6),
          ),
          child: Text(
            'STEP $stepNumber · ${label.toUpperCase()}',
            style: const TextStyle(
              color: KinetismTheme.accent,
              fontSize: 12,
              fontWeight: FontWeight.w700,
              letterSpacing: 0.6,
            ),
          ),
        ),
        const SizedBox(height: 6),
        Semantics(
          header: true,
          child: Text(
            title,
            style: Theme.of(context).textTheme.titleLarge?.copyWith(
              fontSize: 24,
              fontWeight: FontWeight.w700,
            ),
          ),
        ),
        const SizedBox(height: 4),
        Text(
          subtitle,
          style: const TextStyle(
            color: KinetismTheme.secondaryText,
            fontSize: 14,
          ),
        ),
      ],
    );
  }
}

/// STEP 1: Select patient (list of past patients, new patient, or overall record log)
class _PatientSelectionView extends StatefulWidget {
  final KinetismClinicalController controller;

  const _PatientSelectionView({required this.controller});

  @override
  State<_PatientSelectionView> createState() => _PatientSelectionViewState();
}

class _PatientSelectionViewState extends State<_PatientSelectionView> {
  final _searchCtrl = TextEditingController();
  String _query = '';

  @override
  void dispose() {
    _searchCtrl.dispose();
    super.dispose();
  }

  Future<void> _showNewPatientDialog() async {
    final nameCtrl = TextEditingController();
    final codeCtrl = TextEditingController();
    final formKey = GlobalKey<FormState>();

    final created = await showDialog<bool>(
      context: context,
      builder: (dialogContext) => AlertDialog(
        title: const Text('New patient'),
        content: SingleChildScrollView(
          child: Form(
            key: formKey,
            child: Column(
              mainAxisSize: MainAxisSize.min,
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                const Text(
                  'Register a patient to begin measuring or logging records.',
                  style: TextStyle(color: KinetismTheme.secondaryText),
                ),
                const SizedBox(height: 16),
                TextFormField(
                  key: const ValueKey('Patient name'),
                  controller: nameCtrl,
                  decoration: const InputDecoration(
                    labelText: 'Patient name',
                    hintText: 'e.g. Nurul Izzah',
                  ),
                  validator: (value) {
                    if (value == null || value.trim().isEmpty) {
                      return 'Enter the patient name.';
                    }
                    return null;
                  },
                ),
                const SizedBox(height: 12),
                TextFormField(
                  key: const ValueKey('Patient ID (optional)'),
                  controller: codeCtrl,
                  decoration: const InputDecoration(
                    labelText: 'Patient ID (optional)',
                    hintText: 'e.g. PT-104',
                  ),
                ),
              ],
            ),
          ),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(dialogContext, false),
            child: const Text('Cancel'),
          ),
          FilledButton(
            onPressed: () {
              if (formKey.currentState?.validate() != true) return;
              Navigator.pop(dialogContext, true);
            },
            child: const Text('Create & select'),
          ),
        ],
      ),
    );

    if (created == true && mounted) {
      await widget.controller.createAndSelectPatient(
        name: nameCtrl.text,
        customCode: codeCtrl.text,
      );
    }
  }

  @override
  Widget build(BuildContext context) {
    final patients = widget.controller.patients.where((p) {
      final q = _query.trim().toLowerCase();
      if (q.isEmpty) return true;
      return p.name.toLowerCase().contains(q) ||
          p.code.toLowerCase().contains(q);
    }).toList();

    return SingleChildScrollView(
      padding: const EdgeInsets.all(24),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          const _StepProgressHeader(
            stepNumber: 1,
            label: 'Patient & log selection',
            title: 'Select patient',
            subtitle:
                'Choose a past patient, register a new patient, or open the overall record log for end-of-day physiotherapist review.',
          ),
          const SizedBox(height: 20),
          Wrap(
            spacing: 12,
            runSpacing: 12,
            children: [
              FilledButton.icon(
                onPressed: _showNewPatientDialog,
                icon: const Icon(Icons.person_add_alt_1),
                label: const Text('New patient'),
              ),
              OutlinedButton.icon(
                onPressed: widget.controller.openOverallRecordLog,
                icon: const Icon(Icons.library_books_outlined),
                label: Text(
                  'Overall record log (${widget.controller.allRecords.length})',
                ),
              ),
            ],
          ),
          const SizedBox(height: 24),
          TextField(
            controller: _searchCtrl,
            onChanged: (v) => setState(() => _query = v),
            decoration: InputDecoration(
              labelText: 'Search past patients',
              hintText: 'Filter by patient name or ID',
              prefixIcon: const Icon(Icons.search),
              suffixIcon: _query.isEmpty
                  ? null
                  : IconButton(
                      tooltip: 'Clear patient search',
                      icon: const Icon(Icons.close),
                      onPressed: () {
                        _searchCtrl.clear();
                        setState(() => _query = '');
                      },
                    ),
            ),
          ),
          const SizedBox(height: 20),
          Text(
            'Past patients (${patients.length})',
            style: Theme.of(context).textTheme.titleLarge,
          ),
          const SizedBox(height: 12),
          if (patients.isEmpty)
            StatusPanel(
              icon: Icons.person_off_outlined,
              title: 'No matching patients',
              message:
                  'Register a new patient or clear your search filter to view past patients.',
              actionLabel: 'New patient',
              onAction: _showNewPatientDialog,
            )
          else
            ...patients.map((patient) {
              final recordCount = widget.controller
                  .recordsForPatient(patient.id)
                  .length;
              final arm = patient.armMeasurements;
              return Padding(
                padding: const EdgeInsets.only(bottom: 12),
                child: Card(
                  margin: EdgeInsets.zero,
                  child: ListTile(
                    contentPadding: const EdgeInsets.symmetric(
                      horizontal: 20,
                      vertical: 12,
                    ),
                    leading: CircleAvatar(
                      backgroundColor: KinetismTheme.accent.withValues(
                        alpha: 0.18,
                      ),
                      foregroundColor: KinetismTheme.accent,
                      child: const Icon(Icons.person_outline),
                    ),
                    title: Text(
                      patient.name,
                      style: Theme.of(context).textTheme.titleMedium,
                    ),
                    subtitle: Padding(
                      padding: const EdgeInsets.only(top: 6),
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          Text(
                            'ID: ${patient.code} · $recordCount saved record${recordCount == 1 ? '' : 's'}',
                            style: const TextStyle(
                              color: KinetismTheme.secondaryText,
                            ),
                          ),
                          if (arm != null) ...[
                            const SizedBox(height: 4),
                            Text(
                              'Arm profile: Bicep ${arm.bicepCircumferenceCm.toStringAsFixed(1)} cm · '
                              'Forearm ${arm.forearmCircumferenceCm.toStringAsFixed(1)} cm · '
                              'Length ${arm.armLengthCm.toStringAsFixed(1)} cm',
                              style: const TextStyle(
                                color: KinetismTheme.secondaryText,
                                fontSize: 13,
                              ),
                            ),
                          ],
                        ],
                      ),
                    ),
                    trailing: const Icon(Icons.chevron_right),
                    onTap: () => widget.controller.selectPatient(patient),
                  ),
                ),
              );
            }),
        ],
      ),
    );
  }
}

/// STEP 2: User selects 1 of 2 sections - Measure or Record log
class _PatientHubView extends StatelessWidget {
  final KinetismClinicalController controller;

  const _PatientHubView({required this.controller});

  @override
  Widget build(BuildContext context) {
    final patient = controller.selectedPatient!;
    final patientRecords = controller.recordsForPatient(patient.id);
    return SingleChildScrollView(
      padding: const EdgeInsets.all(24),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          _StepProgressHeader(
            stepNumber: 2,
            label: 'Select section',
            title: patient.name,
            subtitle:
                'Patient ID: ${patient.code}. Select 1 of 2 sections below: Measure or Record log.',
          ),
          const SizedBox(height: 24),
          LayoutBuilder(
            builder: (context, constraints) {
              final measureCard = _SectionOptionCard(
                icon: Icons.sensors,
                badge: 'SECTION 1',
                title: 'Measure',
                description:
                    'Select body part (Arm), key in bicep/forearm circumferences and arm length, and record angle, force, and AI MMT grade.',
                buttonLabel: 'Open Measure',
                primary: true,
                onTap: controller.openMeasureSection,
              );
              final logCard = _SectionOptionCard(
                icon: Icons.assignment_outlined,
                badge: 'SECTION 2',
                title: 'Record log',
                description:
                    'Refer to saved angle, force, and AI MMT grading records for ${patient.name} (${patientRecords.length} saved).',
                buttonLabel: 'Open Record log',
                primary: false,
                onTap: controller.openPatientRecordLog,
              );
              if (constraints.maxWidth >= 720) {
                return Row(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Expanded(child: measureCard),
                    const SizedBox(width: 20),
                    Expanded(child: logCard),
                  ],
                );
              }
              return Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  measureCard,
                  const SizedBox(height: 16),
                  logCard,
                ],
              );
            },
          ),
          const SizedBox(height: 24),
          Wrap(
            spacing: 12,
            runSpacing: 12,
            children: [
              OutlinedButton.icon(
                onPressed: controller.openOverallRecordLog,
                icon: const Icon(Icons.library_books_outlined),
                label: const Text('Overall record log'),
              ),
              TextButton.icon(
                onPressed: () =>
                    controller.goToStep(ClinicalStep.selectPatient),
                icon: const Icon(Icons.swap_horiz),
                label: const Text('Switch patient'),
              ),
            ],
          ),
        ],
      ),
    );
  }
}

class _SectionOptionCard extends StatelessWidget {
  final IconData icon;
  final String badge;
  final String title;
  final String description;
  final String buttonLabel;
  final bool primary;
  final VoidCallback onTap;

  const _SectionOptionCard({
    required this.icon,
    required this.badge,
    required this.title,
    required this.description,
    required this.buttonLabel,
    required this.primary,
    required this.onTap,
  });

  @override
  Widget build(BuildContext context) {
    return Card(
      margin: EdgeInsets.zero,
      child: InkWell(
        borderRadius: BorderRadius.circular(12),
        onTap: onTap,
        child: Padding(
          padding: const EdgeInsets.all(16),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Row(
                children: [
                  Icon(icon, color: KinetismTheme.accent, size: 26),
                  const SizedBox(width: 10),
                  Flexible(
                    child: Text(
                      badge,
                      style: const TextStyle(
                        color: KinetismTheme.accent,
                        fontWeight: FontWeight.w700,
                        fontSize: 12,
                      ),
                    ),
                  ),
                ],
              ),
              const SizedBox(height: 8),
              Text(title, style: Theme.of(context).textTheme.titleLarge),
              const SizedBox(height: 4),
              Text(
                description,
                style: const TextStyle(
                  color: KinetismTheme.secondaryText,
                  fontSize: 14,
                ),
              ),
              const SizedBox(height: 12),
              if (primary)
                FilledButton.icon(
                  onPressed: onTap,
                  icon: const Icon(Icons.arrow_forward),
                  label: Text(buttonLabel),
                )
              else
                OutlinedButton.icon(
                  onPressed: onTap,
                  icon: const Icon(Icons.receipt_long_outlined),
                  label: Text(buttonLabel),
                ),
            ],
          ),
        ),
      ),
    );
  }
}

/// STEP 3: When user selects Measure - ask which body part they are measuring (Arm first)
class _BodyPartSelectionView extends StatelessWidget {
  final KinetismClinicalController controller;

  const _BodyPartSelectionView({required this.controller});

  @override
  Widget build(BuildContext context) {
    final patientName = controller.selectedPatient?.name ?? 'Selected patient';
    return SingleChildScrollView(
      padding: const EdgeInsets.all(24),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          _StepProgressHeader(
            stepNumber: 3,
            label: 'Body part selection',
            title: 'Which body part are you measuring?',
            subtitle:
                'Patient: $patientName. Select the body part to configure anatomical dimensions for force calculation.',
          ),
          const SizedBox(height: 24),
          Card(
            margin: EdgeInsets.zero,
            child: InkWell(
              borderRadius: BorderRadius.circular(12),
              onTap: () => controller.selectBodyPart('Arm'),
              child: Padding(
                padding: const EdgeInsets.all(24),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Wrap(
                      spacing: 12,
                      runSpacing: 8,
                      crossAxisAlignment: WrapCrossAlignment.center,
                      children: [
                        const Icon(
                          Icons.accessibility_new,
                          color: KinetismTheme.accent,
                          size: 32,
                        ),
                        Text(
                          'Arm',
                          style: Theme.of(context).textTheme.titleLarge,
                        ),
                        Container(
                          padding: const EdgeInsets.symmetric(
                            horizontal: 10,
                            vertical: 4,
                          ),
                          decoration: BoxDecoration(
                            color: KinetismTheme.accent.withValues(alpha: 0.16),
                            borderRadius: BorderRadius.circular(6),
                          ),
                          child: const Text(
                            'Available now · Upper arm & forearm sensors',
                            style: TextStyle(
                              color: KinetismTheme.accent,
                              fontSize: 12,
                              fontWeight: FontWeight.w600,
                            ),
                          ),
                        ),
                      ],
                    ),
                    const SizedBox(height: 12),
                    const Text(
                      'Measures elbow joint angle (°), calculates biomechanical force (N) from bicep circumference, forearm circumference, and arm length, and provides AI MMT grading (Grades 0–5).',
                      style: TextStyle(color: KinetismTheme.secondaryText),
                    ),
                    const SizedBox(height: 20),
                    FilledButton.icon(
                      onPressed: () => controller.selectBodyPart('Arm'),
                      icon: const Icon(Icons.straighten),
                      label: const Text('Select Arm'),
                    ),
                  ],
                ),
              ),
            ),
          ),
          const SizedBox(height: 16),
          Card(
            margin: EdgeInsets.zero,
            child: ListTile(
              contentPadding: const EdgeInsets.symmetric(
                horizontal: 20,
                vertical: 12,
              ),
              leading: const Icon(
                Icons.directions_walk,
                color: KinetismTheme.secondaryText,
              ),
              title: const Text('Lower limb / Knee (Planned module)'),
              subtitle: const Text(
                'Version 1 focuses on Arm telemetry first.',
                style: TextStyle(color: KinetismTheme.secondaryText),
              ),
            ),
          ),
        ],
      ),
    );
  }
}

/// STEP 4 & 5: Key in circumference of bicep, circumference of forearm, length of arm
/// (with picture of where to measure) -> Press Start measuring button
class _ArmAnthropometricsView extends StatefulWidget {
  final KinetismClinicalController controller;

  const _ArmAnthropometricsView({required this.controller});

  @override
  State<_ArmAnthropometricsView> createState() =>
      _ArmAnthropometricsViewState();
}

class _ArmAnthropometricsViewState extends State<_ArmAnthropometricsView> {
  final _formKey = GlobalKey<FormState>();
  late final TextEditingController _bicepCtrl;
  late final TextEditingController _forearmCtrl;
  late final TextEditingController _lengthCtrl;

  @override
  void initState() {
    super.initState();
    final initial = widget.controller.currentAnthropometrics;
    _bicepCtrl = TextEditingController(
      text: initial.bicepCircumferenceCm.toStringAsFixed(1),
    );
    _forearmCtrl = TextEditingController(
      text: initial.forearmCircumferenceCm.toStringAsFixed(1),
    );
    _lengthCtrl = TextEditingController(
      text: initial.armLengthCm.toStringAsFixed(1),
    );
  }

  @override
  void dispose() {
    _bicepCtrl.dispose();
    _forearmCtrl.dispose();
    _lengthCtrl.dispose();
    super.dispose();
  }

  ArmAnthropometrics? _previewParsed() {
    final b = double.tryParse(_bicepCtrl.text.trim());
    final f = double.tryParse(_forearmCtrl.text.trim());
    final l = double.tryParse(_lengthCtrl.text.trim());
    if (b == null ||
        !b.isFinite ||
        b <= 0 ||
        f == null ||
        !f.isFinite ||
        f <= 0 ||
        l == null ||
        !l.isFinite ||
        l <= 0) {
      return null;
    }
    return ArmAnthropometrics(
      bicepCircumferenceCm: b,
      forearmCircumferenceCm: f,
      armLengthCm: l,
    );
  }

  Future<void> _submitAndStart() async {
    if (_formKey.currentState?.validate() != true) return;
    final anthropometrics = ArmAnthropometrics(
      bicepCircumferenceCm: double.parse(_bicepCtrl.text.trim()),
      forearmCircumferenceCm: double.parse(_forearmCtrl.text.trim()),
      armLengthCm: double.parse(_lengthCtrl.text.trim()),
    );
    await widget.controller.saveAnthropometricsAndStartMeasuring(
      anthropometrics,
    );
  }

  @override
  Widget build(BuildContext context) {
    final parsed = _previewParsed();
    return Form(
      key: _formKey,
      child: SingleChildScrollView(
        padding: const EdgeInsets.all(24),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            _StepProgressHeader(
              stepNumber: 4,
            label: 'Arm anthropometrics & guide',
            title: 'Enter arm dimensions',
            subtitle:
                'Follow the illustrated guide below to measure the patient\'s bicep circumference, forearm circumference, and arm length for accurate force calculation.',
          ),
          const SizedBox(height: 20),
          const ArmMeasurementGuideCard(),
          const SizedBox(height: 20),
          Card(
            margin: EdgeInsets.zero,
            child: Padding(
              padding: const EdgeInsets.all(20),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  Text(
                    'Patient arm measurements (cm)',
                    style: Theme.of(context).textTheme.titleLarge,
                  ),
                  const SizedBox(height: 16),
                  TextFormField(
                    key: const ValueKey('Circumference of bicep (cm)'),
                    controller: _bicepCtrl,
                    keyboardType: const TextInputType.numberWithOptions(
                      decimal: true,
                    ),
                    onChanged: (_) => setState(() {}),
                    decoration: const InputDecoration(
                      labelText: '1. Circumference of bicep (cm)',
                      hintText: 'Measure around midpoint of upper arm (e.g. 29.0)',
                    ),
                    validator: ArmAnthropometrics.validateBicepCircumference,
                  ),
                  const SizedBox(height: 16),
                  TextFormField(
                    key: const ValueKey('Circumference of forearm (cm)'),
                    controller: _forearmCtrl,
                    keyboardType: const TextInputType.numberWithOptions(
                      decimal: true,
                    ),
                    onChanged: (_) => setState(() {}),
                    decoration: const InputDecoration(
                      labelText: '2. Circumference of forearm (cm)',
                      hintText: 'Measure around widest proximal forearm (e.g. 24.5)',
                    ),
                    validator: ArmAnthropometrics.validateForearmCircumference,
                  ),
                  const SizedBox(height: 16),
                  TextFormField(
                    key: const ValueKey('Length of arm (cm)'),
                    controller: _lengthCtrl,
                    keyboardType: const TextInputType.numberWithOptions(
                      decimal: true,
                    ),
                    onChanged: (_) => setState(() {}),
                    decoration: const InputDecoration(
                      labelText: '3. Length of arm (cm)',
                      hintText: 'Shoulder acromion to wrist styloid (e.g. 55.0)',
                    ),
                    validator: ArmAnthropometrics.validateArmLength,
                  ),
                  if (parsed != null) ...[
                    const SizedBox(height: 20),
                    Container(
                      padding: const EdgeInsets.all(16),
                      decoration: BoxDecoration(
                        color: KinetismTheme.background,
                        borderRadius: BorderRadius.circular(8),
                      ),
                      child: Wrap(
                        spacing: 24,
                        runSpacing: 12,
                        children: [
                          _miniMetric(
                            'Estimated moving mass',
                            '${parsed.effectiveMovingMassKg.toStringAsFixed(2)} kg',
                          ),
                          _miniMetric(
                            'Arm volume',
                            '${parsed.armVolumeCm3.toStringAsFixed(0)} cm³',
                          ),
                          _miniMetric(
                            '90° reference force',
                            '${parsed.calculateForceNewtons(angleDegrees: 90).toStringAsFixed(1)} N',
                          ),
                        ],
                      ),
                    ),
                  ],
                  const SizedBox(height: 20),
                  const Text(
                    'Confirm the patient is wearing the upper-arm and forearm sensors before starting.',
                    style: TextStyle(color: KinetismTheme.secondaryText),
                  ),
                  const SizedBox(height: 16),
                  FilledButton.icon(
                    onPressed: _submitAndStart,
                    icon: const Icon(Icons.play_arrow),
                    label: const Text('Start measuring'),
                  ),
                ],
              ),
            ),
          ),
        ],
      ),
      ),
    );
  }

  Widget _miniMetric(String label, String value) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(
          label,
          style: const TextStyle(
            color: KinetismTheme.secondaryText,
            fontSize: 13,
          ),
        ),
        const SizedBox(height: 2),
        Text(
          value,
          style: const TextStyle(
            color: KinetismTheme.accent,
            fontWeight: FontWeight.w700,
            fontSize: 16,
          ),
        ),
      ],
    );
  }
}

/// Visual anatomical guide showing where to measure Bicep Circumference,
/// Forearm Circumference, and Length of Arm.
class ArmMeasurementGuideCard extends StatelessWidget {
  const ArmMeasurementGuideCard({super.key});

  @override
  Widget build(BuildContext context) {
    return Card(
      margin: EdgeInsets.zero,
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Text(
              'Where to measure on the arm',
              style: Theme.of(context).textTheme.titleMedium,
            ),
            const SizedBox(height: 4),
            const Text(
              'Measure at landmarks 1, 2, and 3 for accurate force calculation.',
              style: TextStyle(
                color: KinetismTheme.secondaryText,
                fontSize: 13,
              ),
            ),
            const SizedBox(height: 10),
            Container(
              height: 155,
              decoration: BoxDecoration(
                color: KinetismTheme.background,
                borderRadius: BorderRadius.circular(10),
              ),
              child: Semantics(
                label:
                    'Illustrated arm measurement guide showing 1 circumference of bicep at mid-upper arm, 2 circumference of forearm at proximal forearm, and 3 length of arm from shoulder to wrist',
                image: true,
                child: CustomPaint(
                  painter: ArmLandmarkDiagramPainter(),
                  child: const SizedBox.expand(),
                ),
              ),
            ),
            const SizedBox(height: 10),
            const _LandmarkLegendRow(
              number: '1',
              title: 'Circumference of bicep',
              instruction: 'Mid-upper arm (fullest bicep girth).',
            ),
            const SizedBox(height: 6),
            const _LandmarkLegendRow(
              number: '2',
              title: 'Circumference of forearm',
              instruction: 'Widest proximal forearm below elbow.',
            ),
            const SizedBox(height: 6),
            const _LandmarkLegendRow(
              number: '3',
              title: 'Length of arm',
              instruction: 'Shoulder acromion to wrist styloid.',
            ),
          ],
        ),
      ),
    );
  }
}

class _LandmarkLegendRow extends StatelessWidget {
  final String number;
  final String title;
  final String instruction;

  const _LandmarkLegendRow({
    required this.number,
    required this.title,
    required this.instruction,
  });

  @override
  Widget build(BuildContext context) {
    return Row(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Container(
          width: 24,
          height: 24,
          alignment: Alignment.center,
          decoration: const BoxDecoration(
            color: KinetismTheme.accent,
            shape: BoxShape.circle,
          ),
          child: Text(
            number,
            style: const TextStyle(
              color: KinetismTheme.background,
              fontWeight: FontWeight.w800,
              fontSize: 13,
            ),
          ),
        ),
        const SizedBox(width: 12),
        Expanded(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(
                title,
                style: const TextStyle(fontWeight: FontWeight.w600),
              ),
              Text(
                instruction,
                style: const TextStyle(
                  color: KinetismTheme.secondaryText,
                  fontSize: 14,
                ),
              ),
            ],
          ),
        ),
      ],
    );
  }
}

class ArmLandmarkDiagramPainter extends CustomPainter {
  @override
  void paint(Canvas canvas, Size size) {
    final w = size.width;
    final h = size.height;

    final shoulder = Offset(w * 0.16, h * 0.44);
    final elbow = Offset(w * 0.50, h * 0.48);
    final wrist = Offset(w * 0.84, h * 0.44);

    // Soft arm silhouette
    final silhouettePaint = Paint()
      ..color = const Color(0xFF243B47)
      ..style = PaintingStyle.fill;
    final outlinePaint = Paint()
      ..color = const Color(0xFF6B8A99)
      ..strokeWidth = 2.0
      ..style = PaintingStyle.stroke;

    final armPath = Path()
      ..moveTo(shoulder.dx, shoulder.dy - 28)
      ..quadraticBezierTo(
        (shoulder.dx + elbow.dx) / 2,
        shoulder.dy - 34,
        elbow.dx,
        elbow.dy - 22,
      )
      ..quadraticBezierTo(
        (elbow.dx + wrist.dx) / 2,
        elbow.dy - 25,
        wrist.dx,
        wrist.dy - 14,
      )
      ..lineTo(wrist.dx + 18, wrist.dy)
      ..lineTo(wrist.dx, wrist.dy + 14)
      ..quadraticBezierTo(
        (elbow.dx + wrist.dx) / 2,
        elbow.dy + 22,
        elbow.dx,
        elbow.dy + 22,
      )
      ..quadraticBezierTo(
        (shoulder.dx + elbow.dx) / 2,
        shoulder.dy + 30,
        shoulder.dx,
        shoulder.dy + 28,
      )
      ..close();

    canvas.drawPath(armPath, silhouettePaint);
    canvas.drawPath(armPath, outlinePaint);

    // Internal bone axis + joints
    final bonePaint = Paint()
      ..color = const Color(0xFF90A8B4)
      ..strokeWidth = 4
      ..strokeCap = StrokeCap.round;
    canvas.drawLine(shoulder, elbow, bonePaint);
    canvas.drawLine(elbow, wrist, bonePaint);

    final jointPaint = Paint()..color = const Color(0xFFF0F6F8);
    for (final pt in [shoulder, elbow, wrist]) {
      canvas.drawCircle(pt, 5.5, jointPaint);
    }

    // 1. Bicep circumference band (mid-upper arm)
    final bicepX = (shoulder.dx + elbow.dx) * 0.5;
    final bandPaint = Paint()
      ..color = KinetismTheme.accent
      ..strokeWidth = 3.0
      ..style = PaintingStyle.stroke;
    canvas.drawOval(
      Rect.fromCenter(
        center: Offset(bicepX, (shoulder.dy + elbow.dy) * 0.5),
        width: 22,
        height: 62,
      ),
      bandPaint,
    );
    _drawBadge(canvas, Offset(bicepX, shoulder.dy - 54), '1 Bicep');

    // 2. Forearm circumference band (proximal forearm)
    final forearmX = elbow.dx + (wrist.dx - elbow.dx) * 0.38;
    canvas.drawOval(
      Rect.fromCenter(
        center: Offset(forearmX, (elbow.dy + wrist.dy) * 0.5),
        width: 20,
        height: 48,
      ),
      bandPaint,
    );
    _drawBadge(canvas, Offset(forearmX, elbow.dy - 54), '2 Forearm');

    // 3. Total Arm Length arrow (shoulder to wrist)
    final lengthY = h * 0.80;
    final lengthPaint = Paint()
      ..color = const Color(0xFFFFD166)
      ..strokeWidth = 2.5;
    canvas.drawLine(
      Offset(shoulder.dx, lengthY),
      Offset(wrist.dx, lengthY),
      lengthPaint,
    );
    canvas.drawLine(
      Offset(shoulder.dx, shoulder.dy + 30),
      Offset(shoulder.dx, lengthY + 6),
      lengthPaint,
    );
    canvas.drawLine(
      Offset(wrist.dx, wrist.dy + 18),
      Offset(wrist.dx, lengthY + 6),
      lengthPaint,
    );
    _drawBadge(
      canvas,
      Offset((shoulder.dx + wrist.dx) * 0.5, lengthY),
      '3 Length of arm',
      badgeColor: const Color(0xFFFFD166),
    );
  }

  void _drawBadge(
    Canvas canvas,
    Offset center,
    String text, {
    Color badgeColor = KinetismTheme.accent,
  }) {
    final tp = TextPainter(
      text: TextSpan(
        text: text,
        style: const TextStyle(
          color: KinetismTheme.background,
          fontSize: 11,
          fontWeight: FontWeight.w800,
        ),
      ),
      textDirection: TextDirection.ltr,
    )..layout();

    final rect = Rect.fromCenter(
      center: center,
      width: tp.width + 14,
      height: tp.height + 8,
    );
    canvas.drawRRect(
      RRect.fromRectAndRadius(rect, const Radius.circular(12)),
      Paint()..color = badgeColor,
    );
    tp.paint(
      canvas,
      Offset(center.dx - tp.width / 2, center.dy - tp.height / 2),
    );
  }

  @override
  bool shouldRepaint(covariant CustomPainter oldDelegate) => false;
}

/// STEPS 5, 6, 7: Start measuring -> Patient performs movement -> Physiotherapist
/// presses Record when position is reached (Angle, Force, AI MMT Grading) ->
/// Record again button or view Record log / Overall record log.
class _ActiveMeasurementView extends StatelessWidget {
  final KinetismClinicalController controller;

  const _ActiveMeasurementView({required this.controller});

  @override
  Widget build(BuildContext context) {
    final patient = controller.selectedPatient;
    final arm = controller.currentAnthropometrics;
    final liveAngle = controller.liveAngleDegrees;
    final liveForce = controller.liveForceNewtons;
    final liveMmt = controller.liveMmtAssessment;
    final captured = controller.lastCapturedRecord;

    final upperQ = vector.Quaternion.identity();
    final foreQ = vector.Quaternion.axisAngle(
      vector.Vector3(0, 0, 1),
      (180.0 - liveAngle) * (math.pi / 180.0),
    );

    return SingleChildScrollView(
      padding: const EdgeInsets.all(24),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          _StepProgressHeader(
            stepNumber: 6,
          label: 'Live measurement & position capture',
          title: '${controller.selectedBodyPart} measurement',
          subtitle:
              'Patient: ${patient?.name ?? 'Selected patient'} · Ask the patient to perform the movement, then press Record when they reach the target position.',
        ),
        const SizedBox(height: 16),
        if (captured != null) ...[
          Card(
            margin: EdgeInsets.zero,
            child: Padding(
              padding: const EdgeInsets.all(16),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Row(
                    children: [
                      const Icon(
                        Icons.check_circle,
                        color: KinetismTheme.accent,
                      ),
                      const SizedBox(width: 10),
                      Expanded(
                        child: Text(
                          'Recorded attempt #${captured.attemptNumber} saved to record log',
                          style: Theme.of(context).textTheme.titleMedium,
                        ),
                      ),
                    ],
                  ),
                  const SizedBox(height: 10),
                  Wrap(
                    spacing: 24,
                    runSpacing: 8,
                    children: [
                      _liveMetricTile(
                        context,
                        'Recorded angle',
                        '${captured.angleDegrees.toStringAsFixed(1)}°',
                      ),
                      _liveMetricTile(
                        context,
                        'Recorded force',
                        '${captured.forceNewtons.toStringAsFixed(1)} N',
                      ),
                      _liveMetricTile(
                        context,
                        'Recorded AI MMT',
                        captured.mmtBadge,
                      ),
                    ],
                  ),
                  const SizedBox(height: 8),
                  Text(
                    captured.mmtRationale,
                    style: const TextStyle(
                      color: KinetismTheme.secondaryText,
                      fontSize: 13,
                    ),
                  ),
                  const SizedBox(height: 12),
                  Wrap(
                    spacing: 10,
                    runSpacing: 10,
                    children: [
                      OutlinedButton.icon(
                        onPressed: controller.prepareRecordAgain,
                        icon: const Icon(Icons.replay),
                        label: const Text('Record again'),
                      ),
                      FilledButton.icon(
                        onPressed: controller.openPatientRecordLog,
                        icon: const Icon(Icons.assignment_outlined),
                        label: const Text('Open patient record log'),
                      ),
                      OutlinedButton.icon(
                        onPressed: controller.openOverallRecordLog,
                        icon: const Icon(Icons.library_books_outlined),
                        label: const Text('Open overall record log'),
                      ),
                    ],
                  ),
                ],
              ),
            ),
          ),
          const SizedBox(height: 16),
        ],
        Card(
          margin: EdgeInsets.zero,
          child: Padding(
            padding: const EdgeInsets.all(16),
            child: Wrap(
              spacing: 20,
              runSpacing: 8,
              alignment: WrapAlignment.spaceBetween,
              crossAxisAlignment: WrapCrossAlignment.center,
              children: [
                Row(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    const Icon(
                      Icons.sensors,
                      color: KinetismTheme.accent,
                      size: 20,
                    ),
                    const SizedBox(width: 8),
                    Flexible(
                      child: Text(
                        controller.sensorStatus,
                        style: const TextStyle(
                          color: KinetismTheme.secondaryText,
                        ),
                      ),
                    ),
                  ],
                ),
                Text(
                  'Bicep ${arm.bicepCircumferenceCm.toStringAsFixed(1)} cm · '
                  'Forearm ${arm.forearmCircumferenceCm.toStringAsFixed(1)} cm · '
                  'Length ${arm.armLengthCm.toStringAsFixed(1)} cm',
                  style: const TextStyle(
                    color: KinetismTheme.secondaryText,
                    fontSize: 13,
                  ),
                ),
              ],
            ),
          ),
        ),
        const SizedBox(height: 20),
        Card(
          margin: EdgeInsets.zero,
          child: Padding(
            padding: const EdgeInsets.all(20),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                Wrap(
                  spacing: 32,
                  runSpacing: 16,
                  children: [
                    _liveMetricTile(
                      context,
                      'Angle',
                      '${liveAngle.toStringAsFixed(1)}°',
                    ),
                    _liveMetricTile(
                      context,
                      'Force',
                      '${liveForce.toStringAsFixed(1)} N',
                    ),
                    _liveMetricTile(
                      context,
                      'AI MMT grading',
                      liveMmt.badgeLabel,
                    ),
                  ],
                ),
                const SizedBox(height: 10),
                Text(
                  liveMmt.rationale,
                  style: const TextStyle(
                    color: KinetismTheme.secondaryText,
                    fontSize: 14,
                  ),
                ),
                const SizedBox(height: 14),
                Wrap(
                  spacing: 12,
                  runSpacing: 12,
                  children: [
                    FilledButton.icon(
                      onPressed: () async {
                        await controller.recordCurrentPosition();
                        if (context.mounted) {
                          ScaffoldMessenger.of(context).showSnackBar(
                            const SnackBar(
                              content: Text(
                                'Position recorded and saved to the record log.',
                              ),
                            ),
                          );
                        }
                      },
                      icon: const Icon(Icons.fiber_manual_record),
                      label: const Text('Record'),
                    ),
                    if (controller.hasCapturedPosition)
                      FilledButton.tonalIcon(
                        onPressed: controller.prepareRecordAgain,
                        icon: const Icon(Icons.replay),
                        label: const Text('Record again'),
                      ),
                    if (controller.onConnectSensors != null)
                      OutlinedButton.icon(
                        onPressed: controller.onConnectSensors,
                        icon: const Icon(Icons.bluetooth),
                        label: const Text('Connect BLE sensors'),
                      ),
                  ],
                ),
                const SizedBox(height: 14),
                SizedBox(
                  height: 150,
                  child: Semantics(
                    label:
                        'Arm sensor orientation visualizer at ${liveAngle.toStringAsFixed(1)} degrees',
                    image: true,
                    child: CustomPaint(
                      painter: ArmSkeletonPainter(upperQ, foreQ),
                      child: const SizedBox.expand(),
                    ),
                  ),
                ),
                const SizedBox(height: 12),
                Text(
                  'Movement position (${liveAngle.toStringAsFixed(1)}°)',
                  style: const TextStyle(
                    color: KinetismTheme.secondaryText,
                    fontWeight: FontWeight.w600,
                    fontSize: 14,
                  ),
                ),
                Slider(
                  value: liveAngle.clamp(0.0, 160.0),
                  min: 0.0,
                  max: 160.0,
                  divisions: 160,
                  label: '${liveAngle.toStringAsFixed(0)}°',
                  onChanged: (value) {
                    controller.updateLiveSensorPose(angleDegrees: value);
                  },
                ),
                Wrap(
                  spacing: 8,
                  runSpacing: 8,
                  children: [
                    for (final preset in [
                      ('Extension (25°)', 25.0),
                      ('Mid-flexion (85°)', 85.0),
                      ('Target flexion (120°)', 120.0),
                      ('Full flexion (140°)', 140.0),
                    ])
                      ActionChip(
                        label: Text(preset.$1),
                        onPressed: () => controller.updateLiveSensorPose(
                          angleDegrees: preset.$2,
                        ),
                      ),
                  ],
                ),
              ],
            ),
          ),
        ),
      ],
      ),
    );
  }

  Widget _liveMetricTile(BuildContext context, String label, String value) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(
          label,
          style: const TextStyle(color: KinetismTheme.secondaryText),
        ),
        const SizedBox(height: 4),
        Text(
          value,
          style: Theme.of(context).textTheme.titleLarge?.copyWith(
            color: KinetismTheme.accent,
            fontWeight: FontWeight.w700,
          ),
        ),
      ],
    );
  }
}

/// STEPS 8 & 9: Record log (Patient-specific Record log & Overall record log)
class _RecordLogView extends StatefulWidget {
  final KinetismClinicalController controller;
  final bool overall;

  const _RecordLogView({
    required this.controller,
    required this.overall,
  });

  @override
  State<_RecordLogView> createState() => _RecordLogViewState();
}

class _RecordLogViewState extends State<_RecordLogView> {
  final _searchCtrl = TextEditingController();
  String _query = '';

  @override
  void dispose() {
    _searchCtrl.dispose();
    super.dispose();
  }

  void _showRecordDetails(MeasurementRecord record) {
    showDialog<void>(
      context: context,
      builder: (dialogContext) => AlertDialog(
        title: Text('${record.patientName} · ${record.bodyPart}'),
        content: SingleChildScrollView(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            mainAxisSize: MainAxisSize.min,
            children: [
              Text('Patient ID: ${record.patientId}'),
              const SizedBox(height: 6),
              Text('Attempt: #${record.attemptNumber}'),
              const Divider(height: 24),
              Text(
                'Recorded Angle: ${record.angleDegrees.toStringAsFixed(1)}°',
                style: const TextStyle(fontWeight: FontWeight.w700),
              ),
              const SizedBox(height: 6),
              Text(
                'Recorded Force: ${record.forceNewtons.toStringAsFixed(1)} N',
                style: const TextStyle(fontWeight: FontWeight.w700),
              ),
              const SizedBox(height: 6),
              Text(
                'AI MMT Grading: ${record.mmtBadge}',
                style: const TextStyle(
                  color: KinetismTheme.accent,
                  fontWeight: FontWeight.w700,
                ),
              ),
              const SizedBox(height: 8),
              Text(
                record.mmtRationale,
                style: const TextStyle(color: KinetismTheme.secondaryText),
              ),
              const Divider(height: 24),
              Text(
                'Arm dimensions used for force calculation:\n'
                '• Bicep circumference: ${record.bicepCircumferenceCm.toStringAsFixed(1)} cm\n'
                '• Forearm circumference: ${record.forearmCircumferenceCm.toStringAsFixed(1)} cm\n'
                '• Length of arm: ${record.armLengthCm.toStringAsFixed(1)} cm',
                style: const TextStyle(color: KinetismTheme.secondaryText),
              ),
            ],
          ),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(dialogContext),
            child: const Text('Close'),
          ),
        ],
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final selectedPatient = widget.controller.selectedPatient;
    final sourceRecords = (!widget.overall && selectedPatient != null)
        ? widget.controller.recordsForPatient(selectedPatient.id)
        : widget.controller.allRecords;
    final filtered = sourceRecords
        .where((r) => r.matchesQuery(_query))
        .toList();

    final maxForce = filtered.isEmpty
        ? 0.0
        : filtered.map((r) => r.forceNewtons).reduce(math.max);
    final maxAngle = filtered.isEmpty
        ? 0.0
        : filtered.map((r) => r.angleDegrees).reduce(math.max);

    return SingleChildScrollView(
      padding: const EdgeInsets.all(24),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          _StepProgressHeader(
            stepNumber: 8,
          label: widget.overall
              ? 'Overall record log'
              : 'Patient record log',
          title: widget.overall
              ? 'Overall record log'
              : '${selectedPatient?.name ?? 'Patient'} record log',
          subtitle: widget.overall
              ? 'All measured angle, force, and AI MMT grading records across all patients for physiotherapist reference.'
              : 'Saved angle, force, and AI MMT grading measurements for ${selectedPatient?.name ?? 'this patient'}.',
        ),
        const SizedBox(height: 16),
        Wrap(
          spacing: 12,
          runSpacing: 12,
          children: [
            if (selectedPatient != null)
              FilledButton.icon(
                onPressed: widget.controller.openMeasureSection,
                icon: const Icon(Icons.sensors),
                label: const Text('Measure patient'),
              ),
            if (!widget.overall)
              OutlinedButton.icon(
                onPressed: widget.controller.openOverallRecordLog,
                icon: const Icon(Icons.library_books_outlined),
                label: const Text('Switch to overall record log'),
              )
            else if (selectedPatient != null)
              OutlinedButton.icon(
                onPressed: widget.controller.openPatientRecordLog,
                icon: const Icon(Icons.person_outline),
                label: Text('${selectedPatient.name} record log'),
              ),
            TextButton.icon(
              onPressed: () =>
                  widget.controller.goToStep(ClinicalStep.selectPatient),
              icon: const Icon(Icons.people_outline),
              label: const Text('Select patient'),
            ),
          ],
        ),
        const SizedBox(height: 20),
        TextField(
          controller: _searchCtrl,
          onChanged: (v) => setState(() => _query = v),
          decoration: InputDecoration(
            labelText: 'Search record log',
            hintText: 'Patient name, ID, body part, or MMT grade',
            prefixIcon: const Icon(Icons.search),
            suffixIcon: _query.isEmpty
                ? null
                : IconButton(
                    tooltip: 'Clear record search',
                    icon: const Icon(Icons.close),
                    onPressed: () {
                      _searchCtrl.clear();
                      setState(() => _query = '');
                    },
                  ),
          ),
        ),
        const SizedBox(height: 20),
        Wrap(
          spacing: 32,
          runSpacing: 12,
          children: [
            _summaryStat(context, 'Saved records', '${filtered.length}'),
            _summaryStat(
              context,
              'Peak angle',
              filtered.isEmpty ? '--' : '${maxAngle.toStringAsFixed(1)}°',
            ),
            _summaryStat(
              context,
              'Peak force',
              filtered.isEmpty ? '--' : '${maxForce.toStringAsFixed(1)} N',
            ),
          ],
        ),
        const SizedBox(height: 20),
        if (filtered.isEmpty)
          StatusPanel(
            icon: Icons.assignment_late_outlined,
            title: 'No records found',
            message:
                'Start a measurement session to record angle, force, and AI MMT grading.',
            actionLabel: selectedPatient != null ? 'Start measuring' : null,
            onAction: selectedPatient != null
                ? widget.controller.openMeasureSection
                : null,
          )
        else
          ...filtered.map((record) {
            return Padding(
              padding: const EdgeInsets.only(bottom: 12),
              child: Card(
                margin: EdgeInsets.zero,
                child: ListTile(
                  contentPadding: const EdgeInsets.symmetric(
                    horizontal: 20,
                    vertical: 12,
                  ),
                  title: Text(
                    '${record.patientName} · ${record.bodyPart} (Attempt #${record.attemptNumber})',
                    style: Theme.of(context).textTheme.titleMedium,
                  ),
                  subtitle: Padding(
                    padding: const EdgeInsets.only(top: 8),
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Wrap(
                          spacing: 16,
                          runSpacing: 6,
                          children: [
                            Text(
                              'Angle: ${record.angleDegrees.toStringAsFixed(1)}°',
                              style: const TextStyle(
                                color: KinetismTheme.accent,
                                fontWeight: FontWeight.w700,
                              ),
                            ),
                            Text(
                              'Force: ${record.forceNewtons.toStringAsFixed(1)} N',
                              style: const TextStyle(
                                color: KinetismTheme.accent,
                                fontWeight: FontWeight.w700,
                              ),
                            ),
                            Text(
                              'AI MMT: ${record.mmtBadge}',
                              style: const TextStyle(
                                fontWeight: FontWeight.w600,
                              ),
                            ),
                          ],
                        ),
                        const SizedBox(height: 4),
                        Text(
                          'Bicep ${record.bicepCircumferenceCm.toStringAsFixed(1)} cm · '
                          'Forearm ${record.forearmCircumferenceCm.toStringAsFixed(1)} cm · '
                          'Arm length ${record.armLengthCm.toStringAsFixed(1)} cm',
                          style: const TextStyle(
                            color: KinetismTheme.secondaryText,
                            fontSize: 13,
                          ),
                        ),
                      ],
                    ),
                  ),
                  trailing: const Icon(Icons.chevron_right),
                  onTap: () => _showRecordDetails(record),
                ),
              ),
            );
          }),
      ],
      ),
    );
  }

  Widget _summaryStat(BuildContext context, String label, String value) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(
          label,
          style: const TextStyle(color: KinetismTheme.secondaryText),
        ),
        const SizedBox(height: 4),
        Text(value, style: Theme.of(context).textTheme.headlineMedium),
      ],
    );
  }
}
