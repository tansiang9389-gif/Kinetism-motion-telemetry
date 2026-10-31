from __future__ import annotations

import argparse
import asyncio
from contextlib import asynccontextmanager
import logging
from pathlib import Path
import socket
import time
from typing import Any

from fastapi import FastAPI, WebSocket, WebSocketDisconnect
from fastapi.responses import RedirectResponse
from fastapi.staticfiles import StaticFiles
import uvicorn

from .motion import MotionDetector
from .protocol import PacketError, Telemetry, parse_packet

LOGGER = logging.getLogger("kinetism")
DASHBOARDS_DIR = Path(__file__).parent / "dashboards"
ELBOW_DASHBOARD_DIR = DASHBOARDS_DIR / "elbow"
INITIAL_DASHBOARD_DIR = DASHBOARDS_DIR / "initial-prototype"


class TelemetryHub:
    def __init__(self) -> None:
        self.clients: set[WebSocket] = set()
        self.detector = MotionDetector()
        self.latest: dict[str, Any] | None = None
        self.last_sequence: int | None = None
        self.last_device_timestamp_ms: int | None = None
        self.last_source: str | None = None
        self.packet_count = 0
        self.invalid_count = 0
        self.lost_count = 0
        self.last_received_monotonic = 0.0
        self.zero_quaternion: tuple[float, float, float, float] | None = None

    async def publish(self, telemetry: Telemetry, source: tuple[str, int]) -> None:
        source_host = source[0]
        stream_restarted = (
            self.last_source != source_host
            or (
                self.last_device_timestamp_ms is not None
                and telemetry.timestamp_ms < self.last_device_timestamp_ms
            )
        )
        if stream_restarted:
            self.last_sequence = None
        if self.last_sequence is not None:
            delta = (telemetry.sequence - self.last_sequence) & 0xFFFFFFFF
            if 1 < delta < 0x80000000:
                self.lost_count += delta - 1
        self.last_sequence = telemetry.sequence
        self.last_device_timestamp_ms = telemetry.timestamp_ms
        self.last_source = source_host
        self.packet_count += 1
        self.last_received_monotonic = time.monotonic()

        motion = self.detector.update(
            telemetry.linear_accel, telemetry.timestamp_ms
        )
        payload = telemetry.as_dict()
        payload.update(
            {
                "type": "telemetry",
                "source": source[0],
                "moving": motion.moving,
                "magnitude_g": motion.magnitude_g,
                "current_peak_g": motion.current_peak_g,
                "last_peak_g": motion.last_peak_g,
                "movement_duration_ms": motion.movement_duration_ms,
                "noise_floor_g": motion.noise_floor_g,
                "packets_received": self.packet_count,
                "packets_invalid": self.invalid_count,
                "packets_lost": self.lost_count,
                "relative_quaternion": self.relative_quaternion(
                    telemetry.quaternion
                ),
            }
        )
        self.latest = payload
        await self.broadcast(payload)

    def relative_quaternion(
        self, current: tuple[float, float, float, float]
    ) -> list[float]:
        if self.zero_quaternion is None:
            self.zero_quaternion = current
        w0, x0, y0, z0 = self.zero_quaternion
        w, x, y, z = current
        return [
            w0 * w + x0 * x + y0 * y + z0 * z,
            w0 * x - x0 * w - y0 * z + z0 * y,
            w0 * y + x0 * z - y0 * w - z0 * x,
            w0 * z - x0 * y + y0 * x - z0 * w,
        ]

    async def broadcast(self, payload: dict[str, Any]) -> None:
        disconnected: list[WebSocket] = []
        for client in self.clients:
            try:
                await client.send_json(payload)
            except (RuntimeError, WebSocketDisconnect):
                disconnected.append(client)
        self.clients.difference_update(disconnected)

    def zero_orientation(self) -> None:
        if self.latest is not None:
            self.zero_quaternion = tuple(self.latest["quaternion"])

    def health(self) -> dict[str, Any]:
        age_ms = (
            (time.monotonic() - self.last_received_monotonic) * 1000
            if self.last_received_monotonic
            else None
        )
        return {
            "connected": age_ms is not None and age_ms < 1000,
            "last_packet_age_ms": age_ms,
            "packets_received": self.packet_count,
            "packets_invalid": self.invalid_count,
            "packets_lost": self.lost_count,
        }


class UdpProtocol(asyncio.DatagramProtocol):
    def __init__(self, hub: TelemetryHub) -> None:
        self.hub = hub

    def datagram_received(self, data: bytes, addr: tuple[str, int]) -> None:
        try:
            telemetry = parse_packet(data)
        except PacketError as error:
            self.hub.invalid_count += 1
            LOGGER.warning("Rejected packet from %s:%d: %s", *addr, error)
            return
        asyncio.create_task(self.hub.publish(telemetry, addr))

    def error_received(self, exc: Exception) -> None:
        LOGGER.error("UDP receiver error: %s", exc)


hub = TelemetryHub()
udp_transport: asyncio.DatagramTransport | None = None


@asynccontextmanager
async def lifespan(app: FastAPI):
    global udp_transport
    loop = asyncio.get_running_loop()
    transport, _ = await loop.create_datagram_endpoint(
        lambda: UdpProtocol(hub),
        local_addr=(app.state.udp_host, app.state.udp_port),
        family=socket.AF_INET,
    )
    udp_transport = transport
    LOGGER.info("Listening for telemetry on UDP %s:%d", app.state.udp_host, app.state.udp_port)
    try:
        yield
    finally:
        transport.close()
        udp_transport = None


app = FastAPI(title="Kinetism Motion Dashboard", lifespan=lifespan)
app.state.udp_host = "0.0.0.0"
app.state.udp_port = 4210
app.mount(
    "/elbow",
    StaticFiles(directory=ELBOW_DASHBOARD_DIR, html=True),
    name="elbow-dashboard",
)
app.mount(
    "/initial-prototype",
    StaticFiles(directory=INITIAL_DASHBOARD_DIR, html=True),
    name="initial-dashboard",
)


@app.get("/")
async def index() -> RedirectResponse:
    return RedirectResponse("/elbow/")


@app.get("/api/health")
async def health() -> dict[str, Any]:
    return hub.health()


@app.websocket("/ws")
async def websocket_endpoint(websocket: WebSocket) -> None:
    await websocket.accept()
    hub.clients.add(websocket)
    if hub.latest is not None:
        await websocket.send_json(hub.latest)
    try:
        while True:
            message = await websocket.receive_json()
            if message.get("action") == "zero_orientation":
                hub.zero_orientation()
    except WebSocketDisconnect:
        hub.clients.discard(websocket)


def run() -> None:
    parser = argparse.ArgumentParser(description="Kinetism motion dashboard")
    parser.add_argument("--udp-host", default="0.0.0.0")
    parser.add_argument("--udp-port", default=4210, type=int)
    parser.add_argument("--web-host", default="127.0.0.1")
    parser.add_argument("--web-port", default=8000, type=int)
    args = parser.parse_args()
    app.state.udp_host = args.udp_host
    app.state.udp_port = args.udp_port
    logging.basicConfig(level=logging.INFO, format="%(levelname)s %(message)s")
    uvicorn.run(app, host=args.web_host, port=args.web_port)


if __name__ == "__main__":
    run()
