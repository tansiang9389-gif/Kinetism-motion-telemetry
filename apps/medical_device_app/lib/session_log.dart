class SessionLog {
  final String id;
  final String exerciseName;
  final String? patientId;
  final int? repetitions;
  final DateTime? recordedAt;
  final List<String> issues;

  const SessionLog({
    required this.id,
    required this.exerciseName,
    required this.repetitions,
    required this.recordedAt,
    this.patientId,
    this.issues = const [],
  });

  factory SessionLog.fromMap(String id, Map<String, dynamic> data) {
    final issues = <String>[];
    final name = data['exerciseName'];
    final exerciseName = name is String && name.trim().isNotEmpty
        ? name.trim()
        : 'Unnamed exercise';
    if (exerciseName == 'Unnamed exercise' &&
        !(name is String && name.trim().isNotEmpty)) {
      issues.add('Exercise name is unavailable.');
    }
    final count = data['repsCompleted'];
    final validCount =
        count is num &&
        count.isFinite &&
        count >= 0 &&
        count == count.truncateToDouble();
    if (!validCount) issues.add('Repetition count is unavailable.');
    final date = data['date'];
    if (date is! DateTime) issues.add('Session date is unavailable.');
    final patient = data['patientId'];
    return SessionLog(
      id: id,
      exerciseName: exerciseName,
      patientId: patient is String && patient.trim().isNotEmpty
          ? patient
          : null,
      repetitions: validCount ? count.toInt() : null,
      recordedAt: date is DateTime ? date.toLocal() : null,
      issues: List.unmodifiable(issues),
    );
  }

  bool matches(String query) {
    final normalized = query.trim().toLowerCase();
    return exerciseName.toLowerCase().contains(normalized) ||
        (patientId?.toLowerCase().contains(normalized) ?? false);
  }
}

class SessionActivity {
  final int sessionsToday;
  final int recordedRepetitionsToday;
  final int missingCountsToday;
  final int missingDates;

  const SessionActivity({
    required this.sessionsToday,
    required this.recordedRepetitionsToday,
    required this.missingCountsToday,
    required this.missingDates,
  });

  factory SessionActivity.fromLogs(List<SessionLog> logs, DateTime now) {
    var sessions = 0;
    var repetitions = 0;
    var missingCounts = 0;
    var missingDates = 0;
    final today = now.toLocal();
    for (final log in logs) {
      final date = log.recordedAt?.toLocal();
      if (date == null) {
        missingDates++;
      } else if (date.year == today.year &&
          date.month == today.month &&
          date.day == today.day) {
        sessions++;
        if (log.repetitions == null) {
          missingCounts++;
        } else {
          repetitions += log.repetitions!;
        }
      }
    }
    return SessionActivity(
      sessionsToday: sessions,
      recordedRepetitionsToday: repetitions,
      missingCountsToday: missingCounts,
      missingDates: missingDates,
    );
  }
}
