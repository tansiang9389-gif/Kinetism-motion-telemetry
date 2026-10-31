import 'package:flutter_test/flutter_test.dart';
import 'package:medical_device_app/ai_coach.dart';

void main() {
  final missingConfiguration = isA<StateError>().having(
    (error) => error.message,
    'message',
    'AI coaching is not configured for this build.',
  );

  test(
    'Public builds report missing AI configuration instead of fake praise',
    () async {
      await expectLater(
        AiCoach.getFeedback('good 180'),
        throwsA(missingConfiguration),
      );
    },
  );

  test(
    'Public builds do not fabricate or save an AI session recommendation',
    () async {
      await expectLater(
        AiCoach.generateSessionFeedback(
          totalReps: 20,
          numSessions: 1,
          primaryExercise: 'Elbow curl',
        ),
        throwsA(missingConfiguration),
      );
    },
  );
}
