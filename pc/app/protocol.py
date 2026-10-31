from __future__ import annotations

from dataclasses import asdict, dataclass
import math
import struct
from typing import Any

MAGIC = 0x314E494B
VERSION = 1
PACKET = struct.Struct("<IBBHII10fBBHf")


class PacketError(ValueError):
    pass


@dataclass(frozen=True)
class Telemetry:
    sequence: int
    timestamp_ms: int
    quaternion: tuple[float, float, float, float]
    linear_accel: tuple[float, float, float]
    gyro: tuple[float, float, float]
    battery_percent: int
    quaternion_accuracy: int
    sample_rate_hz: float
    flags: int

    def as_dict(self) -> dict[str, Any]:
        data = asdict(self)
        data["quaternion"] = list(self.quaternion)
        data["linear_accel"] = list(self.linear_accel)
        data["gyro"] = list(self.gyro)
        return data


def parse_packet(data: bytes) -> Telemetry:
    if len(data) != PACKET.size:
        raise PacketError(f"expected {PACKET.size} bytes, received {len(data)}")

    unpacked = PACKET.unpack(data)
    magic, version, flags, packet_size, sequence, timestamp_ms = unpacked[:6]
    values = unpacked[6:16]
    battery_percent, accuracy, reserved, sample_rate_hz = unpacked[16:]

    if magic != MAGIC:
        raise PacketError(f"invalid magic 0x{magic:08x}")
    if version != VERSION:
        raise PacketError(f"unsupported protocol version {version}")
    if packet_size != PACKET.size:
        raise PacketError(f"declared packet size {packet_size} is invalid")
    if reserved != 0:
        raise PacketError("reserved field must be zero")
    if not all(math.isfinite(value) for value in (*values, sample_rate_hz)):
        raise PacketError("packet contains a non-finite value")
    if not 0 <= battery_percent <= 100:
        raise PacketError(f"invalid battery percentage {battery_percent}")

    quaternion = tuple(values[:4])
    norm = math.sqrt(sum(component * component for component in quaternion))
    if not 0.8 <= norm <= 1.2:
        raise PacketError(f"invalid quaternion norm {norm:.3f}")

    return Telemetry(
        sequence=sequence,
        timestamp_ms=timestamp_ms,
        quaternion=quaternion,
        linear_accel=tuple(values[4:7]),
        gyro=tuple(values[7:10]),
        battery_percent=battery_percent,
        quaternion_accuracy=accuracy,
        sample_rate_hz=sample_rate_hz,
        flags=flags,
    )

