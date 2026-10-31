// ─────────────────────────────────────────────────────────────────────────────
// rehab_chat_service.dart
//
// Complete Flutter service for the Rehab AI Chatbot.
// Features:
//   - Connects to HuggingFace Spaces FastAPI backend
//   - Cold start warm-up with status callbacks
//   - Sends sensor data (during exercise) or plain query (after exercise)
//   - Returns AI reply as TEXT first, then speaks it after
//   - Text-To-Speech (TTS) output
//
// Dependencies to add in pubspec.yaml:
//   dependencies:
//     http: ^1.2.0
//     flutter_tts: ^4.0.2
// ─────────────────────────────────────────────────────────────────────────────

import 'dart:convert';
import 'package:http/http.dart' as http;
import 'package:flutter_tts/flutter_tts.dart';

// ─────────────────────────────────────────────
// DATA MODELS
// ─────────────────────────────────────────────

/// Sensor data sent from the IMU / feedback engine during exercise.
/// All fields are optional — only send what is available.
class SensorStats {
  final String? exercise;   // e.g. "shoulder raise"
  final double? accuracy;   // 0.0 to 100.0
  final String? issue;      // PHRASE_BANK key: "extension_error", "speed_error", etc.
  final int? repCount;      // number of reps completed

  SensorStats({
    this.exercise,
    this.accuracy,
    this.issue,
    this.repCount,
  });

  Map<String, dynamic> toJson() => {
        if (exercise != null) 'exercise': exercise,
        if (accuracy != null) 'accuracy': accuracy,
        if (issue != null) 'issue': issue,
        if (repCount != null) 'rep_count': repCount,
      };
}

class ChatResponse {
  final String reply;           // AI-generated answer text
  final List<String> sources;   // PDF sources used
  final bool isError;           // true if something went wrong

  ChatResponse({
    required this.reply,
    required this.sources,
    this.isError = false,
  });

  factory ChatResponse.fromJson(Map<String, dynamic> json) {
    return ChatResponse(
      reply: json['reply'] ?? 'No response received.',
      sources: List<String>.from(json['sources'] ?? []),
    );
  }

  /// Convenience error constructor
  factory ChatResponse.error(String message) {
    return ChatResponse(reply: message, sources: [], isError: true);
  }
}

/// Server warm-up status — passed back to the UI so it can show
enum WarmUpStatus {
  checking,    // pinging the server right now
  warming,     // server is waking up, please wait
  ready,       // server is awake and ready
  failed,      // could not reach server after retries
}

// ─────────────────────────────────────────────
// TTS SERVICE
// ─────────────────────────────────────────────

/// Handles all Text-To-Speech functionality.
/// Call RehabTtsService.init() once at app startup.
class RehabTtsService {
  static final FlutterTts _tts = FlutterTts();
  static bool _isInitialized = false;

  /// Call this once in your main widget's initState()
  static Future<void> init() async {
    if (_isInitialized) return;

    await _tts.setLanguage('en-US');
    await _tts.setSpeechRate(0.45);   
    await _tts.setVolume(1.0);
    await _tts.setPitch(1.0);

    _tts.setStartHandler(() {});
    _tts.setCompletionHandler(() {});
    _tts.setErrorHandler((msg) {});

    _isInitialized = true;
  }

  /// Speak the given text out loud.
  static Future<void> speak(String text) async {
    await _tts.stop();
    await _tts.speak(text);
  }

  /// Stop speaking immediately.
  static Future<void> stop() async {
    await _tts.stop();
  }

  /// Check if TTS is currently speaking.
  static Future<bool> isSpeaking() async {
    // isSpeaking getter removed in flutter_tts 4.x; stub returns false
    return false;
  }
}

// ─────────────────────────────────────────────
// REHAB CHATBOT 
// ─────────────────────────────────────────────

class RehabChatService {

  static const String _baseUrl = 'https://yeeeee102-rehabbot.hf.space';

  // ── WARM-UP (COLD START HANDLER) ────────────────────────────────────────────

  /// Call this when the app first opens — before the user sends any message.
  /// This wakes up the HuggingFace Space so the first real query is fast.
  ///
  /// [onStatusChange] gives your UI live updates so you can show a message
  /// like "Connecting to AI Coach..." while waiting.
  ///
  /// Example usage in initState():
  /// ```dart
  /// RehabChatService.warmUp(
  ///   onStatusChange: (status) {
  ///     setState(() {
  ///       if (status == WarmUpStatus.checking) _statusText = 'Connecting...';
  ///       if (status == WarmUpStatus.warming)  _statusText = 'AI Coach is waking up...';
  ///       if (status == WarmUpStatus.ready)    _statusText = 'AI Coach is ready!';
  ///       if (status == WarmUpStatus.failed)   _statusText = 'Could not connect.';
  ///     });
  ///   },
  /// );
  /// ```
  static Future<bool> warmUp({
    Function(WarmUpStatus)? onStatusChange,
  }) async {
    onStatusChange?.call(WarmUpStatus.checking);

    // Try up to 4 times with increasing wait between attempts
    // Attempt 1: wait 5s, Attempt 2: wait 10s, Attempt 3: wait 15s, Attempt 4: wait 20s
    final waitSeconds = [5, 10, 15, 20];

    for (int attempt = 0; attempt < 4; attempt++) {
      try {
        if (attempt > 0) {
          // Server didn't respond yet — tell UI it's still waking up
          onStatusChange?.call(WarmUpStatus.warming);
        }

        // Ping the health check endpoint (GET /)
        final response = await http
            .get(Uri.parse('$_baseUrl/'))
            .timeout(Duration(seconds: waitSeconds[attempt]));

        if (response.statusCode == 200) {
          // Server is awake!
          onStatusChange?.call(WarmUpStatus.ready);
          return true;
        }

      } on Exception {
        // Timeout or connection error — try again
        if (attempt < 3) {
          await Future.delayed(Duration(seconds: waitSeconds[attempt]));
        }
      }
    }

    // All 4 attempts failed
    onStatusChange?.call(WarmUpStatus.failed);
    return false;
  }

  // ── PUBLIC METHODS ──────────────────────────────────────────────────────────

  /// Use this DURING exercise when sensor data is available.
  ///
  /// IMPORTANT: Text is returned FIRST (instantly when server responds),
  /// then TTS speaks it. Both happen from the same reply — not simultaneously.
  /// See "Text vs Speech" note at the bottom of this file.
  ///
  /// Example:
  /// ```dart
  /// final response = await RehabChatService.askDuringExercise(
  ///   query: "my shoulder feels sore",
  ///   sensorStats: SensorStats(
  ///     exercise: "shoulder raise",
  ///     accuracy: 45.0,
  ///     issue: "extension_error",
  ///     repCount: 5,
  ///   ),
  ///   speakReply: true,
  /// );
  /// // At this point response.reply already has the text
  /// // TTS is speaking it in the background
  /// setState(() => _replyText = response.reply);
  /// ```
  static Future<ChatResponse> askDuringExercise({
    required String query,
    required SensorStats sensorStats,
    bool speakReply = true,
  }) async {
    return _sendRequest(
      query: query,
      sensorStats: sensorStats,
      speakReply: speakReply,
    );
  }

  /// Use this AFTER exercise or for general questions (no sensor data).
  static Future<ChatResponse> askAfterExercise({
    required String query,
    bool speakReply = true,
  }) async {
    return _sendRequest(
      query: query,
      sensorStats: null,
      speakReply: speakReply,
    );
  }

  /// Stop the TTS voice at any time (e.g. when user taps a stop button).
  static Future<void> stopSpeaking() async {
    await RehabTtsService.stop();
  }

  // ── PRIVATE CORE METHOD ─────────────────────────────────────────────────────

  static Future<ChatResponse> _sendRequest({
    required String query,
    SensorStats? sensorStats,
    bool speakReply = true,
  }) async {
    try {
      final Map<String, dynamic> body = {
        'query': query,
        'sensor_stats': sensorStats?.toJson(),
      };

      final response = await http
          .post(
            Uri.parse('$_baseUrl/chat'),
            headers: {'Content-Type': 'application/json'},
            body: jsonEncode(body),
          )
          .timeout(const Duration(seconds: 60)); // longer timeout for cold starts

      if (response.statusCode == 200) {
        final json = jsonDecode(response.body);
        final chatResponse = ChatResponse.fromJson(json);

        // ── TEXT comes back here (return value) ──────────────────────────────
        // ── SPEECH starts here  (speaks the same text) ──────────────────────
        // They are the SAME content, not two different outputs.
        // Text is returned immediately in chatResponse.reply.
        // TTS then speaks that same text in the background.
        if (speakReply && chatResponse.reply.isNotEmpty) {
          RehabTtsService.speak(chatResponse.reply); // no await — runs in background
        }

        return chatResponse; // UI gets the text instantly without waiting for speech to finish

      } else {
        final errorMsg = 'The AI coach is temporarily unavailable. Please try again.';
        if (speakReply) RehabTtsService.speak(errorMsg);
        return ChatResponse.error(errorMsg);
      }

    } on Exception {
      final errorMsg = 'Connection error. Please check your internet and try again.';
      if (speakReply) RehabTtsService.speak(errorMsg);
      return ChatResponse.error(errorMsg);
    }
  }
}


//
// ─────────────────────────────────────────────────────────────────────────────
// COLD START — RECOMMENDED USAGE MAIN WIDGET
// ─────────────────────────────────────────────────────────────────────────────
//
// class _ExerciseScreenState extends State<ExerciseScreen> {
//   String _statusText = 'Connecting to AI Coach...';
//   String _replyText = '';
//   bool _serverReady = false;
//
//   @override
//   void initState() {
//     super.initState();
//     RehabTtsService.init();
//     _startWarmUp();
//   }
//
//   Future<void> _startWarmUp() async {
//     await RehabChatService.warmUp(
//       onStatusChange: (status) {
//         setState(() {
//           switch (status) {
//             case WarmUpStatus.checking:
//               _statusText = 'Connecting to AI Coach...';
//               break;
//             case WarmUpStatus.warming:
//               _statusText = 'AI Coach is waking up, please wait...';
//               break;
//             case WarmUpStatus.ready:
//               _statusText = 'AI Coach is ready!';
//               _serverReady = true;
//               break;
//             case WarmUpStatus.failed:
//               _statusText = 'Could not connect. Check your internet.';
//               break;
//           }
//         });
//       },
//     );
//   }
//
//   Future<void> _sendQuestion(String query) async {
//     if (!_serverReady) {
//       setState(() => _replyText = 'Please wait, AI Coach is still loading...');
//       return;
//     }
//     final response = await RehabChatService.askAfterExercise(query: query);
//     setState(() => _replyText = response.reply);
//     // TTS is already speaking in the background
//   }
// }
//
// ─────────────────────────────────────────────────────────────────────────────

// How to call and ask question? 
// Use this format: 
//
// // During exercise
//final response = await RehabChatService.askDuringExercise(
//  query: "my shoulder feels sore",
//  sensorStats: SensorStats(
//    exercise: "shoulder raise",
//    accuracy: 45.0,
//    issue: "extension_error",
//    repCount: 5,
//  ),
//);
//setState(() => _replyText = response.reply);

// After exercise
//final response = await RehabChatService.askAfterExercise(
//  query: "why does my shoulder feel sore?",
//);
//setState(() => _replyText = response.reply);