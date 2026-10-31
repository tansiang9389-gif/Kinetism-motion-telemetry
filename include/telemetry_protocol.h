#pragma once

#include <Arduino.h>

constexpr uint32_t TELEMETRY_MAGIC = 0x314E494B;  // "KIN1" on little-endian hosts
constexpr uint8_t TELEMETRY_VERSION = 1;

enum TelemetryFlags : uint8_t {
  TELEMETRY_SENSOR_OK = 1 << 0,
  TELEMETRY_WIFI_OK = 1 << 1,
  TELEMETRY_SWITCH_ON = 1 << 2,
};

struct __attribute__((packed)) TelemetryPacket {
  uint32_t magic;
  uint8_t version;
  uint8_t flags;
  uint16_t packetSize;
  uint32_t sequence;
  uint32_t timestampMs;
  float qw;
  float qx;
  float qy;
  float qz;
  float linearAccelX;
  float linearAccelY;
  float linearAccelZ;
  float gyroX;
  float gyroY;
  float gyroZ;
  uint8_t batteryPercent;
  uint8_t quaternionAccuracy;
  uint16_t reserved;
  float sampleRateHz;
};

static_assert(sizeof(TelemetryPacket) == 64, "Telemetry protocol size changed");

