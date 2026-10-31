import 'package:google_generative_ai/google_generative_ai.dart';

class AiCoach {
  static const String _apiKey = String.fromEnvironment(
    'KINETISM_GEMINI_API_KEY',
  );

  // Hard-Coded Vocabulary (Translated directly from Python script)
  static final Map<String, String> phraseBank = {
    "extension_error": "straighten your arm fully",
    "flexion_error": "curl upwards further",
    "speed_error": "slow down your motion",
    "success": "perfect form, keep it up",
    "general": "adjust your position",
    // Quality judge fault keys (from RepQualityJudge)
    "insufficient_rom":
        "try to straighten and bend your arm through the full range",
    "shoulder_swing": "keep your upper arm still and close to your body",
    "compensation": "isolate the movement to your elbow only",
    "tremor": "slow down and focus on a smooth, steady movement",
    "speed_cheating":
        "slow down — control the full arc for better muscle engagement",
    "unsmooth": "aim for a smooth, continuous arc with no jerky stops",
  };

  static GenerativeModel _createModel() {
    if (_apiKey.trim().isEmpty) {
      throw StateError('AI coaching is not configured for this build.');
    }
    return GenerativeModel(model: 'gemini-2.5-flash', apiKey: _apiKey);
  }

  static Future<String> getFeedback(String sensorIssue) async {
    final String lowerIssue = sensorIssue.toLowerCase();
    String targetPhrase;

    if (phraseBank.containsKey(lowerIssue)) {
      targetPhrase = phraseBank[lowerIssue]!;
    } else if (lowerIssue.contains("degree") && lowerIssue.contains("45")) {
      targetPhrase = phraseBank["extension_error"]!;
    } else if (lowerIssue.contains("fast")) {
      targetPhrase = phraseBank["speed_error"]!;
    } else if (lowerIssue.contains("good") || lowerIssue.contains("180")) {
      targetPhrase = phraseBank["success"]!;
    } else if (lowerIssue.contains("curl") || lowerIssue.contains("peak")) {
      targetPhrase = phraseBank["flexion_error"]!;
    } else {
      targetPhrase = phraseBank["general"]!;
    }

    final model = _createModel();
    final prompt =
        """
        You are a friendly physical therapy coach.
        The technical correction is: '$targetPhrase'.
        Rewrite this into a 1-sentence encouraging command for the patient.
        Do not use conversational filler or say 'AI:'.
      """;
    final response = await model.generateContent([Content.text(prompt)]);
    final cleanText =
        response.text?.replaceAll('"', '').replaceAll('AI:', '').trim() ?? '';
    if (cleanText.length < 5) {
      throw const FormatException(
        'AI feedback did not include a usable recommendation. Try again.',
      );
    }
    return cleanText;
  }

  static Future<String> generateSessionFeedback({
    required int totalReps,
    required int numSessions,
    required String primaryExercise,
  }) async {
    final avgRepsPerSession = numSessions > 0 ? totalReps / numSessions : 0.0;
    final model = _createModel();
    final prompt =
        """
You are a physical therapy assistant. Analyze this session data:
- Total Reps: $totalReps
- Number of Sessions: $numSessions
- Average Reps per Session: ${avgRepsPerSession.toStringAsFixed(1)}
- Primary Exercise: $primaryExercise

Write a 1-sentence recommendation for the patient.
""";
    final response = await model.generateContent([Content.text(prompt)]);
    final text = response.text?.trim() ?? '';
    if (text.isEmpty) {
      throw const FormatException(
        'AI report did not include a recommendation. Try again.',
      );
    }
    return text;
  }
}
