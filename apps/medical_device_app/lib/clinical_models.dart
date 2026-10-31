import 'dart:math' as math;

class ArmAnthropometrics {
  final double bicepCircumferenceCm;
  final double forearmCircumferenceCm;
  final double armLengthCm;

  const ArmAnthropometrics({
    required this.bicepCircumferenceCm,
    required this.forearmCircumferenceCm,
    required this.armLengthCm,
  });

  static const double standardGravity = 9.80665;
  static const double tissueDensityKgPerM3 = 1050.0;

  static String? validateBicepCircumference(String? value) {
    final parsed = _parsePositive(value);
    if (parsed == null) {
      return 'Enter a valid bicep circumference in cm.';
    }
    if (parsed < 10 || parsed > 65) {
      return 'Enter a bicep circumference between 10 and 65 cm.';
    }
    return null;
  }

  static String? validateForearmCircumference(String? value) {
    final parsed = _parsePositive(value);
    if (parsed == null) {
      return 'Enter a valid forearm circumference in cm.';
    }
    if (parsed < 10 || parsed > 55) {
      return 'Enter a forearm circumference between 10 and 55 cm.';
    }
    return null;
  }

  static String? validateArmLength(String? value) {
    final parsed = _parsePositive(value);
    if (parsed == null) {
      return 'Enter a valid arm length in cm.';
    }
    if (parsed < 15 || parsed > 95) {
      return 'Enter an arm length between 15 and 95 cm.';
    }
    return null;
  }

  static double? _parsePositive(String? value) {
    if (value == null) return null;
    final parsed = double.tryParse(value.trim());
    if (parsed == null || !parsed.isFinite || parsed <= 0) return null;
    return parsed;
  }

  /// Cross-sectional area of the mid-upper arm (bicep) in cm^2.
  double get bicepAreaCm2 =>
      (bicepCircumferenceCm * bicepCircumferenceCm) / (4 * math.pi);

  /// Cross-sectional area of the maximum forearm in cm^2.
  double get forearmAreaCm2 =>
      (forearmCircumferenceCm * forearmCircumferenceCm) / (4 * math.pi);

  /// Frustum volume of the measured arm in cm^3.
  double get armVolumeCm3 =>
      (armLengthCm / 3.0) *
      (bicepAreaCm2 +
          forearmAreaCm2 +
          math.sqrt(bicepAreaCm2 * forearmAreaCm2));

  /// Total estimated arm mass in kg using soft-tissue density (1.05 g/cm^3).
  double get totalArmMassKg => (armVolumeCm3 * 1.05) / 1000.0;

  /// Effective moving forearm-and-hand segment mass in kg used for force math.
  double get effectiveMovingMassKg {
    final areaSum = bicepAreaCm2 + forearmAreaCm2;
    if (areaSum <= 0) return 1.2;
    final distalShare = (0.45 + 0.35 * (forearmAreaCm2 / areaSum)).clamp(
      0.40,
      0.75,
    );
    return (totalArmMassKg * distalShare).clamp(0.25, 8.0);
  }

  /// Effective moment arm in meters (center of mass distance from elbow axis).
  double get momentArmMeters => (armLengthCm * 0.01) * 0.43;

  /// Calculates the biomechanical force (in Newtons) at the target position.
  ///
  /// Uses the patient's bicep circumference, forearm circumference, and arm
  /// length to determine segment mass and moment arm, combined with gravitational
  /// load at [angleDegrees] and dynamic movement intensity [accelMagnitudeG].
  double calculateForceNewtons({
    required double angleDegrees,
    double accelMagnitudeG = 0.18,
  }) {
    final clampedAngle = angleDegrees.isFinite
        ? angleDegrees.clamp(0.0, 180.0)
        : 0.0;
    final cleanAccelG = accelMagnitudeG.isFinite && accelMagnitudeG >= 0
        ? accelMagnitudeG
        : 0.0;

    final massKg = effectiveMovingMassKg;
    final radians = (clampedAngle.clamp(12.0, 160.0)) * (math.pi / 180.0);
    final postureFactor = 0.55 + 0.45 * math.sin(radians).abs();
    final flexionLoadFactor = 1.0 + 0.38 * (clampedAngle / 145.0);
    final leverScale = (armLengthCm / 55.0).clamp(0.55, 1.65);
    final bicepLeverage = (bicepCircumferenceCm / 28.0).clamp(0.65, 1.60);

    final gravityComponentN =
        massKg *
        standardGravity *
        postureFactor *
        flexionLoadFactor *
        leverScale;
    final dynamicComponentN =
        massKg * cleanAccelG * standardGravity * leverScale * bicepLeverage;

    final totalForceN = gravityComponentN + dynamicComponentN;
    return double.parse(totalForceN.toStringAsFixed(1));
  }

  Map<String, dynamic> toMap() => {
    'bicepCircumferenceCm': bicepCircumferenceCm,
    'forearmCircumferenceCm': forearmCircumferenceCm,
    'armLengthCm': armLengthCm,
  };

  static ArmAnthropometrics? fromMap(Map<String, dynamic>? data) {
    if (data == null) return null;
    final bicep = data['bicepCircumferenceCm'];
    final forearm = data['forearmCircumferenceCm'];
    final length = data['armLengthCm'];
    if (bicep is num &&
        bicep.isFinite &&
        bicep > 0 &&
        forearm is num &&
        forearm.isFinite &&
        forearm > 0 &&
        length is num &&
        length.isFinite &&
        length > 0) {
      return ArmAnthropometrics(
        bicepCircumferenceCm: bicep.toDouble(),
        forearmCircumferenceCm: forearm.toDouble(),
        armLengthCm: length.toDouble(),
      );
    }
    return null;
  }
}

class MmtAssessment {
  final int grade;
  final String title;
  final String rationale;

  const MmtAssessment({
    required this.grade,
    required this.title,
    required this.rationale,
  });

  String get badgeLabel => 'Grade $grade / 5 ($title)';

  /// AI-assisted Manual Muscle Testing (MMT) grading based on recorded
  /// joint range of motion [angleDegrees], calculated [forceNewtons], and
  /// patient [anthropometrics].
  factory MmtAssessment.evaluate({
    required double angleDegrees,
    required double forceNewtons,
    required ArmAnthropometrics anthropometrics,
  }) {
    final angle = angleDegrees.isFinite ? angleDegrees.clamp(0.0, 180.0) : 0.0;
    final force = forceNewtons.isFinite && forceNewtons >= 0
        ? forceNewtons
        : 0.0;
    final baselineGravityN =
        anthropometrics.effectiveMovingMassKg *
        ArmAnthropometrics.standardGravity;
    final forceRatio = baselineGravityN > 0 ? force / baselineGravityN : 0.0;

    if (angle < 10.0 && force < 2.0) {
      return const MmtAssessment(
        grade: 0,
        title: 'Zero',
        rationale:
            'No measurable joint excursion or active force output detected.',
      );
    }
    if (angle < 35.0 || forceRatio < 0.45) {
      return MmtAssessment(
        grade: 1,
        title: 'Trace',
        rationale:
            'Trace muscle activation (${angle.toStringAsFixed(1)}°, ${force.toStringAsFixed(1)} N) without functional excursion.',
      );
    }
    if (angle < 75.0 || forceRatio < 0.80) {
      return MmtAssessment(
        grade: 2,
        title: 'Poor',
        rationale:
            'Partial range of motion (${angle.toStringAsFixed(1)}°) with gravity-assisted force (${force.toStringAsFixed(1)} N).',
      );
    }
    if (angle < 105.0 || forceRatio < 1.12) {
      return MmtAssessment(
        grade: 3,
        title: 'Fair',
        rationale:
            'Holds target position against gravity (${angle.toStringAsFixed(1)}°, ${force.toStringAsFixed(1)} N) without added resistance.',
      );
    }
    if (angle < 128.0 || forceRatio < 1.38) {
      return MmtAssessment(
        grade: 4,
        title: 'Good',
        rationale:
            'Full functional range (${angle.toStringAsFixed(1)}°) with moderate force capacity (${force.toStringAsFixed(1)} N).',
      );
    }
    return MmtAssessment(
      grade: 5,
      title: 'Normal',
      rationale:
          'Full range of motion (${angle.toStringAsFixed(1)}°) with strong peak force output (${force.toStringAsFixed(1)} N).',
    );
  }
}

class PatientProfile {
  final String id;
  final String name;
  final String code;
  final ArmAnthropometrics? armMeasurements;
  final DateTime? updatedAt;

  const PatientProfile({
    required this.id,
    required this.name,
    required this.code,
    this.armMeasurements,
    this.updatedAt,
  });

  PatientProfile copyWith({
    String? name,
    String? code,
    ArmAnthropometrics? armMeasurements,
    DateTime? updatedAt,
  }) {
    return PatientProfile(
      id: id,
      name: name ?? this.name,
      code: code ?? this.code,
      armMeasurements: armMeasurements ?? this.armMeasurements,
      updatedAt: updatedAt ?? this.updatedAt,
    );
  }

  Map<String, dynamic> toMap() => {
    'name': name,
    'code': code,
    if (armMeasurements != null) ...armMeasurements!.toMap(),
    if (updatedAt != null) 'updatedAt': updatedAt,
  };

  factory PatientProfile.fromMap(String id, Map<String, dynamic> data) {
    final rawName = data['name'];
    final rawCode = data['code'];
    final rawDate = data['updatedAt'];
    return PatientProfile(
      id: id,
      name: rawName is String && rawName.trim().isNotEmpty
          ? rawName.trim()
          : 'Unnamed patient',
      code: rawCode is String && rawCode.trim().isNotEmpty
          ? rawCode.trim()
          : id.toUpperCase(),
      armMeasurements: ArmAnthropometrics.fromMap(data),
      updatedAt: rawDate is DateTime ? rawDate.toLocal() : null,
    );
  }
}

class MeasurementRecord {
  final String id;
  final String patientId;
  final String patientName;
  final String bodyPart;
  final double bicepCircumferenceCm;
  final double forearmCircumferenceCm;
  final double armLengthCm;
  final double angleDegrees;
  final double forceNewtons;
  final int mmtGrade;
  final String mmtTitle;
  final String mmtRationale;
  final int attemptNumber;
  final DateTime? recordedAt;

  const MeasurementRecord({
    required this.id,
    required this.patientId,
    required this.patientName,
    required this.bodyPart,
    required this.bicepCircumferenceCm,
    required this.forearmCircumferenceCm,
    required this.armLengthCm,
    required this.angleDegrees,
    required this.forceNewtons,
    required this.mmtGrade,
    required this.mmtTitle,
    required this.mmtRationale,
    required this.attemptNumber,
    required this.recordedAt,
  });

  String get mmtBadge => 'Grade $mmtGrade / 5 ($mmtTitle)';

  ArmAnthropometrics get anthropometrics => ArmAnthropometrics(
    bicepCircumferenceCm: bicepCircumferenceCm,
    forearmCircumferenceCm: forearmCircumferenceCm,
    armLengthCm: armLengthCm,
  );

  bool matchesQuery(String query) {
    final q = query.trim().toLowerCase();
    if (q.isEmpty) return true;
    return patientName.toLowerCase().contains(q) ||
        patientId.toLowerCase().contains(q) ||
        bodyPart.toLowerCase().contains(q) ||
        mmtBadge.toLowerCase().contains(q);
  }

  Map<String, dynamic> toMap() => {
    'patientId': patientId,
    'patientName': patientName,
    'bodyPart': bodyPart,
    'bicepCircumferenceCm': bicepCircumferenceCm,
    'forearmCircumferenceCm': forearmCircumferenceCm,
    'armLengthCm': armLengthCm,
    'angleDegrees': angleDegrees,
    'forceNewtons': forceNewtons,
    'mmtGrade': mmtGrade,
    'mmtTitle': mmtTitle,
    'mmtRationale': mmtRationale,
    'attemptNumber': attemptNumber,
    'date': recordedAt,
  };

  factory MeasurementRecord.fromMap(String id, Map<String, dynamic> data) {
    final patientId = data['patientId'] is String &&
            (data['patientId'] as String).trim().isNotEmpty
        ? (data['patientId'] as String).trim()
        : 'unknown-patient';
    final patientName = data['patientName'] is String &&
            (data['patientName'] as String).trim().isNotEmpty
        ? (data['patientName'] as String).trim()
        : patientId;
    final bodyPart = data['bodyPart'] is String &&
            (data['bodyPart'] as String).trim().isNotEmpty
        ? (data['bodyPart'] as String).trim()
        : 'Arm';
    final bicep = (data['bicepCircumferenceCm'] is num &&
            (data['bicepCircumferenceCm'] as num).isFinite)
        ? (data['bicepCircumferenceCm'] as num).toDouble()
        : 28.0;
    final forearm = (data['forearmCircumferenceCm'] is num &&
            (data['forearmCircumferenceCm'] as num).isFinite)
        ? (data['forearmCircumferenceCm'] as num).toDouble()
        : 24.0;
    final length =
        (data['armLengthCm'] is num && (data['armLengthCm'] as num).isFinite)
            ? (data['armLengthCm'] as num).toDouble()
            : 54.0;
    final angle =
        (data['angleDegrees'] is num && (data['angleDegrees'] as num).isFinite)
            ? (data['angleDegrees'] as num).toDouble()
            : 90.0;
    final anthropometrics = ArmAnthropometrics(
      bicepCircumferenceCm: bicep,
      forearmCircumferenceCm: forearm,
      armLengthCm: length,
    );
    final force =
        (data['forceNewtons'] is num && (data['forceNewtons'] as num).isFinite)
            ? (data['forceNewtons'] as num).toDouble()
            : anthropometrics.calculateForceNewtons(angleDegrees: angle);
    final fallbackMmt = MmtAssessment.evaluate(
      angleDegrees: angle,
      forceNewtons: force,
      anthropometrics: anthropometrics,
    );
    final grade =
        (data['mmtGrade'] is num && (data['mmtGrade'] as num).isFinite)
            ? (data['mmtGrade'] as num).toInt().clamp(0, 5)
            : fallbackMmt.grade;
    final title = data['mmtTitle'] is String &&
            (data['mmtTitle'] as String).trim().isNotEmpty
        ? (data['mmtTitle'] as String).trim()
        : fallbackMmt.title;
    final rationale = data['mmtRationale'] is String &&
            (data['mmtRationale'] as String).trim().isNotEmpty
        ? (data['mmtRationale'] as String).trim()
        : fallbackMmt.rationale;
    final attempt = (data['attemptNumber'] is num &&
            (data['attemptNumber'] as num).isFinite &&
            (data['attemptNumber'] as num) >= 1)
        ? (data['attemptNumber'] as num).toInt()
        : 1;
    final rawDate = data['date'];
    return MeasurementRecord(
      id: id,
      patientId: patientId,
      patientName: patientName,
      bodyPart: bodyPart,
      bicepCircumferenceCm: bicep,
      forearmCircumferenceCm: forearm,
      armLengthCm: length,
      angleDegrees: angle,
      forceNewtons: force,
      mmtGrade: grade,
      mmtTitle: title,
      mmtRationale: rationale,
      attemptNumber: attempt,
      recordedAt: rawDate is DateTime ? rawDate.toLocal() : null,
    );
  }
}
