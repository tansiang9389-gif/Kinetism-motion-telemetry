import asyncio
import math
import struct

import pytest

from pc.app.main import TelemetryHub
from pc.app.motion import MotionDetector, STANDARD_GRAVITY
from pc.app.protocol import MAGIC, PACKET, PacketError, Telemetry, parse_packet


def packet(
    *,
    sequence: int = 1,
    quaternion: tuple[float, float, float, float] = (1.0, 0.0, 0.0, 0.0),
    linear_accel: tuple[float, float, float] = (0.0, 0.0, 0.0),
) -> bytes:
    return PACKET.pack(
        MAGIC,
        1,
        7,
        PACKET.size,
        sequence,
        1000,
        *quaternion,
        *linear_accel,
        0.0,
        0.0,
        0.0,
        80,
        3,
        0,
        100.0,
    )


def test_packet_contract_is_64_bytes_and_parses() -> None:
    parsed = parse_packet(packet(sequence=42, linear_accel=(1.0, 2.0, 3.0)))
    assert PACKET.size == 64
    assert parsed.sequence == 42
    assert parsed.linear_accel == pytest.approx((1.0, 2.0, 3.0))
    assert parsed.battery_percent == 80


@pytest.mark.parametrize(
    "data",
    [
        b"short",
        struct.pack("<I", 0xDEADBEEF) + packet()[4:],
        packet(quaternion=(0.0, 0.0, 0.0, 0.0)),
        packet(linear_accel=(math.nan, 0.0, 0.0)),
    ],
)
def test_invalid_packets_are_rejected(data: bytes) -> None:
    with pytest.raises(PacketError):
        parse_packet(data)


def test_motion_hysteresis_and_peak_capture() -> None:
    detector = MotionDetector(
        calibration_samples=3,
        start_margin_g=0.08,
        stop_margin_g=0.04,
        quiet_hold_ms=100,
    )
    for timestamp in (0, 10, 20):
        state = detector.update((0.0, 0.0, 0.0), timestamp)
    assert not state.moving

    state = detector.update((2 * STANDARD_GRAVITY, 0.0, 0.0), 30)
    assert state.moving
    assert state.current_peak_g == pytest.approx(2.0)

    detector.update((0.0, 0.0, 0.0), 40)
    state = detector.update((0.0, 0.0, 0.0), 141)
    assert not state.moving
    assert state.last_peak_g == pytest.approx(2.0)


def test_sequence_gap_tracking_resets_for_new_source() -> None:
    hub = TelemetryHub()

    def telemetry(sequence: int, timestamp_ms: int) -> Telemetry:
        return Telemetry(
            sequence=sequence,
            timestamp_ms=timestamp_ms,
            quaternion=(1.0, 0.0, 0.0, 0.0),
            linear_accel=(0.0, 0.0, 0.0),
            gyro=(0.0, 0.0, 0.0),
            battery_percent=80,
            quaternion_accuracy=3,
            sample_rate_hz=100.0,
            flags=7,
        )

    asyncio.run(hub.publish(telemetry(10, 1000), ("127.0.0.1", 4210)))
    asyncio.run(hub.publish(telemetry(20, 2000), ("127.0.0.1", 4210)))
    assert hub.lost_count == 9

    asyncio.run(hub.publish(telemetry(100, 100), ("192.168.43.66", 4210)))
    assert hub.lost_count == 9
