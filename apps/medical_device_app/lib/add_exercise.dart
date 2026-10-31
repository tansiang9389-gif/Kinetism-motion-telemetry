import 'package:cloud_firestore/cloud_firestore.dart';
import 'package:flutter/material.dart';

import 'app_design.dart';
import 'exercise_validation.dart';

class AddExerciseScreen extends StatefulWidget {
  final Future<void> Function(Map<String, Object>)? saveExercise;
  final String savedMessage;

  const AddExerciseScreen({
    super.key,
    this.saveExercise,
    this.savedMessage = 'Exercise saved.',
  });

  @override
  State<AddExerciseScreen> createState() => _AddExerciseScreenState();
}

class _AddExerciseScreenState extends State<AddExerciseScreen> {
  final _formKey = GlobalKey<FormState>();
  final _nameCtrl = TextEditingController();
  final _jointCtrl = TextEditingController(text: 'Elbow');
  final _restCtrl = TextEditingController();
  final _peakCtrl = TextEditingController();
  final _videoCtrl = TextEditingController();
  final _goalCtrl = TextEditingController(text: '10');
  bool _isSaving = false;
  String? _saveError;

  @override
  void dispose() {
    for (final controller in [
      _nameCtrl,
      _jointCtrl,
      _restCtrl,
      _peakCtrl,
      _videoCtrl,
      _goalCtrl,
    ]) {
      controller.dispose();
    }
    super.dispose();
  }

  Future<void> _saveExercise() async {
    if (_isSaving || !_formKey.currentState!.validate()) return;
    final route = ModalRoute.of(context);
    final recipe = <String, Object>{
      'name': _nameCtrl.text.trim(),
      'targetJoint': _jointCtrl.text.trim(),
      'restingAngle': double.parse(_restCtrl.text.trim()),
      'peakAngle': double.parse(_peakCtrl.text.trim()),
      'videoUrl': _videoCtrl.text.trim(),
      'dailyGoal': int.parse(_goalCtrl.text.trim()),
    };
    setState(() {
      _isSaving = true;
      _saveError = null;
    });
    try {
      if (widget.saveExercise != null) {
        await widget.saveExercise!(recipe);
      } else {
        await FirebaseFirestore.instance.collection('Exercises').add(recipe);
      }
      if (!mounted || route?.isCurrent != true) return;
      ScaffoldMessenger.of(
        context,
      ).showSnackBar(SnackBar(content: Text(widget.savedMessage)));
      Navigator.pop(context);
    } on FirebaseException catch (error) {
      if (!mounted) return;
      setState(
        () => _saveError =
            'Exercise was not confirmed saved (${error.code}). Check the connection and database access, then retry.',
      );
    } finally {
      if (mounted) setState(() => _isSaving = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(title: const Text('Add exercise')),
      body: SafeArea(
        child: Align(
          alignment: Alignment.topCenter,
          child: ConstrainedBox(
            constraints: const BoxConstraints(maxWidth: 720),
            child: SingleChildScrollView(
              padding: const EdgeInsets.all(24),
              child: Form(
                key: _formKey,
                autovalidateMode: AutovalidateMode.onUserInteraction,
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  children: [
                    Text(
                      'Exercise details',
                      style: Theme.of(context).textTheme.titleLarge,
                    ),
                    const SizedBox(height: 8),
                    const Text(
                      'Configure movement thresholds, not a strength or MMT grade.',
                      style: TextStyle(color: KinetismTheme.secondaryText),
                    ),
                    const SizedBox(height: 24),
                    _field(
                      _nameCtrl,
                      'Exercise name',
                      validator: ExerciseValidation.requiredText,
                      hint: 'e.g. Elbow flexion',
                    ),
                    const SizedBox(height: 16),
                    _field(
                      _jointCtrl,
                      'Target joint',
                      validator: ExerciseValidation.requiredText,
                    ),
                    const SizedBox(height: 24),
                    LayoutBuilder(
                      builder: (context, constraints) {
                        final rest = _field(
                          _restCtrl,
                          'Resting angle (degrees)',
                          validator: ExerciseValidation.angle,
                          numeric: true,
                          hint: 'e.g. 160',
                        );
                        final peak = _field(
                          _peakCtrl,
                          'Peak angle (degrees)',
                          validator: (value) => ExerciseValidation.peakAngle(
                            value,
                            _restCtrl.text,
                          ),
                          numeric: true,
                          hint: 'e.g. 60',
                        );
                        if (constraints.maxWidth < 520) {
                          return Column(
                            children: [rest, const SizedBox(height: 16), peak],
                          );
                        }
                        return Row(
                          crossAxisAlignment: CrossAxisAlignment.start,
                          children: [
                            Expanded(child: rest),
                            const SizedBox(width: 16),
                            Expanded(child: peak),
                          ],
                        );
                      },
                    ),
                    const SizedBox(height: 16),
                    _field(
                      _goalCtrl,
                      'Daily repetition goal',
                      validator: ExerciseValidation.dailyGoal,
                      numeric: true,
                      wholeNumber: true,
                    ),
                    const SizedBox(height: 16),
                    _field(
                      _videoCtrl,
                      'Demonstration video URL',
                      validator: ExerciseValidation.videoUrl,
                      hint: 'https://...',
                      url: true,
                    ),
                    const SizedBox(height: 20),
                    if (_saveError != null)
                      NoticeBanner(
                        icon: Icons.error_outline,
                        message: _saveError!,
                        isError: true,
                      ),
                    FilledButton.icon(
                      onPressed: _isSaving ? null : _saveExercise,
                      icon: _isSaving
                          ? const SizedBox(
                              width: 20,
                              height: 20,
                              child: CircularProgressIndicator(strokeWidth: 2),
                            )
                          : const Icon(Icons.save_outlined),
                      label: Text(
                        _isSaving ? 'Waiting for cloud save' : 'Save exercise',
                      ),
                    ),
                  ],
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }

  Widget _field(
    TextEditingController controller,
    String label, {
    required FormFieldValidator<String> validator,
    String? hint,
    bool numeric = false,
    bool wholeNumber = false,
    bool url = false,
  }) {
    return TextFormField(
      key: ValueKey(label),
      controller: controller,
      enabled: !_isSaving,
      validator: validator,
      keyboardType: numeric
          ? TextInputType.numberWithOptions(decimal: !wholeNumber)
          : url
          ? TextInputType.url
          : TextInputType.text,
      textInputAction: TextInputAction.next,
      decoration: InputDecoration(labelText: label, hintText: hint),
    );
  }
}
