import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:medical_device_app/app_design.dart';
import 'package:medical_device_app/exercise_validation.dart';
import 'package:medical_device_app/past_reports.dart';
import 'package:medical_device_app/prescriptions.dart';
import 'package:medical_device_app/repetition_cycle.dart';
import 'package:medical_device_app/sensor_frame.dart';
import 'package:medical_device_app/session_log.dart';

void main() {
  test('Theme preserves text contrast, readable sizes and touch targets', () {
    final theme = KinetismTheme.dark;
    double contrast(Color foreground, Color background) {
      final first = foreground.computeLuminance();
      final second = background.computeLuminance();
      return first > second
          ? (first + 0.05) / (second + 0.05)
          : (second + 0.05) / (first + 0.05);
    }

    for (final background in [
      KinetismTheme.background,
      KinetismTheme.surface,
    ]) {
      expect(
        contrast(theme.colorScheme.onSurface, background),
        greaterThanOrEqualTo(4.5),
      );
      expect(
        contrast(KinetismTheme.secondaryText, background),
        greaterThanOrEqualTo(4.5),
      );
    }
    expect(
      contrast(theme.colorScheme.onPrimary, theme.colorScheme.primary),
      greaterThanOrEqualTo(4.5),
    );
    expect(theme.textTheme.bodyMedium?.fontSize, greaterThanOrEqualTo(16));
    expect(theme.textTheme.bodyLarge?.fontSize, greaterThanOrEqualTo(16));
    expect(theme.textTheme.bodySmall?.fontSize, greaterThanOrEqualTo(12));
    for (final style in [
      theme.filledButtonTheme.style,
      theme.outlinedButtonTheme.style,
      theme.iconButtonTheme.style,
    ]) {
      final size = style?.minimumSize?.resolve(const <WidgetState>{});
      expect(size, isNotNull);
      expect(size!.width, greaterThanOrEqualTo(48));
      expect(size.height, greaterThanOrEqualTo(48));
    }
  });

  test('Missing records do not become zero repetitions or today', () {
    final log = SessionLog.fromMap('missing', {});
    expect(log.repetitions, isNull);
    expect(log.recordedAt, isNull);
    expect(log.patientId, isNull);
    expect(log.issues, hasLength(3));
    final zero = SessionLog.fromMap('zero', {'repsCompleted': 0});
    expect(zero.repetitions, 0);
  });

  for (final value in [null, '12', -1, 1.5, double.nan, double.infinity]) {
    test('Invalid repetition count $value remains unavailable', () {
      expect(
        SessionLog.fromMap('invalid', {'repsCompleted': value}).repetitions,
        isNull,
      );
    });
  }

  test('Activity counts actual same-local-day records and missing fields', () {
    final now = DateTime(2026, 10, 31, 12);
    final logs = [
      SessionLog.fromMap('one', {
        'date': DateTime(2026, 10, 31),
        'repsCompleted': 12,
      }),
      SessionLog.fromMap('zero', {'date': now, 'repsCompleted': 0}),
      SessionLog.fromMap('partial', {'date': now}),
      SessionLog.fromMap('yesterday', {
        'date': DateTime(2026, 10, 30, 23, 59),
        'repsCompleted': 99,
      }),
      SessionLog.fromMap('undated', {'repsCompleted': 100}),
    ];
    final activity = SessionActivity.fromLogs(logs, now);
    expect(activity.sessionsToday, 3);
    expect(activity.recordedRepetitionsToday, 12);
    expect(activity.missingCountsToday, 1);
    expect(activity.missingDates, 1);
  });

  test('Search matches exercise or stored ID, case-insensitively', () {
    final log = SessionLog.fromMap('search', {
      'exerciseName': 'Elbow flexion',
      'patientId': 'Synthetic-ID-001',
    });

    expect(log.matches('  ELBOW '), isTrue);
    expect(log.matches('id-001'), isTrue);
    expect(log.matches('wrist'), isFalse);
  });

  test('UTC server dates are presented in the same local day', () {
    final local = DateTime(2026, 10, 31, 0, 15);
    final log = SessionLog.fromMap('utc', {
      'date': local.toUtc(),
      'repsCompleted': 3,
    });
    expect(log.recordedAt, local);
    expect(SessionActivity.fromLogs([log], local).sessionsToday, 1);
  });

  test('Archive never fabricates a date or missing analysis', () {
    final report = ArchivedReport.fromMap({});
    expect(report.date, isNull);
    expect(report.analysis, isNull);
    expect(ArchivedReport.fromMap({'aiAnalysis': ' '}).analysis, isNull);
  });

  test('Missing, fractional or invalid goals are not presented as zero', () {
    for (final goal in [null, 0, -1, 1.5, '10', double.nan]) {
      expect(ExerciseGoal.fromMap('goal', {'dailyGoal': goal}).target, isNull);
    }
    expect(ExerciseGoal.fromMap('goal', {'dailyGoal': 10}).target, 10);
  });

  test('Recipe validation rejects empty text and non-finite angles', () {
    expect(ExerciseValidation.requiredText('  '), isNotNull);
    for (final angle in ['NaN', 'Infinity', '-1', '181', 'word', '']) {
      expect(ExerciseValidation.angle(angle), isNotNull);
    }
    expect(ExerciseValidation.angle('0'), isNull);
    expect(ExerciseValidation.angle('180'), isNull);
    expect(ExerciseValidation.peakAngle('160', '160'), isNotNull);
    expect(ExerciseValidation.peakAngle('170', '160'), isNotNull);
    expect(ExerciseValidation.peakAngle('60', '160'), isNull);
  });

  test('Goals must be positive integers and video URLs HTTP(S)', () {
    for (final goal in ['0', '-1', '1.5', 'NaN', '']) {
      expect(ExerciseValidation.dailyGoal(goal), isNotNull);
    }
    expect(ExerciseValidation.dailyGoal(' 10 '), isNull);
    for (final url in [
      'video.mp4',
      'file:///video.mp4',
      'ftp://example.test/video',
      'https://',
      'https://user:secret@example.test/video',
    ]) {
      expect(ExerciseValidation.videoUrl(url), isNotNull);
    }
    expect(
      ExerciseValidation.videoUrl('https://example.test/video.mp4'),
      isNull,
    );
  });

  test('Legacy x,y,z,w paired quaternion packets are normalized', () {
    final frame = ArmSensorFrame.parse(utf8.encode('0,0,0,2,0,0,2,2\r\n'));
    expect(frame.upper.w, 1);
    expect(frame.forearm.z, closeTo(0.70710678, 1e-7));
    expect(frame.forearm.w, closeTo(0.70710678, 1e-7));
    expect(frame.upper.length2, closeTo(1, 1e-12));
  });

  for (final packet in [
    '',
    '0,0,0,1',
    '0,0,0,1,0,0,0,1,extra',
    'NaN,0,0,1,0,0,0,1',
    'Infinity,0,0,1,0,0,0,1',
    'x,0,0,1,0,0,0,1',
    '0,0,0,0,0,0,0,1',
  ]) {
    test('Malformed packet "$packet" is explicitly rejected', () {
      expect(
        () => ArmSensorFrame.parse(utf8.encode(packet)),
        throwsFormatException,
      );
    });
  }
  test('Malformed UTF-8 is explicitly rejected', () {
    expect(() => ArmSensorFrame.parse([0xff]), throwsFormatException);
  });

  test(
    'Complete peak-to-rest cycles count once, including exact boundaries',
    () {
      var atPeak = false;
      var count = 0;
      for (final angle in [
        180.0,
        90.0,
        0.0,
        0.0,
        179.9,
        180.0,
        180.0,
        0.0,
        180.0,
      ]) {
        final step = advanceRepetitionCycle(
          angle: angle,
          restingAngle: 180,
          peakAngle: 0,
          atPeak: atPeak,
        );
        atPeak = step.atPeak;
        if (step.counted) count++;
      }
      expect(count, 2);
      expect(atPeak, isFalse);
    },
  );

  test('Stopping just short of the peak does not count a repetition', () {
    final step = advanceRepetitionCycle(
      angle: 60.01,
      restingAngle: 160,
      peakAngle: 60,
      atPeak: false,
    );
    expect(step.atPeak, isFalse);
    expect(
      advanceRepetitionCycle(
        angle: 160,
        restingAngle: 160,
        peakAngle: 60,
        atPeak: step.atPeak,
      ).counted,
      isFalse,
    );
  });

  test('Cycle rejects invalid angles instead of counting them', () {
    for (final angle in [double.nan, double.infinity, -1.0, 181.0]) {
      expect(
        () => advanceRepetitionCycle(
          angle: angle,
          restingAngle: 160,
          peakAngle: 60,
          atPeak: true,
        ),
        throwsArgumentError,
      );
    }
  });
}
