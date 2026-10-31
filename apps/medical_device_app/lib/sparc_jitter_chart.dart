import 'package:flutter/material.dart';

/// A bar chart matching the Python "Motion Jitter Analysis (SPARC)" graph:
///   • Orange bar  = patient's current smoothness score
///   • Blue bar    = clinical target (default -20)
///   • Dashed red line at the target value
/// SPARC scores are <= 0, so bars grow DOWNWARD from a zero baseline at the top.
class SparcJitterChart extends StatelessWidget {
  final double currentSparc;
  final double clinicalTarget;

  const SparcJitterChart({
    super.key,
    required this.currentSparc,
    this.clinicalTarget = -20.0,
  });

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        const Text(
          'Motion Jitter Analysis (SPARC)',
          style: TextStyle(fontWeight: FontWeight.bold, fontSize: 14),
        ),
        const SizedBox(height: 8),
        SizedBox(
          height: 180,
          width: double.infinity,
          child: CustomPaint(
            painter: _JitterChartPainter(
              currentSparc: currentSparc,
              clinicalTarget: clinicalTarget,
            ),
          ),
        ),
        const SizedBox(height: 6),
        Text(
          currentSparc > clinicalTarget
              ? 'STABLE — smoother than the clinical target.'
              : 'HIGH JITTER — rougher than the clinical target.',
          style: TextStyle(
            fontSize: 12,
            fontWeight: FontWeight.bold,
            color: currentSparc > clinicalTarget
                ? Colors.green
                : Colors.deepOrange,
          ),
        ),
      ],
    );
  }
}

class _JitterChartPainter extends CustomPainter {
  final double currentSparc;
  final double clinicalTarget;

  _JitterChartPainter({required this.currentSparc, required this.clinicalTarget});

  @override
  void paint(Canvas canvas, Size size) {
    const leftPad = 38.0;   // room for y-axis labels
    const bottomPad = 22.0; // room for x-axis labels
    const topPad = 8.0;
    final chartW = size.width - leftPad;
    final chartH = size.height - bottomPad - topPad;

    // Y range: 0 at top down to a little past the most-negative value shown.
    final minVal = [currentSparc, clinicalTarget, 0.0].reduce((a, b) => a < b ? a : b);
    final yMin = (minVal * 1.15).floorToDouble(); // most negative
    const yMax = 0.0;

    double yToPx(double v) =>
        topPad + (yMax - v) / (yMax - yMin) * chartH;

    final axisPaint = Paint()
      ..color = Colors.grey.shade500
      ..strokeWidth = 1;

    // Axes
    canvas.drawLine(Offset(leftPad, topPad), Offset(leftPad, topPad + chartH), axisPaint);
    canvas.drawLine(Offset(leftPad, topPad + chartH),
        Offset(leftPad + chartW, topPad + chartH), axisPaint);

    // Y gridlines + labels
    final textStyle = TextStyle(color: Colors.grey.shade700, fontSize: 10);
    for (int i = 0; i <= 4; i++) {
      final v = yMin + (yMax - yMin) * i / 4;
      final y = yToPx(v);
      final tp = TextPainter(
        text: TextSpan(text: v.toStringAsFixed(0), style: textStyle),
        textDirection: TextDirection.ltr,
      )..layout();
      tp.paint(canvas, Offset(leftPad - tp.width - 4, y - tp.height / 2));
    }

    // Two bars
    final barSlot = chartW / 2;
    const barW = 56.0;
    _drawBar(canvas, leftPad + barSlot * 0.5 - barW / 2, yToPx(0),
        yToPx(currentSparc), barW, const Color(0xFFE67E22)); // orange
    _drawBar(canvas, leftPad + barSlot * 1.5 - barW / 2, yToPx(0),
        yToPx(clinicalTarget), barW, const Color(0xFF2980B9)); // blue

    // Dashed red target line at clinicalTarget
    final dashPaint = Paint()
      ..color = Colors.red
      ..strokeWidth = 1.5;
    final ty = yToPx(clinicalTarget);
    double x = leftPad;
    while (x < leftPad + chartW) {
      canvas.drawLine(Offset(x, ty), Offset(x + 6, ty), dashPaint);
      x += 11;
    }

    // X labels
    _xLabel(canvas, 'Current\nSmoothness', leftPad + barSlot * 0.5, topPad + chartH + 4);
    _xLabel(canvas, 'Clinical\nTarget', leftPad + barSlot * 1.5, topPad + chartH + 4);
  }

  void _drawBar(Canvas canvas, double left, double topZeroY, double valueY,
      double w, Color color) {
    final paint = Paint()..color = color;
    final top = topZeroY < valueY ? topZeroY : valueY;
    final height = (valueY - topZeroY).abs();
    canvas.drawRect(Rect.fromLTWH(left, top, w, height), paint);
  }

  void _xLabel(Canvas canvas, String text, double centerX, double topY) {
    final tp = TextPainter(
      text: TextSpan(
        text: text,
        style: TextStyle(color: Colors.grey.shade800, fontSize: 10),
      ),
      textAlign: TextAlign.center,
      textDirection: TextDirection.ltr,
    )..layout(maxWidth: 90);
    tp.paint(canvas, Offset(centerX - tp.width / 2, topY));
  }

  @override
  bool shouldRepaint(covariant _JitterChartPainter old) =>
      old.currentSparc != currentSparc || old.clinicalTarget != clinicalTarget;
}
