import 'dart:math' as math;
import 'package:vector_math/vector_math_64.dart' as vector;

/// Fault name constants matching Python quality_judge.py
class RepFault {
  static const String insufficientRom = 'insufficient_rom';
  static const String shoulderSwing   = 'shoulder_swing';
  static const String compensation    = 'compensation';
  static const String tremor          = 'tremor';
  static const String speedCheating   = 'speed_cheating';
  static const String unsmooth        = 'unsmooth';
}

/// Human-readable fault messages (mirrors Python _RULES feedback strings)
const Map<String, String> repFaultMessages = {
  RepFault.insufficientRom: 'Range of motion too limited — try to straighten and bend your arm fully.',
  RepFault.shoulderSwing:   'Shoulder is swinging — keep your upper arm still and close to your body.',
  RepFault.compensation:    'Upper arm is compensating — isolate the movement to your elbow.',
  RepFault.tremor:          'Movement is unsteady — try to move slowly and smoothly.',
  RepFault.speedCheating:   'Rep is too fast — slow down for better muscle engagement.',
  RepFault.unsmooth:        'Movement is jerky — aim for a smooth, controlled arc.',
};

class RepJudgeResult {
  final bool passed;
  final double qualityScore;          // 0.0 – 1.0  (1 = perfect)
  final List<String> faults;
  final List<String> feedbackMessages;

  const RepJudgeResult({
    required this.passed,
    required this.qualityScore,
    required this.faults,
    required this.feedbackMessages,
  });
}

/// Flutter adaptation of Python quality_judge.py
///
/// Two-layer system:
///   Layer 1 — Rule engine: 6 fault rules mapped from Python _RULES
///   Layer 2 — Composite quality score (weighted, matching Python weights)
///
/// Uses elbow angle time-series + upper-arm quaternion stream from dual IMU.
class RepQualityJudge {
  final double targetRom;         // expected ROM in degrees (restingAngle − peakAngle)
  final double minRepDurationMs;  // speed-cheat floor (mirrors Python min_duration_s)

  // Per-rep sample buffers
  final List<double> _angles = [];
  final List<int>    _times  = [];
  final List<double> _compAngles = [];  // upper-arm frame-to-frame rotation (°)

  vector.Quaternion? _prevQUpper;

  RepQualityJudge({
    required this.targetRom,
    this.minRepDurationMs = 800.0,
  });

  /// Call every IMU frame during an active rep.
  void addSample(double elbowAngle, vector.Quaternion qUpper) {
    _angles.add(elbowAngle);
    _times.add(DateTime.now().millisecondsSinceEpoch);

    // Compute upper-arm geodesic rotation between frames (compensation proxy)
    if (_prevQUpper != null) {
      final qPrev = _prevQUpper!.normalized();
      final qCurr = qUpper.normalized();
      // q_diff = q_prev_inverse * q_curr
      final qInv = vector.Quaternion(-qPrev.x, -qPrev.y, -qPrev.z, qPrev.w);
      final qDiff = qInv * qCurr;
      final wClamped = qDiff.w.abs().clamp(0.0, 1.0);
      final angleDeg = 2.0 * math.acos(wClamped) * (180.0 / math.pi);
      _compAngles.add(angleDeg);
    }
    _prevQUpper = qUpper.clone();
  }

  /// Reset buffers for the next rep.
  void reset() {
    _angles.clear();
    _times.clear();
    _compAngles.clear();
    _prevQUpper = null;
  }

  /// Judge the buffered rep. Call once per rep at completion.
  RepJudgeResult judge() {
    if (_angles.length < 5) {
      return const RepJudgeResult(
        passed: true, qualityScore: 1.0, faults: [], feedbackMessages: [],
      );
    }

    // ── Feature extraction ──────────────────────────────────────────────────
    final double maxAngle = _angles.reduce(math.max);
    final double minAngle = _angles.reduce(math.min);
    final double rom = maxAngle - minAngle;

    final double durationMs = _times.length > 1
        ? (_times.last - _times.first).toDouble()
        : 0.0;

    // Frame-to-frame angle deltas (velocity proxy, °/frame)
    final List<double> deltas = [];
    for (int i = 1; i < _angles.length; i++) {
      deltas.add((_angles[i] - _angles[i - 1]).abs());
    }
    final double meanDelta = deltas.isEmpty
        ? 0.0
        : deltas.reduce((a, b) => a + b) / deltas.length;

    // Jerk: rate of change of velocity (°/frame²)
    final List<double> jerkVals = [];
    for (int i = 1; i < deltas.length; i++) {
      jerkVals.add((deltas[i] - deltas[i - 1]).abs());
    }
    final double meanJerk = jerkVals.isEmpty
        ? 0.0
        : jerkVals.reduce((a, b) => a + b) / jerkVals.length;

    // Upper-arm compensation: mean rotation per frame (°)
    final double compMean = _compAngles.isEmpty
        ? 0.0
        : _compAngles.reduce((a, b) => a + b) / _compAngles.length;

    // ── Rule engine (mirrors Python _RULES) ────────────────────────────────
    final List<String> faults = [];

    // 1. insufficient_rom: ROM < 90% of target
    if (rom < targetRom * 0.90) {
      faults.add(RepFault.insufficientRom);
    }

    // 2. shoulder_swing: upper arm drifts > 2°/frame on average
    if (compMean > 2.0) {
      faults.add(RepFault.shoulderSwing);
    }

    // 3. compensation: heavier upper-arm drift > 3.5°/frame
    if (compMean > 3.5) {
      faults.add(RepFault.compensation);
    }

    // 4. tremor: high jerk with small per-frame motion (shaky, not fast)
    if (meanJerk > 1.5 && meanDelta < 8.0) {
      faults.add(RepFault.tremor);
    }

    // 5. speed_cheating: rep completed too fast
    if (durationMs > 0 && durationMs < minRepDurationMs) {
      faults.add(RepFault.speedCheating);
    }

    // 6. unsmooth: high jerk regardless of speed
    if (meanJerk > 3.0) {
      faults.add(RepFault.unsmooth);
    }

    // ── Composite quality score (mirrors Python weights) ───────────────────
    // ROM score       35%  (higher_is_better)
    final double romScore = (rom / targetRom).clamp(0.0, 1.0);
    // Smoothness      25%  (inverse jerk, lower_is_better)
    final double smoothScore = (1.0 - (meanJerk / 5.0)).clamp(0.0, 1.0);
    // Compensation    15%  (inverse comp, lower_is_better)
    final double compScore = (1.0 - (compMean / 5.0)).clamp(0.0, 1.0);
    // Speed           15%  (penalise too-fast)
    final double speedScore = durationMs > 0
        ? (durationMs / minRepDurationMs).clamp(0.0, 1.0)
        : 1.0;
    // Stability       10%  (inverse tremor)
    final double stabilityScore = (1.0 - (meanJerk / 3.0)).clamp(0.0, 1.0);

    final double quality = ((romScore      * 0.35) +
                            (smoothScore   * 0.25) +
                            (compScore     * 0.15) +
                            (speedScore    * 0.15) +
                            (stabilityScore * 0.10))
        .clamp(0.0, 1.0);

    final List<String> messages =
        faults.map((f) => repFaultMessages[f] ?? 'Check your form.').toList();

    return RepJudgeResult(
      passed: faults.isEmpty,
      qualityScore: quality,
      faults: faults,
      feedbackMessages: messages,
    );
  }
}
