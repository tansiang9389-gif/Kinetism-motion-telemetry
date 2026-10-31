import 'package:cloud_firestore/cloud_firestore.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import 'app_design.dart';
import 'exercise_validation.dart';
import 'notification_service.dart';

class ExerciseGoal {
  final String id;
  final String name;
  final int? target;

  const ExerciseGoal({required this.id, required this.name, this.target});

  factory ExerciseGoal.fromMap(String id, Map<String, dynamic> data) {
    final name = data['name'];
    final goal = data['dailyGoal'];
    final validGoal =
        goal is num &&
        goal.isFinite &&
        goal > 0 &&
        goal == goal.truncateToDouble();
    return ExerciseGoal(
      id: id,
      name: name is String && name.trim().isNotEmpty
          ? name.trim()
          : 'Exercise name unavailable',
      target: validGoal ? goal.toInt() : null,
    );
  }
}

class PrescriptionsScreen extends StatefulWidget {
  final Stream<List<ExerciseGoal>> Function()? loadGoals;
  final Future<void> Function(String id, int goal)? saveGoal;
  final Future<bool> Function()? loadReminderStatus;
  final Future<void> Function(bool enabled)? changeReminder;

  const PrescriptionsScreen({
    super.key,
    this.loadGoals,
    this.saveGoal,
    this.loadReminderStatus,
    this.changeReminder,
  });

  @override
  State<PrescriptionsScreen> createState() => _PrescriptionsScreenState();
}

class _PrescriptionsScreenState extends State<PrescriptionsScreen> {
  late Stream<List<ExerciseGoal>> _goals;
  bool? _alertsEnabled;
  bool _updatingAlerts = false;
  String? _reminderError;

  @override
  void initState() {
    super.initState();
    _subscribe();
    _loadReminderStatus();
  }

  void _subscribe() {
    _goals =
        widget.loadGoals?.call() ??
        FirebaseFirestore.instance
            .collection('Exercises')
            .snapshots()
            .map(
              (snapshot) => snapshot.docs
                  .map((doc) => ExerciseGoal.fromMap(doc.id, doc.data()))
                  .toList(),
            );
  }

  Future<void> _loadReminderStatus() async {
    if (_updatingAlerts) return;
    setState(() {
      _updatingAlerts = true;
      _reminderError = null;
    });
    try {
      final enabled =
          await (widget.loadReminderStatus?.call() ??
              NotificationService.isDailyReminderEnabled());
      if (mounted) setState(() => _alertsEnabled = enabled);
    } on PlatformException catch (error) {
      if (mounted) {
        setState(
          () => _reminderError =
              'Reminder status unavailable (${error.code}). Check notification permissions and retry.',
        );
      }
    } on UnsupportedError catch (error) {
      if (mounted) setState(() => _reminderError = error.message);
    } on MissingPluginException {
      if (mounted) {
        setState(
          () => _reminderError =
              'Notifications are not available in this app build.',
        );
      }
    } finally {
      if (mounted) setState(() => _updatingAlerts = false);
    }
  }

  Future<void> _changeReminder(bool enabled) async {
    if (_updatingAlerts) return;
    setState(() {
      _updatingAlerts = true;
      _reminderError = null;
    });
    try {
      if (widget.changeReminder != null) {
        await widget.changeReminder!(enabled);
      } else if (enabled) {
        await NotificationService.enableDailyReminder();
      } else {
        await NotificationService.cancelReminders();
      }
      if (mounted) setState(() => _alertsEnabled = enabled);
    } on PlatformException catch (error) {
      if (mounted) {
        setState(
          () => _reminderError =
              'Reminder change was not confirmed (${error.code}). Check permissions and refresh its status.',
        );
      }
    } on UnsupportedError catch (error) {
      if (mounted) setState(() => _reminderError = error.message);
    } on MissingPluginException {
      if (mounted) {
        setState(
          () => _reminderError =
              'Notifications are not available in this app build.',
        );
      }
    } finally {
      if (mounted) setState(() => _updatingAlerts = false);
    }
  }

  Future<void> _updateGoal(ExerciseGoal exercise) async {
    final controller = TextEditingController(
      text: exercise.target?.toString() ?? '',
    );
    final formKey = GlobalKey<FormState>();
    var saving = false;
    String? saveError;
    ModalRoute<void>? dialogRoute;
    try {
      await showDialog<void>(
        context: context,
        builder: (dialogContext) {
          dialogRoute = ModalRoute.of<void>(dialogContext);
          return StatefulBuilder(
            builder: (context, setDialogState) => AlertDialog(
              scrollable: true,
              title: Text('Update goal: ${exercise.name}'),
              content: Form(
                key: formKey,
                child: Column(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    TextFormField(
                      controller: controller,
                      enabled: !saving,
                      validator: ExerciseValidation.dailyGoal,
                      keyboardType: TextInputType.number,
                      decoration: const InputDecoration(
                        labelText: 'Daily repetition goal',
                      ),
                    ),
                    if (saveError != null) ...[
                      const SizedBox(height: 12),
                      Text(
                        saveError!,
                        style: const TextStyle(color: Colors.orange),
                      ),
                    ],
                  ],
                ),
              ),
              actions: [
                TextButton(
                  onPressed: () => Navigator.pop(dialogContext),
                  child: Text(saving ? 'Close (save may continue)' : 'Cancel'),
                ),
                FilledButton(
                  onPressed: saving
                      ? null
                      : () async {
                          if (!formKey.currentState!.validate()) return;
                          final goal = int.parse(controller.text.trim());
                          setDialogState(() {
                            saving = true;
                            saveError = null;
                          });
                          try {
                            if (widget.saveGoal != null) {
                              await widget.saveGoal!(exercise.id, goal);
                            } else {
                              await FirebaseFirestore.instance
                                  .collection('Exercises')
                                  .doc(exercise.id)
                                  .update({'dailyGoal': goal});
                            }
                            if (dialogContext.mounted &&
                                dialogRoute?.isCurrent == true) {
                              Navigator.pop(dialogContext);
                            }
                          } on FirebaseException catch (error) {
                            if (dialogContext.mounted &&
                                dialogRoute?.isCurrent == true) {
                              setDialogState(
                                () => saveError =
                                    'Goal save was not confirmed (${error.code}). Check database access and retry.',
                              );
                            }
                          } finally {
                            if (dialogContext.mounted &&
                                dialogRoute?.isCurrent == true) {
                              setDialogState(() => saving = false);
                            }
                          }
                        },
                  child: Text(saving ? 'Waiting for cloud save' : 'Save goal'),
                ),
              ],
            ),
          );
        },
      );
      // The pop future completes before the dialog's exit animation.
      await dialogRoute?.completed;
    } finally {
      controller.dispose();
    }
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(title: const Text('Exercise goals & reminders')),
      body: SafeArea(
        child: Center(
          child: ConstrainedBox(
            constraints: const BoxConstraints(maxWidth: 880),
            child: StreamBuilder<List<ExerciseGoal>>(
              stream: _goals,
              builder: (context, snapshot) {
                final goals = snapshot.data ?? [];
                return CustomScrollView(
                  slivers: [
                    SliverPadding(
                      padding: const EdgeInsets.all(24),
                      sliver: SliverToBoxAdapter(
                        child: Column(
                          crossAxisAlignment: CrossAxisAlignment.stretch,
                          children: [
                            Row(
                              children: [
                                const Expanded(
                                  child: Column(
                                    crossAxisAlignment:
                                        CrossAxisAlignment.start,
                                    children: [
                                      Text(
                                        'Daily exercise reminder',
                                        style: TextStyle(
                                          fontSize: 18,
                                          fontWeight: FontWeight.w600,
                                        ),
                                      ),
                                      SizedBox(height: 4),
                                      Text(
                                        'Android notification',
                                        style: TextStyle(
                                          color: KinetismTheme.secondaryText,
                                        ),
                                      ),
                                    ],
                                  ),
                                ),
                                const SizedBox(width: 16),
                                if (_updatingAlerts)
                                  const SizedBox(
                                    width: 24,
                                    height: 24,
                                    child: CircularProgressIndicator(
                                      strokeWidth: 2,
                                    ),
                                  )
                                else if (_alertsEnabled != null)
                                  Semantics(
                                    label: 'Daily exercise reminder',
                                    child: Switch(
                                      value: _alertsEnabled!,
                                      onChanged: _changeReminder,
                                      activeThumbColor: KinetismTheme.accent,
                                    ),
                                  ),
                              ],
                            ),
                            if (_reminderError != null)
                              NoticeBanner(
                                icon: Icons.notifications_off_outlined,
                                message: _reminderError!,
                                isError: true,
                                onRetry: _loadReminderStatus,
                              ),
                            const SizedBox(height: 28),
                            Text(
                              'Exercise goals',
                              style: Theme.of(context).textTheme.titleLarge,
                            ),
                            const SizedBox(height: 8),
                            const Text(
                              'Targets are exercise settings, not measured strength.',
                              style: TextStyle(
                                color: KinetismTheme.secondaryText,
                              ),
                            ),
                          ],
                        ),
                      ),
                    ),
                    if (snapshot.hasError)
                      SliverFillRemaining(
                        hasScrollBody: false,
                        child: StatusPanel(
                          icon: Icons.cloud_off,
                          title: 'Exercise goals unavailable',
                          message:
                              'Check your connection and database access, then retry.',
                          actionLabel: 'Retry loading',
                          onAction: () => setState(_subscribe),
                        ),
                      )
                    else if (snapshot.connectionState ==
                        ConnectionState.waiting)
                      const SliverFillRemaining(
                        hasScrollBody: false,
                        child: StatusPanel(
                          icon: Icons.track_changes,
                          title: 'Loading exercise goals',
                          message: 'Fetching configured repetition targets.',
                          loading: true,
                        ),
                      )
                    else if (goals.isEmpty)
                      const SliverFillRemaining(
                        hasScrollBody: false,
                        child: StatusPanel(
                          icon: Icons.track_changes,
                          title: 'No exercise goals',
                          message:
                              'Add an exercise to configure its repetition target.',
                        ),
                      )
                    else
                      SliverPadding(
                        padding: const EdgeInsets.fromLTRB(24, 0, 24, 32),
                        sliver: SliverList.builder(
                          itemCount: goals.length,
                          itemBuilder: (context, index) {
                            final goal = goals[index];
                            return ListTile(
                              contentPadding: const EdgeInsets.symmetric(
                                vertical: 12,
                              ),
                              title: Text(goal.name),
                              subtitle: Text(
                                goal.target == null
                                    ? 'Daily goal unavailable'
                                    : 'Target: ${goal.target} reps',
                              ),
                              trailing: IconButton(
                                tooltip: 'Edit exercise goal',
                                icon: const Icon(Icons.edit_outlined),
                                onPressed: () => _updateGoal(goal),
                              ),
                            );
                          },
                        ),
                      ),
                  ],
                );
              },
            ),
          ),
        ),
      ),
    );
  }
}
