// rep_classifier.dart
// ===================
// On-device rep-fault classifier. Pure Dart — no Python, no server.
//
// HOW IT FITS YOUR APP:
//   During a rep, you already buffer samples. Each sample has:
//     - timestamp (ms)
//     - upper-arm quaternion  (qx1,qy1,qz1,qw1)
//     - forearm quaternion    (qx2,qy2,qz2,qw2)
//   When the rep ends, build a List<RepSample>, call RepClassifier.classify(samples).
//   It returns the predicted fault label (one of the 7 classes).
//
// CRITICAL: The 8 features below are computed to MATCH train_pipeline.py exactly.
//   If you change a formula here, you must change it there too, then retrain.
//
// TO UPDATE THE MODEL after retraining in Python:
//   open dart_weights.txt, copy the kModelWeights map, paste it over the one below.

import 'dart:math' as math;

class RepSample {
  final double tSec; // timestamp in SECONDS (divide your ms by 1000)
  final double qx1, qy1, qz1, qw1; // upper arm
  final double qx2, qy2, qz2, qw2; // forearm
  RepSample(this.tSec, this.qx1, this.qy1, this.qz1, this.qw1, this.qx2,
      this.qy2, this.qz2, this.qw2);
}

// ──────────────────────────────────────────────
// Quaternion helpers  (mirror of the Python scipy operations)
// quaternion stored as [x, y, z, w]
// ──────────────────────────────────────────────
class _Q {
  // Normalize to unit quaternion. scipy does this automatically in from_quat();
  // raw IMU quaternions are only approximately unit, so we MUST match that.
  static List<double> norm(List<double> q) {
    final m = math.sqrt(
        q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]);
    if (m == 0) return [0, 0, 0, 1];
    return [q[0] / m, q[1] / m, q[2] / m, q[3] / m];
  }

  static List<double> inv(List<double> qIn) {
    final q = norm(qIn);
    final x = q[0], y = q[1], z = q[2], w = q[3];
    final n = x * x + y * y + z * z + w * w;
    return [-x / n, -y / n, -z / n, w / n];
  }

  // Hamilton product a*b, both [x,y,z,w]
  static List<double> mul(List<double> a, List<double> b) {
    final ax = a[0], ay = a[1], az = a[2], aw = a[3];
    final bx = b[0], by = b[1], bz = b[2], bw = b[3];
    return [
      aw * bx + ax * bw + ay * bz - az * by,
      aw * by - ax * bz + ay * bw + az * bx,
      aw * bz + ax * by - ay * bx + az * bw,
      aw * bw - ax * bx - ay * by - az * bz,
    ];
  }

  // geodesic angle (degrees) of relative rotation inv(a)*b
  static double angleBetween(List<double> a, List<double> b) {
    final rel = mul(inv(a), norm(b));
    final w = rel[3].abs().clamp(0.0, 1.0);
    return 2.0 * math.acos(w) * 180.0 / math.pi;
  }

  // euler 'xyz' (degrees) of relative rotation inv(start)*now
  // Matches scipy Rotation.as_euler('xyz', degrees=True).
  static List<double> eulerXyz(List<double> start, List<double> now) {
    final q = mul(inv(start), norm(now)); // [x,y,z,w]
    final x = q[0], y = q[1], z = q[2], w = q[3];
    // intrinsic xyz (same as scipy 'xyz')
    final roll = math.atan2(2 * (w * x + y * z), 1 - 2 * (x * x + y * y));
    final s = (2 * (w * y - z * x)).clamp(-1.0, 1.0);
    final pitch = math.asin(s);
    final yaw = math.atan2(2 * (w * z + x * y), 1 - 2 * (y * y + z * z));
    const r2d = 180.0 / math.pi;
    return [roll * r2d, pitch * r2d, yaw * r2d];
  }
}

double _median(List<double> v) {
  final s = List<double>.from(v)..sort();
  final n = s.length;
  if (n == 0) return 0;
  return n.isOdd ? s[n ~/ 2] : (s[n ~/ 2 - 1] + s[n ~/ 2]) / 2.0;
}

double _ptp(List<double> v) =>
    v.isEmpty ? 0 : v.reduce(math.max) - v.reduce(math.min);

// ──────────────────────────────────────────────
// Feature extraction — produces the 8 features in the SAME ORDER as Python
// ──────────────────────────────────────────────
List<double> extractFeatures(List<RepSample> s) {
  final n = s.length;
  if (n < 3) return List.filled(8, 0.0);

  final upper = s.map((e) => [e.qx1, e.qy1, e.qz1, e.qw1]).toList();
  final fore = s.map((e) => [e.qx2, e.qy2, e.qz2, e.qw2]).toList();
  final t = s.map((e) => e.tSec).toList();

  // elbow angle, baseline-subtracted (median of first 10)
  final elbowRaw = List<double>.generate(
      n, (i) => _Q.angleBetween(upper[i], fore[i]));
  final base = _median(elbowRaw.take(math.min(10, n)).toList());
  final elbow = elbowRaw.map((a) => a - base).toList();

  // upper-arm drift from start
  final drift =
      List<double>.generate(n, (i) => _Q.angleBetween(upper[0], upper[i]));

  // upper-arm euler xyz from start
  final eul = List<List<double>>.generate(
      n, (i) => _Q.eulerXyz(upper[0], upper[i]));

  // duration
  final dur = (t[n - 1] - t[0]).abs() < 1e-6 ? 0.01 : (t[n - 1] - t[0]);

  // velocity & jerk of elbow angle
  final vel = <double>[];
  for (var i = 1; i < n; i++) {
    var dt = t[i] - t[i - 1];
    if (dt == 0) dt = 1e-3;
    vel.add((elbow[i] - elbow[i - 1]) / dt);
  }
  final acc = <double>[];
  for (var i = 1; i < vel.length; i++) {
    var dt = t[i] - t[i - 1];
    if (dt == 0) dt = 1e-3;
    acc.add((vel[i] - vel[i - 1]) / dt);
  }
  final jerk = <double>[];
  for (var i = 1; i < acc.length; i++) {
    var dt = t[i] - t[i - 1];
    if (dt == 0) dt = 1e-3;
    jerk.add((acc[i] - acc[i - 1]) / dt);
  }

  double meanAbs(List<double> v) =>
      v.isEmpty ? 0 : v.map((e) => e.abs()).reduce((a, b) => a + b) / v.length;
  double maxAbs(List<double> v) =>
      v.isEmpty ? 0 : v.map((e) => e.abs()).reduce(math.max);

  return [
    _ptp(elbow), // 0 rom
    drift.reduce(math.max), // 1 upper_drift
    meanAbs(jerk), // 2 mean_jerk
    dur, // 3 duration_s
    maxAbs(vel), // 4 peak_vel
    _ptp(eul.map((e) => e[0]).toList()), // 5 upper_roll
    _ptp(eul.map((e) => e[1]).toList()), // 6 upper_pitch
    _ptp(eul.map((e) => e[2]).toList()), // 7 upper_yaw
  ];
}

// ──────────────────────────────────────────────
// Logistic regression inference (multinomial / softmax)
// ──────────────────────────────────────────────
class RepClassifier {
  static String classify(List<RepSample> samples) {
    final f = extractFeatures(samples);
    final mean = (kModelWeights['scaler_mean'] as List).cast<num>();
    final std = (kModelWeights['scaler_std'] as List).cast<num>();
    final coef = (kModelWeights['coef'] as List)
        .map((row) => (row as List).cast<num>())
        .toList();
    final intercept = (kModelWeights['intercept'] as List).cast<num>();
    final classes = (kModelWeights['classes'] as List).cast<String>();

    // standardize
    final z = List<double>.generate(
        f.length, (i) => (f[i] - mean[i]) / (std[i] == 0 ? 1 : std[i]));

    // score per class = intercept + coef · z
    final scores = List<double>.generate(classes.length, (c) {
      var s = intercept[c].toDouble();
      for (var j = 0; j < z.length; j++) {
        s += coef[c][j] * z[j];
      }
      return s;
    });

    // argmax (softmax not needed if we only want the label)
    var best = 0;
    for (var c = 1; c < scores.length; c++) {
      if (scores[c] > scores[best]) best = c;
    }
    return classes[best];
  }

  // Optional: full probability distribution (softmax) if you want confidence.
  static Map<String, double> classifyProba(List<RepSample> samples) {
    final f = extractFeatures(samples);
    final mean = (kModelWeights['scaler_mean'] as List).cast<num>();
    final std = (kModelWeights['scaler_std'] as List).cast<num>();
    final coef = (kModelWeights['coef'] as List)
        .map((row) => (row as List).cast<num>())
        .toList();
    final intercept = (kModelWeights['intercept'] as List).cast<num>();
    final classes = (kModelWeights['classes'] as List).cast<String>();

    final z = List<double>.generate(
        f.length, (i) => (f[i] - mean[i]) / (std[i] == 0 ? 1 : std[i]));
    final scores = List<double>.generate(classes.length, (c) {
      var s = intercept[c].toDouble();
      for (var j = 0; j < z.length; j++) s += coef[c][j] * z[j];
      return s;
    });
    final mx = scores.reduce(math.max);
    final exps = scores.map((s) => math.exp(s - mx)).toList();
    final sum = exps.reduce((a, b) => a + b);
    return {for (var c = 0; c < classes.length; c++) classes[c]: exps[c] / sum};
  }
}

// ──────────────────────────────────────────────
// PASTE the contents of dart_weights.txt below (replace this placeholder).
// ──────────────────────────────────────────────
const Map<String, dynamic> kModelWeights = {
  // ... paste from dart_weights.txt ...
};
