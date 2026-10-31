import 'package:cloud_firestore/cloud_firestore.dart';
import 'package:flutter/material.dart';
import 'package:intl/intl.dart';

import 'app_design.dart';
import 'report_service.dart';

class ArchivedReport {
  final DateTime? date;
  final String? analysis;

  const ArchivedReport({this.date, this.analysis});

  factory ArchivedReport.fromMap(Map<String, dynamic> data) {
    final date = data['date'];
    final analysis = data['aiAnalysis'];
    return ArchivedReport(
      date: date is DateTime ? date.toLocal() : null,
      analysis: analysis is String && analysis.trim().isNotEmpty
          ? analysis
          : null,
    );
  }
}

class PastReportsScreen extends StatefulWidget {
  final Stream<List<ArchivedReport>> Function()? loadReports;
  final ValueChanged<ArchivedReport>? onOpen;

  const PastReportsScreen({super.key, this.loadReports, this.onOpen});

  @override
  State<PastReportsScreen> createState() => _PastReportsScreenState();
}

class _PastReportsScreenState extends State<PastReportsScreen> {
  late Stream<List<ArchivedReport>> _reports;

  @override
  void initState() {
    super.initState();
    _subscribe();
  }

  void _subscribe() {
    _reports =
        widget.loadReports?.call() ??
        FirebaseFirestore.instance
            .collection('WeeklyReports')
            .orderBy('date', descending: true)
            .snapshots()
            .map(
              (snapshot) => snapshot.docs.map((doc) {
                final data = Map<String, dynamic>.of(doc.data());
                final date = data['date'];
                if (date is Timestamp) data['date'] = date.toDate();
                return ArchivedReport.fromMap(data);
              }).toList(),
            );
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(title: const Text('Report archive')),
      body: SafeArea(
        child: StreamBuilder<List<ArchivedReport>>(
          stream: _reports,
          builder: (context, snapshot) {
            if (snapshot.hasError) {
              return StatusPanel(
                icon: Icons.cloud_off,
                title: 'Reports unavailable',
                message:
                    'Could not load archived reports. Check the connection and database access, then retry.',
                actionLabel: 'Retry loading',
                onAction: () => setState(_subscribe),
              );
            }
            if (snapshot.connectionState == ConnectionState.waiting) {
              return const StatusPanel(
                icon: Icons.description_outlined,
                title: 'Loading reports',
                message: 'Fetching archived report drafts.',
                loading: true,
              );
            }
            final reports = snapshot.data ?? [];
            if (reports.isEmpty) {
              return const StatusPanel(
                icon: Icons.description_outlined,
                title: 'No archived reports',
                message:
                    'Generate a weekly report from the session log after recording exercise sessions.',
              );
            }
            return Center(
              child: ConstrainedBox(
                constraints: const BoxConstraints(maxWidth: 880),
                child: ListView.separated(
                  padding: const EdgeInsets.all(24),
                  itemCount: reports.length + 1,
                  separatorBuilder: (_, index) => index == 0
                      ? const SizedBox(height: 20)
                      : const Divider(height: 1),
                  itemBuilder: (context, index) {
                    if (index == 0) {
                      return const NoticeBanner(
                        icon: Icons.info_outline,
                        message:
                            'AI-generated report drafts require therapist review. They are not confirmed MMT assessments.',
                      );
                    }
                    final report = reports[index - 1];
                    final available =
                        report.date != null && report.analysis != null;
                    return ListTile(
                      contentPadding: const EdgeInsets.symmetric(vertical: 12),
                      leading: Icon(
                        available
                            ? Icons.description_outlined
                            : Icons.warning_amber,
                        color: KinetismTheme.accent,
                      ),
                      title: const Text('Weekly report draft'),
                      subtitle: Text(
                        report.date == null
                            ? 'Report date unavailable'
                            : '${DateFormat('d MMM yyyy, HH:mm').format(report.date!)}'
                                  '${report.analysis == null ? '\nReport content unavailable' : ''}',
                        style: const TextStyle(
                          color: KinetismTheme.secondaryText,
                        ),
                      ),
                      trailing: available
                          ? const Icon(Icons.chevron_right)
                          : null,
                      enabled: available,
                      onTap: !available
                          ? null
                          : () {
                              if (widget.onOpen != null) {
                                widget.onOpen!(report);
                              } else {
                                ReportService.showSavedReport(
                                  context,
                                  report.analysis!,
                                  report.date!,
                                );
                              }
                            },
                    );
                  },
                ),
              ),
            );
          },
        ),
      ),
    );
  }
}
