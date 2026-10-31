class ExerciseValidation {
  static String? requiredText(String? value) =>
      value == null || value.trim().isEmpty ? 'Enter a value.' : null;

  static String? angle(String? value) {
    final number = double.tryParse(value?.trim() ?? '');
    if (number == null || !number.isFinite || number < 0 || number > 180) {
      return 'Enter an angle from 0 to 180 degrees.';
    }
    return null;
  }

  static String? peakAngle(String? value, String restingAngle) {
    final error = angle(value);
    if (error != null) return error;
    final resting = double.tryParse(restingAngle.trim());
    final peak = double.parse(value!.trim());
    if (resting != null && resting.isFinite && peak >= resting) {
      return 'Peak must be below resting angle for this tracker.';
    }
    return null;
  }

  static String? dailyGoal(String? value) {
    final number = int.tryParse(value?.trim() ?? '');
    return number == null || number <= 0
        ? 'Enter a whole-number goal greater than zero.'
        : null;
  }

  static String? videoUrl(String? value) {
    final uri = Uri.tryParse(value?.trim() ?? '');
    if (uri == null ||
        !const ['https', 'http'].contains(uri.scheme) ||
        uri.host.isEmpty ||
        uri.userInfo.isNotEmpty) {
      return 'Enter a full HTTP or HTTPS video URL without credentials.';
    }
    return null;
  }
}
