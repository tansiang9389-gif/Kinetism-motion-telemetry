({bool atPeak, bool counted}) advanceRepetitionCycle({
  required double angle,
  required double restingAngle,
  required double peakAngle,
  required bool atPeak,
}) {
  if (!angle.isFinite || angle < 0 || angle > 180) {
    throw ArgumentError.value(
      angle,
      'angle',
      'Must be finite and within 0-180.',
    );
  }
  if (!restingAngle.isFinite ||
      !peakAngle.isFinite ||
      restingAngle > 180 ||
      peakAngle < 0 ||
      peakAngle >= restingAngle) {
    throw ArgumentError('Peak must be below resting angle within 0-180.');
  }
  if (angle >= restingAngle) return (atPeak: false, counted: atPeak);
  if (angle <= peakAngle) return (atPeak: true, counted: false);
  return (atPeak: atPeak, counted: false);
}
