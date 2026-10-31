import 'dart:math' as math;

import 'package:flutter/material.dart';
import 'package:vector_math/vector_math_64.dart' as vector;

import 'app_design.dart';

class ArmTrackerView extends StatelessWidget {
  final String exerciseName;
  final String status;
  final double? angle;
  final int repetitions;
  final vector.Quaternion upper;
  final vector.Quaternion forearm;
  final String feedback;
  final Color feedbackColor;
  final Widget? video;
  final bool videoLoading;
  final String? videoMessage;
  final bool saving;
  final VoidCallback? onConnect;
  final VoidCallback? onSave;
  final VoidCallback? onRetryExercise;
  final VoidCallback onOpenLog;

  const ArmTrackerView({
    super.key,
    required this.exerciseName,
    required this.status,
    required this.angle,
    required this.repetitions,
    required this.upper,
    required this.forearm,
    required this.feedback,
    required this.feedbackColor,
    required this.onOpenLog,
    this.video,
    this.videoLoading = false,
    this.videoMessage,
    this.saving = false,
    this.onConnect,
    this.onSave,
    this.onRetryExercise,
  });

  @override
  Widget build(BuildContext context) {
    final videoPanel = Card(
      margin: EdgeInsets.zero,
      child: Padding(
        padding: const EdgeInsets.all(20),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Text(
              'Movement demonstration',
              style: Theme.of(context).textTheme.titleLarge,
            ),
            const SizedBox(height: 16),
            if (video != null)
              video!
            else
              StatusPanel(
                icon: Icons.video_library_outlined,
                title: videoLoading
                    ? 'Loading demonstration'
                    : 'Video unavailable',
                loading: videoLoading,
                message:
                    videoMessage ??
                    'Sensor tracking can be used without a demonstration video.',
              ),
          ],
        ),
      ),
    );
    final trackerPanel = Card(
      margin: EdgeInsets.zero,
      child: Padding(
        padding: const EdgeInsets.all(20),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Wrap(
              spacing: 40,
              runSpacing: 16,
              children: [
                _metric(
                  context,
                  'Raw angle',
                  angle == null ? '--' : '${angle!.toStringAsFixed(1)}°',
                ),
                _metric(context, 'Repetitions', '$repetitions'),
              ],
            ),
            const SizedBox(height: 16),
            if (angle == null)
              const StatusPanel(
                icon: Icons.sensors_off,
                title: 'Awaiting sensor data',
                message: 'Connect the paired sensors to view orientation.',
              )
            else
              SizedBox(
                height: 280,
                child: Semantics(
                  label: 'Paired arm sensor orientation illustration',
                  image: true,
                  child: CustomPaint(
                    painter: ArmSkeletonPainter(upper, forearm),
                    child: const SizedBox.expand(),
                  ),
                ),
              ),
          ],
        ),
      ),
    );
    return Scaffold(
      appBar: AppBar(
        title: const Text('Arm tracker'),
        actions: [
          IconButton(
            tooltip: 'Open session log',
            icon: const Icon(Icons.receipt_long_outlined),
            onPressed: onOpenLog,
          ),
        ],
      ),
      body: SafeArea(
        child: Align(
          alignment: Alignment.topCenter,
          child: ConstrainedBox(
            constraints: const BoxConstraints(maxWidth: 1120),
            child: SingleChildScrollView(
              padding: const EdgeInsets.all(24),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  Text(
                    exerciseName,
                    style: Theme.of(context).textTheme.headlineMedium,
                  ),
                  const SizedBox(height: 8),
                  Text(
                    status,
                    style: const TextStyle(color: KinetismTheme.secondaryText),
                  ),
                  const SizedBox(height: 20),
                  LayoutBuilder(
                    builder: (context, constraints) =>
                        constraints.maxWidth >= 880
                        ? Row(
                            crossAxisAlignment: CrossAxisAlignment.start,
                            children: [
                              Expanded(child: videoPanel),
                              const SizedBox(width: 20),
                              Expanded(child: trackerPanel),
                            ],
                          )
                        : Column(
                            children: [
                              trackerPanel,
                              const SizedBox(height: 20),
                              videoPanel,
                            ],
                          ),
                  ),
                  const SizedBox(height: 20),
                  Semantics(
                    liveRegion: true,
                    child: Text(
                      feedback,
                      style: TextStyle(color: feedbackColor),
                    ),
                  ),
                  const SizedBox(height: 16),
                  const Text(
                    'Raw, uncalibrated orientation only. Force, patient-specific measurements and MMT grading are not implemented in this prototype.',
                    style: TextStyle(color: KinetismTheme.secondaryText),
                  ),
                  const SizedBox(height: 20),
                  Wrap(
                    spacing: 12,
                    runSpacing: 12,
                    children: [
                      if (onRetryExercise != null)
                        FilledButton.icon(
                          onPressed: onRetryExercise,
                          icon: const Icon(Icons.refresh),
                          label: const Text('Retry exercise'),
                        ),
                      OutlinedButton.icon(
                        onPressed: onConnect,
                        icon: const Icon(Icons.bluetooth),
                        label: Text(
                          status == 'Scanning...'
                              ? 'Scanning for sensors'
                              : 'Connect sensors',
                        ),
                      ),
                      FilledButton.icon(
                        onPressed: saving ? null : onSave,
                        icon: const Icon(Icons.save_outlined),
                        label: Text(
                          saving
                              ? 'Waiting for cloud save'
                              : 'End & save session',
                        ),
                      ),
                    ],
                  ),
                ],
              ),
            ),
          ),
        ),
      ),
    );
  }

  Widget _metric(BuildContext context, String label, String value) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(label, style: const TextStyle(color: KinetismTheme.secondaryText)),
        const SizedBox(height: 4),
        Text(value, style: Theme.of(context).textTheme.headlineMedium),
      ],
    );
  }
}

class ArmSkeletonPainter extends CustomPainter {
  final vector.Quaternion qUpper;
  final vector.Quaternion qFore;

  ArmSkeletonPainter(this.qUpper, this.qFore);

  @override
  void paint(Canvas canvas, Size size) {
    final bonePaint = Paint()
      ..color = KinetismTheme.accent
      ..strokeWidth = 10
      ..strokeCap = StrokeCap.round;
    final jointPaint = Paint()..color = Colors.white;
    final upper = vector.Vector3(0, 180, 0);
    final forearm = vector.Vector3(0, 160, 0);
    qUpper.rotate(upper);
    qFore.rotate(forearm);
    final points = [
      Offset.zero,
      Offset(upper.x, upper.y),
      Offset(upper.x + forearm.x, upper.y + forearm.y),
    ];
    final minX = points.map((point) => point.dx).reduce(math.min);
    final maxX = points.map((point) => point.dx).reduce(math.max);
    final minY = points.map((point) => point.dy).reduce(math.min);
    final maxY = points.map((point) => point.dy).reduce(math.max);
    final scale = math.max(
      0.0,
      math.min(
        (size.width - 48) / math.max(1, maxX - minX),
        (size.height - 48) / math.max(1, maxY - minY),
      ),
    );
    final center = Offset((minX + maxX) / 2, (minY + maxY) / 2);
    Offset project(Offset point) =>
        Offset(size.width / 2, size.height / 2) + (point - center) * scale;
    final shoulder = project(points[0]);
    final elbow = project(points[1]);
    final wrist = project(points[2]);
    canvas.drawLine(shoulder, elbow, bonePaint);
    canvas.drawLine(elbow, wrist, bonePaint);
    for (final joint in [shoulder, elbow, wrist]) {
      canvas.drawCircle(joint, 7, jointPaint);
    }
  }

  @override
  bool shouldRepaint(covariant ArmSkeletonPainter oldDelegate) =>
      qUpper != oldDelegate.qUpper || qFore != oldDelegate.qFore;
}
