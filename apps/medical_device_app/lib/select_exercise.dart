import 'package:cloud_firestore/cloud_firestore.dart';
import 'package:flutter/material.dart';

import 'add_exercise.dart';
import 'app_design.dart';
import 'main.dart' show VisualizerScreen;

class ExerciseSummary {
  final String id;
  final String name;
  final String joint;

  const ExerciseSummary({
    required this.id,
    required this.name,
    required this.joint,
  });

  factory ExerciseSummary.fromMap(String id, Map<String, dynamic> data) {
    final name = data['name'];
    final joint = data['targetJoint'];
    return ExerciseSummary(
      id: id,
      name: name is String && name.trim().isNotEmpty
          ? name.trim()
          : 'Exercise name unavailable',
      joint: joint is String && joint.trim().isNotEmpty
          ? joint.trim()
          : 'Joint not configured',
    );
  }
}

class SelectExerciseScreen extends StatefulWidget {
  final Stream<List<ExerciseSummary>> Function()? loadExercises;
  final ValueChanged<ExerciseSummary>? onSelect;
  final VoidCallback? onAddExercise;

  const SelectExerciseScreen({
    super.key,
    this.loadExercises,
    this.onSelect,
    this.onAddExercise,
  });

  @override
  State<SelectExerciseScreen> createState() => _SelectExerciseScreenState();
}

class _SelectExerciseScreenState extends State<SelectExerciseScreen> {
  late Stream<List<ExerciseSummary>> _exercises;

  @override
  void initState() {
    super.initState();
    _subscribe();
  }

  void _subscribe() {
    _exercises =
        widget.loadExercises?.call() ??
        FirebaseFirestore.instance.collection('Exercises').snapshots().map((
          snapshot,
        ) {
          final exercises = snapshot.docs
              .map((doc) => ExerciseSummary.fromMap(doc.id, doc.data()))
              .toList();
          exercises.sort((a, b) => a.name.compareTo(b.name));
          return exercises;
        });
  }

  void _addExercise() {
    if (widget.onAddExercise != null) {
      widget.onAddExercise!();
    } else {
      Navigator.push(
        context,
        MaterialPageRoute<void>(builder: (_) => const AddExerciseScreen()),
      );
    }
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(title: const Text('Start exercise')),
      body: SafeArea(
        child: StreamBuilder<List<ExerciseSummary>>(
          stream: _exercises,
          builder: (context, snapshot) {
            if (snapshot.hasError) {
              return StatusPanel(
                icon: Icons.cloud_off,
                title: 'Exercises unavailable',
                message:
                    'Could not load exercises. Check the connection and database access, then retry.',
                actionLabel: 'Retry loading',
                onAction: () => setState(_subscribe),
              );
            }
            if (snapshot.connectionState == ConnectionState.waiting) {
              return const StatusPanel(
                icon: Icons.sensors,
                title: 'Loading exercises',
                message: 'Fetching configured exercises.',
                loading: true,
              );
            }
            final exercises = snapshot.data ?? [];
            if (exercises.isEmpty) {
              return StatusPanel(
                icon: Icons.sensors,
                title: 'No exercises yet',
                message: 'Add an exercise before connecting your sensors.',
                actionLabel: 'Add exercise',
                onAction: _addExercise,
              );
            }
            return Center(
              child: ConstrainedBox(
                constraints: const BoxConstraints(maxWidth: 880),
                child: ListView.separated(
                  padding: const EdgeInsets.all(24),
                  itemCount: exercises.length + 1,
                  separatorBuilder: (_, index) => index == 0
                      ? const SizedBox(height: 20)
                      : const Divider(height: 1),
                  itemBuilder: (context, index) {
                    if (index == 0) {
                      return Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          Text(
                            'Choose a movement',
                            style: Theme.of(context).textTheme.headlineMedium,
                          ),
                          const SizedBox(height: 8),
                          const Text(
                            'Select an exercise, then connect the paired arm sensors.',
                            style: TextStyle(
                              color: KinetismTheme.secondaryText,
                            ),
                          ),
                        ],
                      );
                    }
                    final exercise = exercises[index - 1];
                    return ListTile(
                      contentPadding: const EdgeInsets.symmetric(vertical: 12),
                      leading: const Icon(
                        Icons.sensors,
                        color: KinetismTheme.accent,
                      ),
                      title: Text(exercise.name),
                      subtitle: Text(
                        exercise.joint,
                        style: const TextStyle(
                          color: KinetismTheme.secondaryText,
                        ),
                      ),
                      trailing: const Icon(Icons.chevron_right),
                      onTap: () {
                        if (widget.onSelect != null) {
                          widget.onSelect!(exercise);
                        } else {
                          Navigator.push(
                            context,
                            MaterialPageRoute<void>(
                              builder: (_) =>
                                  VisualizerScreen(exerciseId: exercise.id),
                            ),
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
