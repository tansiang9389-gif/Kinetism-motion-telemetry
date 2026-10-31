import 'package:cloud_firestore/cloud_firestore.dart';
import 'package:fl_chart/fl_chart.dart';
import 'package:flutter/material.dart';
import 'package:intl/intl.dart';

import 'add_exercise.dart';
import 'app_design.dart';
import 'past_reports.dart';
import 'prescriptions.dart';
import 'rehab_tutorial.dart';
import 'report_service.dart';
import 'select_exercise.dart';
import 'session_log.dart';

class TherapistDashboard extends StatefulWidget {
  const TherapistDashboard({super.key});

  @override
  State<TherapistDashboard> createState() => _TherapistDashboardState();
}

class _TherapistDashboardState extends State<TherapistDashboard> {
  late Stream<QuerySnapshot<Map<String, dynamic>>> _logs;

  @override
  void initState() {
    super.initState();
    _subscribe();
  }

  void _subscribe() {
    _logs = FirebaseFirestore.instance
        .collection('SessionLogs')
        .orderBy('date', descending: true)
        .snapshots(includeMetadataChanges: true);
  }

  void _open(Widget screen) {
    Navigator.push(context, MaterialPageRoute<void>(builder: (_) => screen));
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(title: const KinetismBrand()),
      drawer: Drawer(
        child: SafeArea(
          child: ListView(
            padding: const EdgeInsets.symmetric(vertical: 16),
            children: [
              const Padding(
                padding: EdgeInsets.all(24),
                child: KinetismBrand(),
              ),
              const ListTile(
                selected: true,
                leading: Icon(Icons.history),
                title: Text('Session log'),
                subtitle: Text('Shared prototype activity'),
              ),
              _destination(
                'Start exercise',
                Icons.sensors,
                const SelectExerciseScreen(),
              ),
              _destination(
                'Add exercise',
                Icons.add,
                const AddExerciseScreen(),
              ),
              const Divider(),
              _destination(
                'Report archive',
                Icons.description_outlined,
                const PastReportsScreen(),
              ),
              _destination(
                'Exercise goals & reminders',
                Icons.alarm,
                const PrescriptionsScreen(),
              ),
              _destination(
                'Movement tutorial',
                Icons.self_improvement,
                const RehabTutorialScreen(),
              ),
            ],
          ),
        ),
      ),
      body: StreamBuilder<QuerySnapshot<Map<String, dynamic>>>(
        stream: _logs,
        builder: (context, snapshot) {
          final docs = snapshot.data?.docs ?? [];
          final logs = docs.map((doc) {
            final data = Map<String, dynamic>.of(doc.data());
            final date = data['date'];
            if (date is Timestamp) data['date'] = date.toDate();
            return SessionLog.fromMap(doc.id, data);
          }).toList();
          return SessionLogView(
            logs: logs,
            loading: snapshot.connectionState == ConnectionState.waiting,
            errorMessage: snapshot.hasError
                ? 'Could not load sessions. Check your connection and database access.'
                : null,
            fromCache: snapshot.data?.metadata.isFromCache ?? false,
            pendingWrites: snapshot.data?.metadata.hasPendingWrites ?? false,
            onRetry: () => setState(_subscribe),
            onStartExercise: () => _open(const SelectExerciseScreen()),
            onReport: (ids) {
              final selected = ids.toSet();
              ReportService.generateWeeklyReport(
                context,
                docs.where((doc) => selected.contains(doc.id)).toList(),
              );
            },
          );
        },
      ),
    );
  }

  Widget _destination(String title, IconData icon, Widget screen) {
    return ListTile(
      leading: Icon(icon),
      title: Text(title),
      onTap: () {
        Navigator.pop(context);
        _open(screen);
      },
    );
  }
}

class SessionLogView extends StatefulWidget {
  final List<SessionLog> logs;
  final bool loading;
  final String? errorMessage;
  final bool fromCache;
  final bool pendingWrites;
  final bool preview;
  final VoidCallback onStartExercise;
  final VoidCallback onRetry;
  final ValueChanged<List<String>>? onReport;
  final DateTime? now;

  const SessionLogView({
    super.key,
    required this.logs,
    required this.onStartExercise,
    required this.onRetry,
    this.onReport,
    this.loading = false,
    this.errorMessage,
    this.fromCache = false,
    this.pendingWrites = false,
    this.preview = false,
    this.now,
  });

  @override
  State<SessionLogView> createState() => _SessionLogViewState();
}

class _SessionLogViewState extends State<SessionLogView> {
  String? _exercise;
  String _query = '';
  final _search = TextEditingController();

  @override
  void dispose() {
    _search.dispose();
    super.dispose();
  }

  void _clearFilters({bool resetExercise = true}) {
    _search.clear();
    setState(() {
      if (resetExercise) _exercise = null;
      _query = '';
    });
  }

  @override
  Widget build(BuildContext context) {
    final exercises =
        widget.logs.map((log) => log.exerciseName).toSet().toList()..sort();
    final selected = exercises.contains(_exercise) ? _exercise : null;
    final filtered = widget.logs
        .where(
          (log) =>
              (selected == null || log.exerciseName == selected) &&
              log.matches(_query),
        )
        .toList();
    final activity = SessionActivity.fromLogs(
      filtered,
      widget.now ?? DateTime.now(),
    );
    return SafeArea(
      child: Center(
        child: ConstrainedBox(
          constraints: const BoxConstraints(maxWidth: 1120),
          child: CustomScrollView(
            slivers: [
              SliverPadding(
                padding: const EdgeInsets.fromLTRB(24, 16, 24, 0),
                sliver: SliverToBoxAdapter(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      LayoutBuilder(
                        builder: (context, constraints) {
                          final title = Column(
                            crossAxisAlignment: CrossAxisAlignment.start,
                            children: [
                              Semantics(
                                header: true,
                                child: Text(
                                  'Session log',
                                  style: Theme.of(
                                    context,
                                  ).textTheme.headlineMedium,
                                ),
                              ),
                              const SizedBox(height: 4),
                              const Text(
                                'Review recorded exercise activity.',
                                style: TextStyle(
                                  color: KinetismTheme.secondaryText,
                                ),
                              ),
                            ],
                          );
                          final start = FilledButton.icon(
                            onPressed: widget.onStartExercise,
                            icon: const Icon(Icons.sensors),
                            label: const Text('Start exercise'),
                          );
                          if (constraints.maxWidth < 620) {
                            return Column(
                              crossAxisAlignment: CrossAxisAlignment.stretch,
                              children: [
                                title,
                                const SizedBox(height: 20),
                                start,
                              ],
                            );
                          }
                          return Row(
                            children: [
                              Expanded(child: title),
                              const SizedBox(width: 24),
                              start,
                            ],
                          );
                        },
                      ),
                      NoticeBanner(
                        icon: widget.preview
                            ? Icons.science_outlined
                            : Icons.info_outline,
                        message: widget.preview
                            ? 'Synthetic records, not patient measurements.'
                            : 'Shared prototype data. Patient selection, calibrated force and MMT assessments are not yet implemented.',
                      ),
                      if (widget.errorMessage != null)
                        NoticeBanner(
                          icon: Icons.cloud_off,
                          message: widget.errorMessage!,
                          isError: true,
                          onRetry: widget.onRetry,
                        ),
                      if (widget.fromCache || widget.pendingWrites)
                        NoticeBanner(
                          icon: Icons.cloud_queue,
                          message: [
                            if (widget.fromCache)
                              'Showing cached records. They may not include the latest sessions.',
                            if (widget.pendingWrites)
                              'Changes are pending cloud confirmation. This is not a backup.',
                          ].join(' '),
                        ),
                      const SizedBox(height: 12),
                      LayoutBuilder(
                        builder: (context, constraints) {
                          final search = TextField(
                            controller: _search,
                            onChanged: (value) =>
                                setState(() => _query = value),
                            decoration: InputDecoration(
                              labelText: 'Search sessions',
                              hintText: 'Exercise or stored patient ID',
                              prefixIcon: const Icon(Icons.search),
                              suffixIcon: _query.isEmpty
                                  ? null
                                  : IconButton(
                                      tooltip: 'Clear search',
                                      onPressed: () =>
                                          _clearFilters(resetExercise: false),
                                      icon: const Icon(Icons.close),
                                    ),
                            ),
                          );
                          final filter = DropdownButtonFormField<String>(
                            key: ValueKey(selected),
                            initialValue: selected,
                            isExpanded: true,
                            decoration: const InputDecoration(
                              labelText: 'Exercise filter',
                            ),
                            items: [
                              const DropdownMenuItem(
                                value: null,
                                child: Text('All exercises'),
                              ),
                              ...exercises.map(
                                (name) => DropdownMenuItem(
                                  value: name,
                                  child: Text(
                                    name,
                                    overflow: TextOverflow.ellipsis,
                                  ),
                                ),
                              ),
                            ],
                            onChanged: (value) =>
                                setState(() => _exercise = value),
                          );
                          if (constraints.maxWidth < 620) {
                            return Column(
                              children: [
                                search,
                                const SizedBox(height: 16),
                                filter,
                              ],
                            );
                          }
                          return Row(
                            children: [
                              Expanded(flex: 2, child: search),
                              const SizedBox(width: 16),
                              Expanded(child: filter),
                            ],
                          );
                        },
                      ),
                      if (filtered.isNotEmpty) ...[
                        const SizedBox(height: 28),
                        Wrap(
                          spacing: 40,
                          runSpacing: 20,
                          children: [
                            _metric(
                              'Sessions today',
                              '${activity.sessionsToday}',
                            ),
                            _metric(
                              'Recorded reps today',
                              '${activity.recordedRepetitionsToday}',
                            ),
                            _metric('Matching sessions', '${filtered.length}'),
                          ],
                        ),
                        if (activity.missingCountsToday > 0 ||
                            activity.missingDates > 0)
                          NoticeBanner(
                            icon: Icons.warning_amber,
                            message: [
                              if (activity.missingCountsToday > 0)
                                '${activity.missingCountsToday} session${activity.missingCountsToday == 1 ? '' : 's'} today '
                                    '${activity.missingCountsToday == 1 ? 'has' : 'have'} no valid rep count.',
                              if (activity.missingDates > 0)
                                '${activity.missingDates} session date${activity.missingDates == 1 ? '' : 's'} '
                                    '${activity.missingDates == 1 ? 'is' : 'are'} unavailable.',
                              'Missing values are not counted as zero or today.',
                            ].join(' '),
                          ),
                        const SizedBox(height: 28),
                        _chart(filtered),
                        const SizedBox(height: 24),
                        Wrap(
                          alignment: WrapAlignment.spaceBetween,
                          crossAxisAlignment: WrapCrossAlignment.center,
                          spacing: 24,
                          runSpacing: 12,
                          children: [
                            Text(
                              'Recorded sessions',
                              style: Theme.of(context).textTheme.titleLarge,
                            ),
                            OutlinedButton.icon(
                              onPressed:
                                  widget.onReport == null ||
                                      widget.loading ||
                                      widget.errorMessage != null
                                  ? null
                                  : () => widget.onReport!(
                                      filtered.map((log) => log.id).toList(),
                                    ),
                              icon: const Icon(Icons.description_outlined),
                              label: const Text('Generate weekly report'),
                            ),
                          ],
                        ),
                        const SizedBox(height: 12),
                      ],
                    ],
                  ),
                ),
              ),
              if (widget.loading && widget.logs.isEmpty)
                const SliverFillRemaining(
                  hasScrollBody: false,
                  child: StatusPanel(
                    icon: Icons.history,
                    title: 'Loading sessions',
                    message: 'Fetching the session log.',
                    loading: true,
                  ),
                )
              else if (filtered.isEmpty)
                SliverFillRemaining(
                  hasScrollBody: false,
                  child: StatusPanel(
                    icon: widget.errorMessage != null
                        ? Icons.cloud_off
                        : Icons.history,
                    title: widget.errorMessage != null
                        ? 'Session log unavailable'
                        : widget.logs.isEmpty
                        ? 'No sessions yet'
                        : 'No matching sessions',
                    message: widget.errorMessage != null
                        ? 'Existing records have not been deleted. Retry when the connection or database access is restored.'
                        : widget.logs.isEmpty
                        ? 'Start an exercise and save a session to see it here.'
                        : 'Try another exercise or clear your search.',
                    actionLabel: widget.errorMessage != null
                        ? 'Retry loading'
                        : widget.logs.isEmpty
                        ? null
                        : 'Clear filters',
                    onAction: widget.errorMessage != null
                        ? widget.onRetry
                        : widget.logs.isEmpty
                        ? null
                        : _clearFilters,
                  ),
                )
              else
                SliverPadding(
                  padding: const EdgeInsets.fromLTRB(24, 0, 24, 32),
                  sliver: SliverList.builder(
                    itemCount: filtered.length,
                    itemBuilder: (context, index) =>
                        _sessionRow(filtered[index]),
                  ),
                ),
            ],
          ),
        ),
      ),
    );
  }

  Widget _metric(String label, String value) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(label, style: const TextStyle(color: KinetismTheme.secondaryText)),
        const SizedBox(height: 4),
        Text(
          value,
          style: const TextStyle(
            fontSize: 32,
            fontWeight: FontWeight.w600,
            fontFeatures: [FontFeature.tabularFigures()],
          ),
        ),
      ],
    );
  }

  Widget _chart(List<SessionLog> logs) {
    final chartLogs = logs
        .where((log) => log.repetitions != null)
        .take(7)
        .toList()
        .reversed
        .toList();
    if (chartLogs.isEmpty) return const SizedBox.shrink();
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(
          'Reps per session',
          style: Theme.of(context).textTheme.titleMedium,
        ),
        const SizedBox(height: 4),
        const Text(
          'Most recent 7 sessions with recorded counts.',
          style: TextStyle(color: KinetismTheme.secondaryText),
        ),
        const SizedBox(height: 20),
        Semantics(
          label: chartLogs
              .map((log) => '${log.exerciseName}: ${log.repetitions} reps')
              .join(', '),
          child: ExcludeSemantics(
            child: SizedBox(
              height: 160,
              child: BarChart(
                BarChartData(
                  minY: 0,
                  barTouchData: BarTouchData(enabled: false),
                  titlesData: FlTitlesData(
                    leftTitles: AxisTitles(
                      sideTitles: SideTitles(
                        showTitles: true,
                        reservedSize: 36,
                        getTitlesWidget: (value, meta) =>
                            value == value.roundToDouble()
                            ? Text(
                                value.toInt().toString(),
                                style: const TextStyle(fontSize: 12),
                              )
                            : const SizedBox.shrink(),
                      ),
                    ),
                    bottomTitles: const AxisTitles(
                      sideTitles: SideTitles(showTitles: false),
                    ),
                    topTitles: const AxisTitles(
                      sideTitles: SideTitles(showTitles: false),
                    ),
                    rightTitles: const AxisTitles(
                      sideTitles: SideTitles(showTitles: false),
                    ),
                  ),
                  borderData: FlBorderData(show: false),
                  gridData: const FlGridData(drawVerticalLine: false),
                  barGroups: [
                    for (var index = 0; index < chartLogs.length; index++)
                      BarChartGroupData(
                        x: index,
                        barRods: [
                          BarChartRodData(
                            toY: chartLogs[index].repetitions!.toDouble(),
                            color: KinetismTheme.accent,
                            width: 18,
                            borderRadius: BorderRadius.circular(4),
                          ),
                        ],
                      ),
                  ],
                ),
              ),
            ),
          ),
        ),
      ],
    );
  }

  Widget _sessionRow(SessionLog log) {
    final date = log.recordedAt == null
        ? 'Date unavailable'
        : DateFormat('d MMM yyyy, HH:mm').format(log.recordedAt!);
    return Column(
      children: [
        ListTile(
          contentPadding: const EdgeInsets.symmetric(vertical: 8),
          title: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(log.exerciseName),
              const SizedBox(height: 4),
              Wrap(
                spacing: 16,
                runSpacing: 4,
                children: [
                  Text(
                    date,
                    style: const TextStyle(color: KinetismTheme.secondaryText),
                  ),
                  Text(
                    log.repetitions == null
                        ? 'Not recorded'
                        : '${log.repetitions} reps',
                    style: TextStyle(
                      fontWeight: FontWeight.w600,
                      color: log.repetitions == null
                          ? Theme.of(context).colorScheme.error
                          : KinetismTheme.accent,
                      fontSize: 16,
                      fontFeatures: const [FontFeature.tabularFigures()],
                    ),
                  ),
                ],
              ),
            ],
          ),
          trailing: const Icon(Icons.chevron_right),
          onTap: () => showDialog<void>(
            context: context,
            builder: (context) => AlertDialog(
              title: Text(log.exerciseName),
              content: SingleChildScrollView(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    Text(date),
                    const SizedBox(height: 12),
                    Text(
                      log.repetitions == null
                          ? 'Repetition count unavailable'
                          : 'Recorded repetitions: ${log.repetitions}',
                    ),
                    const SizedBox(height: 12),
                    Text(
                      'Stored patient ID: ${log.patientId ?? 'Not assigned'}',
                    ),
                    const SizedBox(height: 12),
                    const Text(
                      'Legacy IDs are not proof of patient identity. These sessions do not contain a force measurement or confirmed MMT grade.',
                    ),
                    ...log.issues.map(
                      (issue) => Padding(
                        padding: const EdgeInsets.only(top: 8),
                        child: Text(
                          issue,
                          style: TextStyle(
                            color: Theme.of(context).colorScheme.error,
                          ),
                        ),
                      ),
                    ),
                  ],
                ),
              ),
              actions: [
                TextButton(
                  onPressed: () => Navigator.pop(context),
                  child: const Text('Close'),
                ),
              ],
            ),
          ),
        ),
        const Divider(height: 1),
      ],
    );
  }
}
