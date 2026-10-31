import 'package:flutter/material.dart';
import 'dart:typed_data';
import 'package:pdf/pdf.dart';
import 'package:pdf/widgets.dart' as pw;
import 'package:printing/printing.dart';
import 'package:cloud_firestore/cloud_firestore.dart';
import 'package:intl/intl.dart';
import 'ai_coach.dart';

// Professional Blue matching teammate's Python report: RGB(41, 128, 185)
const _headerBlue = PdfColor(0.161, 0.502, 0.725);
const _tableGrey = PdfColor(0.902, 0.902, 0.902);
const _darkText = PdfColor(0.173, 0.243, 0.314);

class ReportService {

  static Future<void> generateWeeklyReport(BuildContext context, List<QueryDocumentSnapshot> logs) async {
    showDialog(
      context: context,
      barrierDismissible: false,
      builder: (context) => const Center(child: CircularProgressIndicator(color: Colors.cyanAccent)),
    );

    final DateTime now = DateTime.now();
    String dateString = DateFormat('dd/MM/yyyy | HH:mm').format(now);
    String coachFeedback = "";

    // Compute session stats from logs
    int totalReps = logs.fold(0, (acc, doc) {
      var d = doc.data() as Map<String, dynamic>;
      return acc + ((d['repsCompleted'] as num?) ?? 0).toInt();
    });
    int numSessions = logs.length;
    double avgRepsPerSession = numSessions > 0 ? totalReps / numSessions : 0;
    double recoveryRate = numSessions > 0
        ? (totalReps / (numSessions * 50) * 100).clamp(0.0, 100.0)
        : 0.0;

    // Find primary exercise (highest rep count)
    Map<String, int> exerciseCounts = {};
    for (var doc in logs) {
      var d = doc.data() as Map<String, dynamic>;
      String name = (d['exerciseName'] as String?) ?? 'Unknown';
      exerciseCounts[name] = (exerciseCounts[name] ?? 0) + ((d['repsCompleted'] as num?) ?? 0).toInt();
    }
    String primaryExercise = exerciseCounts.isNotEmpty
        ? exerciseCounts.entries.reduce((a, b) => a.value > b.value ? a : b).key
        : 'None';

    try {
      // Check Firebase cache for a report generated in the last 7 days
      final oneWeekAgo = now.subtract(const Duration(days: 7));
      final recentReportsQuery = await FirebaseFirestore.instance
          .collection('WeeklyReports')
          .where('patientId', isEqualTo: 'patient_123')
          .where('date', isGreaterThanOrEqualTo: Timestamp.fromDate(oneWeekAgo))
          .orderBy('date', descending: true)
          .limit(1)
          .get();

      if (recentReportsQuery.docs.isNotEmpty) {
        var savedData = recentReportsQuery.docs.first.data();
        coachFeedback = savedData['aiAnalysis'] ?? "";
        DateTime savedDate = (savedData['date'] as Timestamp).toDate();
        dateString = "${DateFormat('dd/MM/yyyy | HH:mm').format(savedDate)} (Saved)";
      } else {
        // No cached report — call AI coach and save result
        coachFeedback = await AiCoach.generateSessionFeedback(
          totalReps: totalReps,
          numSessions: numSessions,
          primaryExercise: primaryExercise,
        );
        await FirebaseFirestore.instance.collection('WeeklyReports').add({
          'patientId': 'patient_123',
          'date': FieldValue.serverTimestamp(),
          'aiAnalysis': coachFeedback,
        });
      }

      final pdf = pw.Document();
      _buildStructuredPdf(
        pdf: pdf,
        dateString: dateString,
        totalReps: totalReps,
        numSessions: numSessions,
        avgRepsPerSession: avgRepsPerSession,
        recoveryRate: recoveryRate,
        primaryExercise: primaryExercise,
        coachFeedback: coachFeedback,
      );
      final Uint8List pdfBytes = await pdf.save();

      if (context.mounted) Navigator.pop(context);
      if (context.mounted) {
        _showPdfViewer(context, pdfBytes, "WEEKLY REPORT", "IDP_Report.pdf");
      }
    } catch (e) {
      if (context.mounted) {
        Navigator.pop(context);
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text("Error: $e"), backgroundColor: Colors.red),
        );
      }
    }
  }

  static Future<void> showSavedReport(BuildContext context, String coachFeedback, DateTime date) async {
    final String dateString = DateFormat('dd/MM/yyyy | HH:mm').format(date);
    final pdf = pw.Document();

    // Show structured report with saved coach feedback; stats shown as N/A
    _buildStructuredPdf(
      pdf: pdf,
      dateString: dateString,
      totalReps: null,
      numSessions: null,
      avgRepsPerSession: null,
      recoveryRate: null,
      primaryExercise: null,
      coachFeedback: coachFeedback,
    );

    final Uint8List pdfBytes = await pdf.save();
    if (!context.mounted) return;
    _showPdfViewer(context, pdfBytes, "SAVED REPORT", "IDP_Archive_Report.pdf");
  }

  // Builds the structured PDF matching the teammate's Python report format
  static void _buildStructuredPdf({
    required pw.Document pdf,
    required String dateString,
    required int? totalReps,
    required int? numSessions,
    required double? avgRepsPerSession,
    required double? recoveryRate,
    required String? primaryExercise,
    required String coachFeedback,
  }) {
    // Helper: table cell widget
    pw.Widget cell(String text, {bool bold = false, bool center = false}) {
      return pw.Padding(
        padding: const pw.EdgeInsets.all(4),
        child: pw.Text(
          text,
          style: bold
              ? pw.TextStyle(fontWeight: pw.FontWeight.bold, fontSize: 9)
              : const pw.TextStyle(fontSize: 9),
          textAlign: center ? pw.TextAlign.center : pw.TextAlign.left,
        ),
      );
    }

    final String totalRepsStr = totalReps != null ? '$totalReps' : 'N/A';
    final String numSessionsStr = numSessions != null ? '$numSessions' : 'N/A';
    final String avgRepsStr = avgRepsPerSession != null ? avgRepsPerSession.toStringAsFixed(1) : 'N/A';
    final String exerciseStr = primaryExercise ?? 'N/A';
    final String recoveryStr = recoveryRate != null ? '${recoveryRate.toStringAsFixed(1)}%' : 'N/A';

    pdf.addPage(
      pw.MultiPage(
        pageFormat: PdfPageFormat.a4,
        // Zero horizontal/top margins so the blue header reaches page edges
        margin: const pw.EdgeInsets.only(bottom: 25),
        footer: (pw.Context ctx) => pw.Container(
          alignment: pw.Alignment.center,
          padding: const pw.EdgeInsets.only(top: 4),
          child: pw.Text(
            'Confidential Patient Report - Page ${ctx.pageNumber}',
            style: const pw.TextStyle(color: PdfColors.grey, fontSize: 8),
          ),
        ),
        build: (pw.Context ctx) => [
          // ── HEADER (full-width blue bar matching Python report) ──
          pw.Container(
            width: double.infinity,
            decoration: const pw.BoxDecoration(color: _headerBlue),
            padding: const pw.EdgeInsets.symmetric(vertical: 14, horizontal: 10),
            child: pw.Column(
              crossAxisAlignment: pw.CrossAxisAlignment.center,
              children: [
                pw.Text(
                  'ELBOW REHAB MOTION ANALYSIS',
                  style: pw.TextStyle(
                    fontSize: 16,
                    fontWeight: pw.FontWeight.bold,
                    color: PdfColors.white,
                  ),
                  textAlign: pw.TextAlign.center,
                ),
                pw.SizedBox(height: 3),
                pw.Text(
                  'AI-Assisted Clinical Performance Review',
                  style: pw.TextStyle(
                    fontSize: 8,
                    fontStyle: pw.FontStyle.italic,
                    color: PdfColors.white,
                  ),
                  textAlign: pw.TextAlign.center,
                ),
              ],
            ),
          ),

          pw.Padding(
            padding: const pw.EdgeInsets.fromLTRB(20, 12, 20, 0),
            child: pw.Column(
              crossAxisAlignment: pw.CrossAxisAlignment.start,
              children: [

                // ── PATIENT & SESSION INFO ──
                pw.Row(
                  mainAxisAlignment: pw.MainAxisAlignment.spaceBetween,
                  children: [
                    pw.Text('PATIENT ID: patient_123',
                        style: pw.TextStyle(fontWeight: pw.FontWeight.bold, fontSize: 11, color: _darkText)),
                    pw.Text('SESSION DATE: $dateString',
                        style: pw.TextStyle(fontWeight: pw.FontWeight.bold, fontSize: 11, color: _darkText)),
                  ],
                ),
                pw.Divider(),
                pw.SizedBox(height: 8),

                // ── SECTION I: SESSION METRICS TABLE ──
                pw.Text('I. STITCHED SESSION METRICS',
                    style: pw.TextStyle(fontWeight: pw.FontWeight.bold, fontSize: 10)),
                pw.SizedBox(height: 5),
                pw.Table(
                  border: pw.TableBorder.all(color: PdfColors.grey),
                  columnWidths: const {
                    0: pw.FractionColumnWidth(0.25),
                    1: pw.FractionColumnWidth(0.20),
                    2: pw.FractionColumnWidth(0.55),
                  },
                  children: [
                    pw.TableRow(
                      decoration: const pw.BoxDecoration(color: _tableGrey),
                      children: [
                        cell('Metric', bold: true, center: true),
                        cell('Value', bold: true, center: true),
                        cell('Clinical Significance', bold: true, center: true),
                      ],
                    ),
                    pw.TableRow(children: [
                      cell(' Total Reps'),
                      cell(totalRepsStr, center: true),
                      cell(' Total volume of exercise completed.'),
                    ]),
                    pw.TableRow(children: [
                      cell(' Sessions'),
                      cell(numSessionsStr, center: true),
                      cell(' Number of training sessions completed.'),
                    ]),
                    pw.TableRow(children: [
                      cell(' Avg Reps / Session'),
                      cell(avgRepsStr, center: true),
                      cell(' Consistent performance indicator.'),
                    ]),
                    pw.TableRow(children: [
                      cell(' Primary Exercise'),
                      cell(exerciseStr, center: true),
                      cell(' Most frequently performed exercise.'),
                    ]),
                  ],
                ),
                pw.SizedBox(height: 12),

                // ── SECTION II: RECOVERY & FORM CONSISTENCY ──
                pw.Text('II. RECOVERY & FORM CONSISTENCY',
                    style: pw.TextStyle(fontWeight: pw.FontWeight.bold, fontSize: 10)),
                pw.SizedBox(height: 5),
                pw.Text(
                  '* CURRENT RECOVERY STATUS: $recoveryStr towards Weekly Goal',
                  style: const pw.TextStyle(fontSize: 9),
                ),
                pw.SizedBox(height: 12),

                // ── SECTION III: MOTION JITTERING & STABILITY ──
                pw.Text('III. MOTION JITTERING & STABILITY',
                    style: pw.TextStyle(fontWeight: pw.FontWeight.bold, fontSize: 10)),
                pw.SizedBox(height: 5),
                pw.Text(
                  '* MOTION STABILITY: Data not available (requires real-time sensor SPARC analysis)',
                  style: const pw.TextStyle(fontSize: 9),
                ),
                pw.SizedBox(height: 12),

                // ── SECTION IV: AI COACH RECOMMENDATION ──
                pw.Text('IV. AI COACH CLINICAL RECOMMENDATION',
                    style: pw.TextStyle(fontWeight: pw.FontWeight.bold, fontSize: 10)),
                pw.SizedBox(height: 5),
                pw.Container(
                  width: double.infinity,
                  padding: const pw.EdgeInsets.all(8),
                  decoration: pw.BoxDecoration(border: pw.Border.all(color: PdfColors.grey)),
                  child: pw.Text(
                    '"$coachFeedback"',
                    style: pw.TextStyle(fontStyle: pw.FontStyle.italic, fontSize: 10),
                  ),
                ),
              ],
            ),
          ),
        ],
      ),
    );
  }

  // Shows the PDF in a bottom sheet viewer
  static void _showPdfViewer(BuildContext context, Uint8List pdfBytes, String title, String fileName) {
    showModalBottomSheet(
      context: context,
      isScrollControlled: true,
      backgroundColor: Colors.transparent,
      builder: (context) => Container(
        height: MediaQuery.of(context).size.height * 0.85,
        decoration: const BoxDecoration(
          color: Color(0xFF1A1A1A),
          borderRadius: BorderRadius.vertical(top: Radius.circular(25)),
        ),
        padding: const EdgeInsets.all(10),
        child: Column(
          children: [
            Container(
              width: 40, height: 5,
              margin: const EdgeInsets.only(bottom: 10),
              decoration: BoxDecoration(color: Colors.grey, borderRadius: BorderRadius.circular(10)),
            ),
            Text(title, style: const TextStyle(color: Colors.white, fontWeight: FontWeight.bold)),
            const SizedBox(height: 10),
            Expanded(
              child: PdfPreview(
                build: (format) => pdfBytes,
                allowPrinting: true,
                allowSharing: true,
                initialPageFormat: PdfPageFormat.a4,
                pdfFileName: fileName,
              ),
            ),
          ],
        ),
      ),
    );
  }
}
