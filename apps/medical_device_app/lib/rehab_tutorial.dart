import 'package:flutter/material.dart';
import 'package:cloud_firestore/cloud_firestore.dart';
import 'package:flutter_blue_plus/flutter_blue_plus.dart';
import 'package:permission_handler/permission_handler.dart';
import 'package:vector_math/vector_math_64.dart' as vector;
import 'package:speech_to_text/speech_to_text.dart';
import 'dart:math' as math;
import 'rehab_chat_service.dart';
import 'rep_quality_judge.dart';

// ─────────────────────────────────────────────────────────────────────────────
// SCREEN 1 — EXERCISE SELECTION
// ─────────────────────────────────────────────────────────────────────────────

class RehabTutorialScreen extends StatelessWidget {
  const RehabTutorialScreen({super.key});

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      backgroundColor: const Color(0xFF121212),
      appBar: AppBar(
        backgroundColor: Colors.black,
        elevation: 0,
        title: const Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text('Rehab Tutorial', style: TextStyle(fontWeight: FontWeight.bold, fontSize: 18)),
            Text('AI-Guided Movement Training', style: TextStyle(color: Colors.cyanAccent, fontSize: 11)),
          ],
        ),
      ),
      body: StreamBuilder<QuerySnapshot>(
        stream: FirebaseFirestore.instance.collection('Exercises').snapshots(),
        builder: (context, snapshot) {
          if (snapshot.connectionState == ConnectionState.waiting) {
            return const Center(child: CircularProgressIndicator(color: Colors.cyanAccent));
          }
          if (!snapshot.hasData || snapshot.data!.docs.isEmpty) {
            return Center(
              child: Column(
                mainAxisSize: MainAxisSize.min,
                children: [
                  const Icon(Icons.fitness_center, color: Colors.grey, size: 48),
                  const SizedBox(height: 12),
                  const Text('No exercises found.', style: TextStyle(color: Colors.grey, fontSize: 16)),
                  const SizedBox(height: 6),
                  Text('Add exercises via the sidebar first.',
                      style: TextStyle(color: Colors.grey[700], fontSize: 13)),
                ],
              ),
            );
          }

          final exercises = snapshot.data!.docs;

          return CustomScrollView(
            slivers: [
              SliverToBoxAdapter(
                child: Padding(
                  padding: const EdgeInsets.fromLTRB(16, 16, 16, 8),
                  child: Container(
                    padding: const EdgeInsets.all(14),
                    decoration: BoxDecoration(
                      color: Colors.cyanAccent.withValues(alpha: 0.08),
                      borderRadius: BorderRadius.circular(12),
                      border: Border.all(color: Colors.cyanAccent.withValues(alpha: 0.25)),
                    ),
                    child: const Row(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Icon(Icons.info_outline, color: Colors.cyanAccent, size: 18),
                        SizedBox(width: 10),
                        Expanded(
                          child: Text(
                            'Connect your IMU sensor and select an exercise. '
                            'The AI coach will guide your arm through each movement '
                            'with real-time feedback on position and timing.',
                            style: TextStyle(color: Colors.white70, fontSize: 12, height: 1.5),
                          ),
                        ),
                      ],
                    ),
                  ),
                ),
              ),
              SliverPadding(
                padding: const EdgeInsets.fromLTRB(16, 8, 16, 24),
                sliver: SliverGrid(
                  gridDelegate: const SliverGridDelegateWithFixedCrossAxisCount(
                    crossAxisCount: 2,
                    crossAxisSpacing: 12,
                    mainAxisSpacing: 12,
                    childAspectRatio: 1.05,
                  ),
                  delegate: SliverChildBuilderDelegate(
                    (context, index) {
                      final data = exercises[index].data() as Map<String, dynamic>;
                      final name = (data['name'] as String?) ?? 'Unknown';
                      final restingAngle = (data['restingAngle'] as num?)?.toDouble() ?? 180.0;
                      final peakAngle = (data['peakAngle'] as num?)?.toDouble() ?? 0.0;

                      return _ExerciseCard(
                        name: name,
                        restingAngle: restingAngle,
                        peakAngle: peakAngle,
                        onTap: () => Navigator.push(
                          context,
                          MaterialPageRoute(
                            builder: (_) => RehabTutorialSessionScreen(
                              exerciseName: name,
                              restingAngle: restingAngle,
                              peakAngle: peakAngle,
                            ),
                          ),
                        ),
                      );
                    },
                    childCount: exercises.length,
                  ),
                ),
              ),
            ],
          );
        },
      ),
    );
  }
}

// ── Exercise Card ──

class _ExerciseCard extends StatelessWidget {
  final String name;
  final double restingAngle;
  final double peakAngle;
  final VoidCallback onTap;

  const _ExerciseCard({
    required this.name,
    required this.restingAngle,
    required this.peakAngle,
    required this.onTap,
  });

  @override
  Widget build(BuildContext context) {
    return GestureDetector(
      onTap: onTap,
      child: Container(
        decoration: BoxDecoration(
          gradient: const LinearGradient(
            colors: [Color(0xFF1A1A2E), Color(0xFF16213E)],
            begin: Alignment.topLeft,
            end: Alignment.bottomRight,
          ),
          borderRadius: BorderRadius.circular(16),
          border: Border.all(color: Colors.cyanAccent.withValues(alpha: 0.3), width: 1),
        ),
        child: Padding(
          padding: const EdgeInsets.all(14),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            mainAxisAlignment: MainAxisAlignment.spaceBetween,
            children: [
              Container(
                padding: const EdgeInsets.all(8),
                decoration: BoxDecoration(
                  color: Colors.cyanAccent.withValues(alpha: 0.12),
                  borderRadius: BorderRadius.circular(8),
                ),
                child: const Icon(Icons.self_improvement, color: Colors.cyanAccent, size: 22),
              ),
              Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(
                    name,
                    style: const TextStyle(
                      color: Colors.white,
                      fontWeight: FontWeight.bold,
                      fontSize: 13,
                    ),
                    maxLines: 2,
                    overflow: TextOverflow.ellipsis,
                  ),
                  const SizedBox(height: 3),
                  Text(
                    '${restingAngle.toInt()}° → ${peakAngle.toInt()}°',
                    style: TextStyle(color: Colors.grey[500], fontSize: 11),
                  ),
                ],
              ),
              Row(
                children: [
                  const Icon(Icons.play_circle_outline, color: Colors.cyanAccent, size: 14),
                  const SizedBox(width: 4),
                  const Text(
                    'Start Tutorial',
                    style: TextStyle(color: Colors.cyanAccent, fontSize: 11, fontWeight: FontWeight.w600),
                  ),
                ],
              ),
            ],
          ),
        ),
      ),
    );
  }
}

// ─────────────────────────────────────────────────────────────────────────────
// SCREEN 2 — TUTORIAL SESSION (BLE + IMU + AI COACH)
// ─────────────────────────────────────────────────────────────────────────────

class RehabTutorialSessionScreen extends StatefulWidget {
  final String exerciseName;
  final double restingAngle;
  final double peakAngle;

  const RehabTutorialSessionScreen({
    super.key,
    required this.exerciseName,
    required this.restingAngle,
    required this.peakAngle,
  });

  @override
  State<RehabTutorialSessionScreen> createState() => _RehabTutorialSessionScreenState();
}

class _RehabTutorialSessionScreenState extends State<RehabTutorialSessionScreen> {
  // ── Bluetooth ───────────────────────────────────────────────────────────────
  String _status = 'Disconnected';
  BluetoothDevice? _connectedDevice;

  // ── IMU / Quaternions ───────────────────────────────────────────────────────
  vector.Quaternion qUpper = vector.Quaternion(0, 0, 0, 1);
  vector.Quaternion qFore  = vector.Quaternion(0, 0, 0, 1);
  double _currentAngle = 180.0;

  // ── AI state ────────────────────────────────────────────────────────────────
  bool _serverReady  = false;
  bool _isAiBusy     = false;
  bool _isMuted      = false;
  String _lastSentPhase = '';

  // ── Chat ────────────────────────────────────────────────────────────────────
  final List<_ChatEntry> _chat = [];
  final ScrollController _scrollCtrl = ScrollController();

  // ── Rep quality judge ────────────────────────────────────────────────────────
  late RepQualityJudge _qualityJudge;
  String _prevPhase = '';
  int _totalReps = 0;
  int _goodReps  = 0;

  // ── Phase label helpers ──────────────────────────────────────────────────────
  String _phaseLabel = 'Waiting';
  Color  _phaseColor = Colors.grey;

  // ── Voice input ──────────────────────────────────────────────────────────────
  final SpeechToText _speech = SpeechToText();
  bool _speechAvailable = false;
  bool _isListening      = false;
  String _liveTranscript = '';

  // ─────────────────────────────────────────────────────────────────────────
  // LIFECYCLE
  // ─────────────────────────────────────────────────────────────────────────

  @override
  void initState() {
    super.initState();
    RehabTtsService.init();
    _initSpeech();
    _warmUp();
    _qualityJudge = RepQualityJudge(
      targetRom: widget.restingAngle - widget.peakAngle,
    );
  }

  @override
  void dispose() {
    _connectedDevice?.disconnect();
    RehabTtsService.stop();
    _scrollCtrl.dispose();
    super.dispose();
  }

  // ─────────────────────────────────────────────────────────────────────────
  // WARM-UP
  // ─────────────────────────────────────────────────────────────────────────

  Future<void> _warmUp() async {
    _pushMessage('Connecting to AI Coach...', isSystem: true);

    await RehabChatService.warmUp(
      onStatusChange: (status) {
        if (!mounted) return;
        switch (status) {
          case WarmUpStatus.checking:
            _pushMessage('Connecting to AI Coach...', isSystem: true);
            break;
          case WarmUpStatus.warming:
            _pushMessage('AI Coach is waking up, please wait...', isSystem: true);
            break;
          case WarmUpStatus.ready:
            setState(() => _serverReady = true);
            _pushMessage('AI Coach is ready! Connect your IMU sensor to begin.', isSystem: true);
            _sendIntro();
            break;
          case WarmUpStatus.failed:
            _pushMessage('Could not reach AI Coach. Check your internet.', isSystem: true);
            break;
        }
      },
    );
  }

  // ─────────────────────────────────────────────────────────────────────────
  // VOICE INPUT
  // ─────────────────────────────────────────────────────────────────────────

  Future<void> _initSpeech() async {
    final available = await _speech.initialize(
      onError: (e) {
        if (mounted) setState(() => _isListening = false);
      },
      onStatus: (status) {
        // When the recogniser finishes on its own, send whatever was heard
        if (status == SpeechToText.doneStatus && _isListening) {
          _stopAndSend();
        }
      },
    );
    if (mounted) setState(() => _speechAvailable = available);
  }

  Future<void> _startListening() async {
    if (!_speechAvailable || _isAiBusy) return;
    await RehabTtsService.stop(); // pause TTS while user speaks
    setState(() {
      _isListening = true;
      _liveTranscript = '';
    });
    await _speech.listen(
      onResult: (result) {
        if (mounted) setState(() => _liveTranscript = result.recognizedWords);
      },
      listenFor: const Duration(seconds: 30),
      pauseFor: const Duration(seconds: 4),
      localeId: 'en_US',
      listenOptions: SpeechListenOptions(cancelOnError: true, partialResults: true),
    );
  }

  Future<void> _stopAndSend() async {
    if (!_isListening) return;
    await _speech.stop();
    final query = _liveTranscript.trim();
    setState(() {
      _isListening = false;
      _liveTranscript = '';
    });
    if (query.isEmpty || !_serverReady) return;

    // Show user's question in the chat
    _pushMessage(query, isUser: true);

    setState(() => _isAiBusy = true);
    final response = await RehabChatService.askAfterExercise(
      query: query,
      speakReply: !_isMuted,
    );
    _pushMessage(response.reply);
    Future.delayed(const Duration(seconds: 4), () {
      if (mounted) setState(() => _isAiBusy = false);
    });
  }

  // Send an introductory overview once the server is warm
  Future<void> _sendIntro() async {
    final response = await RehabChatService.askAfterExercise(
      query: 'I am about to learn how to do ${widget.exerciseName}. '
             'Please give me a brief overview: what muscles it targets, '
             'the correct starting position, and what the full movement should look like.',
      speakReply: !_isMuted,
    );
    _pushMessage(response.reply);
  }

  // ─────────────────────────────────────────────────────────────────────────
  // BLUETOOTH
  // ─────────────────────────────────────────────────────────────────────────

  Future<void> _connectAndScan() async {
    await [
      Permission.bluetoothScan,
      Permission.bluetoothConnect,
      Permission.location,
    ].request();

    if (!mounted) return;
    setState(() => _status = 'Scanning...');

    FlutterBluePlus.scanResults.listen((results) {
      for (final r in results) {
        if (r.advertisementData.advName == 'Physio_A') {
          FlutterBluePlus.stopScan();
          _connectToDevice(r.device);
          break;
        }
      }
    });

    await FlutterBluePlus.startScan(timeout: const Duration(seconds: 4));
  }

  Future<void> _connectToDevice(BluetoothDevice device) async {
    _connectedDevice = device;
    await device.connect();
    if (!mounted) return;
    setState(() => _status = 'Tracking Active');
    _pushMessage(
      'IMU sensor connected! Hold your arm at the resting position for ${widget.exerciseName} and I will guide you.',
      isSystem: true,
    );

    final services = await device.discoverServices();
    for (final service in services) {
      for (final char in service.characteristics) {
        if (char.properties.notify) {
          await char.setNotifyValue(true);
          char.onValueReceived.listen((value) {
            final raw   = String.fromCharCodes(value);
            final parts = raw.split(',');
            if (parts.length >= 8) {
              qUpper = vector.Quaternion(
                double.parse(parts[0]), double.parse(parts[1]),
                double.parse(parts[2]), double.parse(parts[3]),
              );
              qFore = vector.Quaternion(
                double.parse(parts[4]), double.parse(parts[5]),
                double.parse(parts[6]), double.parse(parts[7]),
              );
              _processSensorData();
            }
          });
        }
      }
    }
  }

  // ─────────────────────────────────────────────────────────────────────────
  // IMU MATH + PHASE DETECTION
  // ─────────────────────────────────────────────────────────────────────────

  void _processSensorData() {
    final vU = vector.Vector3(0, 1, 0);
    final vF = vector.Vector3(0, 1, 0);
    qUpper.rotate(vU);
    qFore.rotate(vF);

    final angle = 180.0 - (vU.angleTo(vF) * (180.0 / math.pi));
    if (!mounted) return;
    setState(() => _currentAngle = angle);

    _qualityJudge.addSample(angle, qUpper);

    // ── Classify movement phase ──────────────────────────────────────────────
    String phase;
    String issueKey;
    String query;
    String label;
    Color  color;

    if (angle >= widget.restingAngle - 12) {
      phase    = 'resting';
      issueKey = 'extension_error';   // arm is extended — coach guides to start lift
      label    = 'Resting Position';
      color    = Colors.cyanAccent;
      query    = 'I am holding ${widget.exerciseName} at the resting position '
                 '(arm angle: ${angle.toStringAsFixed(0)}°). '
                 'Is my starting posture correct? When and how should I begin the upward movement?';
    } else if (angle <= widget.peakAngle + 12) {
      phase    = 'peak';
      issueKey = 'success';            // reached peak — coach confirms + advises descent
      label    = 'Peak Position';
      color    = Colors.greenAccent;
      query    = 'I have reached the peak of ${widget.exerciseName} '
                 '(arm angle: ${angle.toStringAsFixed(0)}°). '
                 'Is this the correct range of motion? Should I hold it, and how should I lower my arm?';
    } else {
      phase    = 'mid';
      issueKey = 'general';            // mid-movement — coach monitors direction / speed
      label    = 'Mid Movement';
      color    = Colors.orangeAccent;
      query    = 'I am mid-movement in ${widget.exerciseName} '
                 '(arm angle: ${angle.toStringAsFixed(0)}°). '
                 'Am I moving at the right speed and through the correct arc? '
                 'What should I focus on right now?';
    }

    // ── Rep completion: peak/mid → resting transition ───────────────────────
    if ((_prevPhase == 'peak' || _prevPhase == 'mid') && phase == 'resting') {
      final result = _qualityJudge.judge();
      _qualityJudge.reset();
      setState(() {
        _totalReps++;
        if (result.passed) _goodReps++;
      });
      if (result.passed) {
        _pushMessage(
          'Rep $_totalReps: Good rep! Quality ${(result.qualityScore * 100).toStringAsFixed(0)}%',
          isSystem: true,
        );
      } else {
        for (final msg in result.feedbackMessages) {
          _pushMessage('Rep $_totalReps: $msg', isSystem: true);
        }
      }
    }
    _prevPhase = phase;

    setState(() {
      _phaseLabel = label;
      _phaseColor = color;
    });

    // ── Trigger AI only on phase transitions and when not busy ──────────────
    if (_lastSentPhase != phase && !_isAiBusy && _serverReady && _status == 'Tracking Active') {
      _lastSentPhase = phase;
      _isAiBusy = true;

      RehabChatService.askDuringExercise(
        query: query,
        sensorStats: SensorStats(
          exercise: widget.exerciseName,
          issue: issueKey,
        ),
        speakReply: !_isMuted,
      ).then((response) {
        _pushMessage(response.reply);
        // Keep AI locked for 4 s so speech can finish before next trigger
        Future.delayed(const Duration(seconds: 4), () {
          if (mounted) setState(() => _isAiBusy = false);
        });
      });
    }
  }

  // ─────────────────────────────────────────────────────────────────────────
  // CHAT HELPERS
  // ─────────────────────────────────────────────────────────────────────────

  void _pushMessage(String text, {bool isSystem = false, bool isUser = false}) {
    if (!mounted) return;
    setState(() => _chat.add(_ChatEntry(text: text, isSystem: isSystem, isUser: isUser)));
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (_scrollCtrl.hasClients) {
        _scrollCtrl.animateTo(
          _scrollCtrl.position.maxScrollExtent,
          duration: const Duration(milliseconds: 300),
          curve: Curves.easeOut,
        );
      }
    });
  }

  // ─────────────────────────────────────────────────────────────────────────
  // UI
  // ─────────────────────────────────────────────────────────────────────────

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      backgroundColor: const Color(0xFF121212),
      appBar: AppBar(
        backgroundColor: Colors.black,
        elevation: 0,
        title: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(widget.exerciseName,
                style: const TextStyle(fontSize: 16, fontWeight: FontWeight.bold)),
            const Text('Tutorial Mode',
                style: TextStyle(color: Colors.cyanAccent, fontSize: 11)),
          ],
        ),
        actions: [
          // Mute / unmute TTS
          IconButton(
            tooltip: _isMuted ? 'Unmute AI voice' : 'Mute AI voice',
            icon: Icon(
              _isMuted ? Icons.volume_off : Icons.volume_up,
              color: _isMuted ? Colors.grey : Colors.cyanAccent,
              size: 20,
            ),
            onPressed: () {
              setState(() => _isMuted = !_isMuted);
              if (_isMuted) RehabTtsService.stop();
            },
          ),
          // Live indicator
          if (_status == 'Tracking Active')
            Padding(
              padding: const EdgeInsets.only(right: 14),
              child: Row(
                children: [
                  Container(
                    width: 8, height: 8,
                    decoration: const BoxDecoration(
                      color: Colors.greenAccent, shape: BoxShape.circle,
                    ),
                  ),
                  const SizedBox(width: 5),
                  const Text('Live', style: TextStyle(color: Colors.greenAccent, fontSize: 12)),
                ],
              ),
            ),
        ],
      ),

      body: Column(
        children: [

          // ── Status bar ───────────────────────────────────────────────────
          Container(
            color: const Color(0xFF1A1A1A),
            padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 10),
            child: Column(
              children: [
                Row(
                  mainAxisAlignment: MainAxisAlignment.spaceBetween,
                  children: [
                    Row(children: [
                      const Icon(Icons.track_changes, color: Colors.grey, size: 15),
                      const SizedBox(width: 6),
                      Text(
                        'Angle: ${_currentAngle.toStringAsFixed(1)}°',
                        style: const TextStyle(color: Colors.white, fontSize: 13),
                      ),
                    ]),
                    Container(
                      padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 4),
                      decoration: BoxDecoration(
                        color: _phaseColor.withValues(alpha: 0.15),
                        borderRadius: BorderRadius.circular(20),
                        border: Border.all(color: _phaseColor.withValues(alpha: 0.5)),
                      ),
                      child: Text(
                        _phaseLabel,
                        style: TextStyle(
                          color: _phaseColor, fontSize: 11, fontWeight: FontWeight.bold,
                        ),
                      ),
                    ),
                  ],
                ),
                if (_totalReps > 0) ...[
                  const SizedBox(height: 5),
                  Row(
                    mainAxisAlignment: MainAxisAlignment.center,
                    children: [
                      const Icon(Icons.check_circle_outline, color: Colors.greenAccent, size: 13),
                      const SizedBox(width: 4),
                      Text(
                        '$_goodReps / $_totalReps Good Reps',
                        style: const TextStyle(color: Colors.greenAccent, fontSize: 11),
                      ),
                    ],
                  ),
                ],
              ],
            ),
          ),

          // ── Arm skeleton visualizer ──────────────────────────────────────
          Container(
            height: 175,
            color: Colors.black,
            child: CustomPaint(
              painter: _TutorialArmPainter(qUpper, qFore, _phaseColor),
              child: const SizedBox.expand(),
            ),
          ),

          // ── AI Coach chat panel ──────────────────────────────────────────
          Expanded(
            child: Container(
              margin: const EdgeInsets.fromLTRB(12, 10, 12, 0),
              decoration: BoxDecoration(
                color: const Color(0xFF1A1A1A),
                borderRadius: const BorderRadius.only(
                  topLeft: Radius.circular(14),
                  topRight: Radius.circular(14),
                ),
                border: Border.all(color: Colors.white12),
              ),
              child: Column(
                children: [
                  // Chat header
                  Padding(
                    padding: const EdgeInsets.fromLTRB(14, 10, 14, 0),
                    child: Row(
                      children: [
                        const Icon(Icons.smart_toy, color: Colors.cyanAccent, size: 16),
                        const SizedBox(width: 8),
                        const Text('AI Coach',
                            style: TextStyle(
                              color: Colors.cyanAccent,
                              fontWeight: FontWeight.bold,
                              fontSize: 13,
                            )),
                        const Spacer(),
                        if (_isAiBusy)
                          const SizedBox(
                            width: 13, height: 13,
                            child: CircularProgressIndicator(
                              strokeWidth: 2, color: Colors.cyanAccent,
                            ),
                          ),
                      ],
                    ),
                  ),
                  const Divider(color: Colors.white12, height: 12),

                  // Message list
                  Expanded(
                    child: _chat.isEmpty
                        ? const Center(
                            child: Text(
                              'Waiting for AI Coach...',
                              style: TextStyle(color: Colors.grey),
                            ),
                          )
                        : ListView.builder(
                            controller: _scrollCtrl,
                            padding: const EdgeInsets.fromLTRB(10, 0, 10, 10),
                            itemCount: _chat.length,
                            itemBuilder: (context, i) {
                              final entry   = _chat[i];
                              final isLatest = i == _chat.length - 1;

                              if (entry.isSystem) {
                                return Padding(
                                  padding: const EdgeInsets.only(bottom: 6),
                                  child: Text(
                                    entry.text,
                                    style: const TextStyle(
                                      color: Colors.grey,
                                      fontSize: 11,
                                      fontStyle: FontStyle.italic,
                                    ),
                                    textAlign: TextAlign.center,
                                  ),
                                );
                              }

                              // User voice question — right-aligned purple bubble
                              if (entry.isUser) {
                                return Padding(
                                  padding: const EdgeInsets.only(bottom: 8),
                                  child: Align(
                                    alignment: Alignment.centerRight,
                                    child: Container(
                                      constraints: BoxConstraints(
                                        maxWidth: MediaQuery.of(context).size.width * 0.72,
                                      ),
                                      padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 9),
                                      decoration: BoxDecoration(
                                        color: Colors.deepPurpleAccent.withValues(alpha: 0.25),
                                        borderRadius: BorderRadius.circular(10),
                                        border: Border.all(color: Colors.deepPurpleAccent.withValues(alpha: 0.5)),
                                      ),
                                      child: Row(
                                        mainAxisSize: MainAxisSize.min,
                                        crossAxisAlignment: CrossAxisAlignment.start,
                                        children: [
                                          const Icon(Icons.mic, color: Colors.deepPurpleAccent, size: 14),
                                          const SizedBox(width: 6),
                                          Flexible(
                                            child: Text(
                                              entry.text,
                                              style: const TextStyle(
                                                color: Colors.white,
                                                fontSize: 13,
                                                height: 1.45,
                                              ),
                                            ),
                                          ),
                                        ],
                                      ),
                                    ),
                                  ),
                                );
                              }

                              // AI coach response — left-aligned cyan bubble
                              return Padding(
                                padding: const EdgeInsets.only(bottom: 8),
                                child: Container(
                                  padding: const EdgeInsets.all(12),
                                  decoration: BoxDecoration(
                                    color: isLatest
                                        ? Colors.cyanAccent.withValues(alpha: 0.1)
                                        : Colors.white.withValues(alpha: 0.04),
                                    borderRadius: BorderRadius.circular(10),
                                    border: isLatest
                                        ? Border.all(color: Colors.cyanAccent.withValues(alpha: 0.3))
                                        : null,
                                  ),
                                  child: Text(
                                    entry.text,
                                    style: TextStyle(
                                      color: isLatest ? Colors.white : Colors.white60,
                                      fontSize: 13,
                                      height: 1.5,
                                    ),
                                  ),
                                ),
                              );
                            },
                          ),
                  ),
                ],
              ),
            ),
          ),

          // ── Connect / scanning button ─────────────────────────────────────
          if (_status == 'Disconnected')
            Padding(
              padding: const EdgeInsets.fromLTRB(12, 10, 12, 12),
              child: SizedBox(
                width: double.infinity,
                child: ElevatedButton.icon(
                  style: ElevatedButton.styleFrom(
                    backgroundColor: Colors.cyanAccent,
                    foregroundColor: Colors.black,
                    padding: const EdgeInsets.symmetric(vertical: 14),
                    shape: RoundedRectangleBorder(
                      borderRadius: BorderRadius.circular(12),
                    ),
                  ),
                  icon: const Icon(Icons.bluetooth_searching),
                  label: const Text(
                    'Connect IMU Sensor',
                    style: TextStyle(fontWeight: FontWeight.bold, fontSize: 15),
                  ),
                  onPressed: _connectAndScan,
                ),
              ),
            ),

          if (_status == 'Scanning...')
            const Padding(
              padding: EdgeInsets.fromLTRB(12, 10, 12, 12),
              child: Row(
                mainAxisAlignment: MainAxisAlignment.center,
                children: [
                  SizedBox(
                    width: 16, height: 16,
                    child: CircularProgressIndicator(strokeWidth: 2, color: Colors.cyanAccent),
                  ),
                  SizedBox(width: 10),
                  Text('Scanning for Physio_A...', style: TextStyle(color: Colors.cyanAccent, fontSize: 13)),
                ],
              ),
            ),

          // ── Voice input bar (always visible once screen loads) ─────────────
          Padding(
            padding: const EdgeInsets.fromLTRB(12, 8, 12, 12),
            child: Column(
              children: [
                // Live transcript preview
                if (_isListening && _liveTranscript.isNotEmpty)
                  Container(
                    width: double.infinity,
                    margin: const EdgeInsets.only(bottom: 8),
                    padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
                    decoration: BoxDecoration(
                      color: Colors.deepPurpleAccent.withValues(alpha: 0.12),
                      borderRadius: BorderRadius.circular(10),
                      border: Border.all(color: Colors.deepPurpleAccent.withValues(alpha: 0.4)),
                    ),
                    child: Text(
                      _liveTranscript,
                      style: const TextStyle(color: Colors.white70, fontSize: 13, fontStyle: FontStyle.italic),
                    ),
                  ),

                // Mic button
                GestureDetector(
                  onTapDown: (_) => _startListening(),
                  onTapUp: (_) => _stopAndSend(),
                  onTapCancel: () => _stopAndSend(),
                  child: AnimatedContainer(
                    duration: const Duration(milliseconds: 200),
                    width: double.infinity,
                    padding: const EdgeInsets.symmetric(vertical: 14),
                    decoration: BoxDecoration(
                      color: _isListening
                          ? Colors.redAccent.withValues(alpha: 0.2)
                          : Colors.deepPurpleAccent.withValues(alpha: 0.15),
                      borderRadius: BorderRadius.circular(14),
                      border: Border.all(
                        color: _isListening
                            ? Colors.redAccent.withValues(alpha: 0.7)
                            : !_speechAvailable
                                ? Colors.grey.withValues(alpha: 0.3)
                                : Colors.deepPurpleAccent.withValues(alpha: 0.5),
                        width: _isListening ? 2 : 1,
                      ),
                    ),
                    child: Row(
                      mainAxisAlignment: MainAxisAlignment.center,
                      children: [
                        Icon(
                          _isListening ? Icons.mic : Icons.mic_none,
                          color: _isListening
                              ? Colors.redAccent
                              : !_speechAvailable
                                  ? Colors.grey
                                  : Colors.deepPurpleAccent,
                          size: 20,
                        ),
                        const SizedBox(width: 8),
                        Text(
                          _isListening
                              ? 'Listening... release to send'
                              : !_speechAvailable
                                  ? 'Microphone unavailable'
                                  : !_serverReady
                                      ? 'Waiting for AI Coach...'
                                      : 'Hold to ask a question',
                          style: TextStyle(
                            color: _isListening
                                ? Colors.redAccent
                                : !_speechAvailable
                                    ? Colors.grey
                                    : Colors.deepPurpleAccent,
                            fontSize: 14,
                            fontWeight: FontWeight.w600,
                          ),
                        ),
                      ],
                    ),
                  ),
                ),
              ],
            ),
          ),
        ],
      ),
    );
  }
}

// ─────────────────────────────────────────────────────────────────────────────
// CHAT ENTRY MODEL
// ─────────────────────────────────────────────────────────────────────────────

class _ChatEntry {
  final String text;
  final bool isSystem; // grey italic status line
  final bool isUser;   // user's voice question bubble

  const _ChatEntry({required this.text, this.isSystem = false, this.isUser = false});
}

// ─────────────────────────────────────────────────────────────────────────────
// ARM SKELETON PAINTER (tutorial version)
// ─────────────────────────────────────────────────────────────────────────────

class _TutorialArmPainter extends CustomPainter {
  final vector.Quaternion qUpper;
  final vector.Quaternion qFore;
  final Color accentColor;

  _TutorialArmPainter(this.qUpper, this.qFore, this.accentColor);

  @override
  void paint(Canvas canvas, Size size) {
    final bonePaint = Paint()
      ..color = accentColor.withValues(alpha: 0.85)
      ..strokeWidth = 11
      ..strokeCap = StrokeCap.round
      ..style = PaintingStyle.stroke;

    final jointPaint = Paint()
      ..color = Colors.white
      ..style = PaintingStyle.fill;

    final guidePaint = Paint()
      ..color = Colors.white.withValues(alpha: 0.12)
      ..strokeWidth = 1
      ..style = PaintingStyle.stroke;

    final shoulder = Offset(size.width * 0.5, size.height * 0.2);
    const double upperLen = 115.0;
    const double foreLen  = 100.0;

    final vU = vector.Vector3(0, upperLen, 0);
    final vF = vector.Vector3(0, foreLen, 0);
    qUpper.rotate(vU);
    qFore.rotate(vF);

    final elbow = Offset(shoulder.dx + vU.x, shoulder.dy + vU.y);
    final wrist = Offset(elbow.dx + vF.x, elbow.dy + vF.y);

    // Vertical guide line
    canvas.drawLine(
      shoulder,
      Offset(shoulder.dx, shoulder.dy + upperLen + foreLen + 10),
      guidePaint,
    );

    // Bones
    canvas.drawLine(shoulder, elbow, bonePaint);
    canvas.drawLine(elbow,    wrist, bonePaint);

    // Joints
    canvas.drawCircle(shoulder, 9, jointPaint);
    canvas.drawCircle(elbow,    9, jointPaint);
    canvas.drawCircle(wrist,    7, jointPaint);
  }

  @override
  bool shouldRepaint(covariant _TutorialArmPainter old) =>
      old.qUpper != qUpper || old.qFore != qFore || old.accentColor != accentColor;
}
