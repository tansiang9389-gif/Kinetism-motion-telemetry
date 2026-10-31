import 'dart:convert';

import 'package:vector_math/vector_math_64.dart' as vector;

class ArmSensorFrame {
  final vector.Quaternion upper;
  final vector.Quaternion forearm;

  ArmSensorFrame._(this.upper, this.forearm);

  factory ArmSensorFrame.parse(List<int> bytes) {
    final parts = utf8.decode(bytes).trim().split(',');
    if (parts.length != 8) {
      throw const FormatException('Expected exactly eight quaternion values.');
    }
    final values = parts.map((part) {
      final value = double.tryParse(part.trim());
      if (value == null || !value.isFinite) {
        throw const FormatException(
          'Quaternion values must be finite numbers.',
        );
      }
      return value;
    }).toList();
    vector.Quaternion quaternion(int offset) {
      final result = vector.Quaternion(
        values[offset],
        values[offset + 1],
        values[offset + 2],
        values[offset + 3],
      );
      if (!result.length2.isFinite || result.length2 < 1e-12) {
        throw const FormatException('Quaternion has no valid orientation.');
      }
      result.normalize();
      return result;
    }

    return ArmSensorFrame._(quaternion(0), quaternion(4));
  }
}
