from __future__ import annotations

from dataclasses import dataclass
import math

STANDARD_GRAVITY = 9.80665


@dataclass(frozen=True)
class MotionState:
    moving: bool
    magnitude_g: float
    current_peak_g: float
    last_peak_g: float
    movement_duration_ms: int
    noise_floor_g: float


class MotionDetector:
    def __init__(
        self,
        calibration_samples: int = 150,
        start_margin_g: float = 0.08,
        stop_margin_g: float = 0.04,
        quiet_hold_ms: int = 180,
    ) -> None:
        self.calibration_samples = calibration_samples
        self.start_margin_g = start_margin_g
        self.stop_margin_g = stop_margin_g
        self.quiet_hold_ms = quiet_hold_ms
        self._calibration: list[float] = []
        self.noise_floor_g = 0.0
        self.moving = False
        self.started_ms = 0
        self.quiet_since_ms: int | None = None
        self.current_peak_g = 0.0
        self.last_peak_g = 0.0

    def update(
        self, linear_accel: tuple[float, float, float], timestamp_ms: int
    ) -> MotionState:
        magnitude_g = math.sqrt(sum(value * value for value in linear_accel))
        magnitude_g /= STANDARD_GRAVITY

        if len(self._calibration) < self.calibration_samples:
            self._calibration.append(magnitude_g)
            sorted_values = sorted(self._calibration)
            percentile_index = int((len(sorted_values) - 1) * 0.95)
            self.noise_floor_g = sorted_values[percentile_index]

        start_threshold = self.noise_floor_g + self.start_margin_g
        stop_threshold = self.noise_floor_g + self.stop_margin_g

        if not self.moving and magnitude_g >= start_threshold:
            self.moving = True
            self.started_ms = timestamp_ms
            self.current_peak_g = magnitude_g
            self.quiet_since_ms = None
        elif self.moving:
            self.current_peak_g = max(self.current_peak_g, magnitude_g)
            if magnitude_g <= stop_threshold:
                if self.quiet_since_ms is None:
                    self.quiet_since_ms = timestamp_ms
                elif timestamp_ms - self.quiet_since_ms >= self.quiet_hold_ms:
                    self.moving = False
                    self.last_peak_g = self.current_peak_g
                    self.current_peak_g = 0.0
                    self.quiet_since_ms = None
            else:
                self.quiet_since_ms = None

        duration = timestamp_ms - self.started_ms if self.moving else 0
        return MotionState(
            moving=self.moving,
            magnitude_g=magnitude_g,
            current_peak_g=self.current_peak_g,
            last_peak_g=self.last_peak_g,
            movement_duration_ms=duration,
            noise_floor_g=self.noise_floor_g,
        )

