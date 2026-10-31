#include <Arduino.h>
#include <WiFi.h>
#include <WiFiUdp.h>
#include <Wire.h>
#include <esp_sleep.h>

#include "SparkFun_BNO080_Arduino_Library.h"
#include "secrets.h"
#include "telemetry_protocol.h"

namespace {

constexpr uint8_t SDA_PIN = 6;
constexpr uint8_t SCL_PIN = 7;
constexpr uint8_t BNO_ADDRESS = 0x4B;
constexpr uint8_t SWITCH_PIN = 3;
constexpr uint8_t LED_PIN = 20;
constexpr uint8_t BATTERY_PIN = 4;

constexpr float ADC_MAX = 4095.0F;
constexpr float ADC_REFERENCE_V = 2.95F;
constexpr float BATTERY_DIVIDER_RATIO = 2.0F;
constexpr float BATTERY_MIN_V = 3.0F;
constexpr float BATTERY_MAX_V = 4.2F;

constexpr uint16_t REPORT_INTERVAL_MS = 10;
constexpr uint32_t SENSOR_TIMEOUT_MS = 2000;
constexpr uint32_t WIFI_CONNECT_TIMEOUT_MS = 12000;
constexpr uint32_t WIFI_RETRY_MIN_MS = 1000;
constexpr uint32_t WIFI_RETRY_MAX_MS = 30000;
constexpr uint32_t UDP_SETTLE_MS = 500;
constexpr uint32_t DIAGNOSTIC_INTERVAL_MS = 1000;

BNO080 imu;
WiFiUDP udp;
IPAddress receiverAddress;

bool sensorReady = false;
bool udpReady = false;
uint32_t sequenceNumber = 0;
uint32_t lastSensorDataMs = 0;
uint32_t lastPacketMs = 0;
uint32_t lastBatteryReadMs = 0;
uint32_t lastDiagnosticMs = 0;
uint32_t lastWifiAttemptMs = 0;
uint32_t wifiConnectedMs = 0;
uint32_t wifiRetryMs = WIFI_RETRY_MIN_MS;
uint32_t framesThisSecond = 0;
uint32_t sendsThisSecond = 0;
uint32_t sendFailuresThisSecond = 0;
float measuredSampleRateHz = 0.0F;
uint8_t batteryPercent = 0;

float clampFloat(float value, float low, float high) {
  return max(low, min(value, high));
}

uint8_t readBatteryPercentage() {
  uint32_t adcTotal = 0;
  for (uint8_t i = 0; i < 10; ++i) {
    adcTotal += analogRead(BATTERY_PIN);
  }
  const float adcValue = adcTotal / 10.0F;
  const float voltage =
      (adcValue / ADC_MAX) * ADC_REFERENCE_V * BATTERY_DIVIDER_RATIO;
  const float percentage =
      (voltage - BATTERY_MIN_V) * 100.0F / (BATTERY_MAX_V - BATTERY_MIN_V);
  return static_cast<uint8_t>(clampFloat(percentage, 0.0F, 100.0F));
}

bool quaternionValid(float w, float x, float y, float z) {
  if (!isfinite(w) || !isfinite(x) || !isfinite(y) || !isfinite(z)) {
    return false;
  }
  const float normSquared = w * w + x * x + y * y + z * z;
  return normSquared > 0.8F && normSquared < 1.2F;
}

void enableReports() {
  imu.enableGameRotationVector(REPORT_INTERVAL_MS);
  imu.enableLinearAccelerometer(REPORT_INTERVAL_MS);
  imu.enableGyro(REPORT_INTERVAL_MS);
}

bool initializeSensor() {
  Serial.println("[IMU] Initializing BNO080...");
  if (!imu.begin(BNO_ADDRESS, Wire)) {
    Serial.println("[IMU] BNO080 not found at 0x4B");
    sensorReady = false;
    return false;
  }

  Wire.setClock(400000);
  enableReports();
  lastSensorDataMs = millis();
  sensorReady = true;
  Serial.println("[IMU] Ready; quaternion, linear acceleration, and gyro enabled");
  return true;
}

void recoverSensor() {
  Serial.println("[IMU] Report timeout; restarting I2C and BNO080");
  sensorReady = false;
  Wire.end();
  delay(20);
  Wire.begin(SDA_PIN, SCL_PIN);
  Wire.setTimeOut(50);
  initializeSensor();
}

void startWifiConnection() {
  Serial.printf("[WIFI] Connecting to %s\n", WIFI_SSID);
  WiFi.disconnect();
  WiFi.begin(WIFI_SSID, WIFI_PASSWORD);
  lastWifiAttemptMs = millis();
}

void serviceWifi() {
  static wl_status_t previousStatus = WL_NO_SHIELD;
  const wl_status_t status = WiFi.status();

  if (status != previousStatus) {
    previousStatus = status;
    if (status == WL_CONNECTED) {
      wifiRetryMs = WIFI_RETRY_MIN_MS;
      udpReady = udp.begin(0) == 1;
      wifiConnectedMs = millis();
      Serial.printf("[WIFI] Connected; ESP32 IP=%s, receiver=%s:%u\n",
                    WiFi.localIP().toString().c_str(),
                    receiverAddress.toString().c_str(), UDP_PORT);
      if (!udpReady) {
        Serial.println("[UDP] Failed to open local UDP socket");
      }
    } else {
      if (udpReady) {
        udp.stop();
        udpReady = false;
      }
      Serial.printf("[WIFI] Status=%d\n", static_cast<int>(status));
    }
  }

  if (status == WL_CONNECTED) {
    return;
  }

  const uint32_t elapsed = millis() - lastWifiAttemptMs;
  if (elapsed >= WIFI_CONNECT_TIMEOUT_MS + wifiRetryMs) {
    startWifiConnection();
    wifiRetryMs = min(wifiRetryMs * 2, WIFI_RETRY_MAX_MS);
  }
}

void enterDeepSleep() {
  Serial.println("[POWER] Switch off; entering deep sleep");
  digitalWrite(LED_PIN, LOW);
  esp_deep_sleep_enable_gpio_wakeup(1ULL << SWITCH_PIN,
                                    ESP_GPIO_WAKEUP_GPIO_HIGH);
  delay(100);
  esp_deep_sleep_start();
}

void sendTelemetry() {
  if (!imu.dataAvailable()) {
    if (sensorReady && millis() - lastSensorDataMs > SENSOR_TIMEOUT_MS) {
      recoverSensor();
    }
    return;
  }

  lastSensorDataMs = millis();

  if (millis() - lastPacketMs < REPORT_INTERVAL_MS) {
    return;
  }
  lastPacketMs += REPORT_INTERVAL_MS;
  if (millis() - lastPacketMs > REPORT_INTERVAL_MS) {
    lastPacketMs = millis();
  }

  const float qw = imu.getQuatReal();
  const float qx = imu.getQuatI();
  const float qy = imu.getQuatJ();
  const float qz = imu.getQuatK();
  if (!quaternionValid(qw, qx, qy, qz)) {
    return;
  }

  if (millis() - lastBatteryReadMs >= 1000 || lastBatteryReadMs == 0) {
    batteryPercent = readBatteryPercentage();
    lastBatteryReadMs = millis();
  }

  if (WiFi.status() != WL_CONNECTED || !udpReady ||
      millis() - wifiConnectedMs < UDP_SETTLE_MS) {
    return;
  }

  TelemetryPacket packet{};
  packet.magic = TELEMETRY_MAGIC;
  packet.version = TELEMETRY_VERSION;
  packet.flags = TELEMETRY_SENSOR_OK | TELEMETRY_SWITCH_ON;
  if (WiFi.status() == WL_CONNECTED) {
    packet.flags |= TELEMETRY_WIFI_OK;
  }
  packet.packetSize = sizeof(packet);
  packet.sequence = sequenceNumber++;
  packet.timestampMs = millis();
  packet.qw = qw;
  packet.qx = qx;
  packet.qy = qy;
  packet.qz = qz;
  packet.linearAccelX = imu.getLinAccelX();
  packet.linearAccelY = imu.getLinAccelY();
  packet.linearAccelZ = imu.getLinAccelZ();
  packet.gyroX = imu.getGyroX();
  packet.gyroY = imu.getGyroY();
  packet.gyroZ = imu.getGyroZ();
  packet.batteryPercent = batteryPercent;
  packet.quaternionAccuracy = imu.getQuatAccuracy();
  packet.sampleRateHz = measuredSampleRateHz;
  ++framesThisSecond;

  if (!udp.beginPacket(receiverAddress, UDP_PORT)) {
    ++sendFailuresThisSecond;
    return;
  }
  udp.write(reinterpret_cast<const uint8_t *>(&packet), sizeof(packet));
  if (udp.endPacket() == 1) {
    ++sendsThisSecond;
  } else {
    ++sendFailuresThisSecond;
  }
  yield();
}

void printDiagnostics() {
  const uint32_t now = millis();
  if (now - lastDiagnosticMs < DIAGNOSTIC_INTERVAL_MS) {
    return;
  }

  const uint32_t elapsed = now - lastDiagnosticMs;
  measuredSampleRateHz = framesThisSecond * 1000.0F / elapsed;
  Serial.printf(
      "[STATUS] imu=%d wifi=%d samples=%.1fHz sent=%lu failures=%lu battery=%u%%\n",
      sensorReady, WiFi.status() == WL_CONNECTED, measuredSampleRateHz,
      static_cast<unsigned long>(sendsThisSecond),
      static_cast<unsigned long>(sendFailuresThisSecond), batteryPercent);
  framesThisSecond = 0;
  sendsThisSecond = 0;
  sendFailuresThisSecond = 0;
  lastDiagnosticMs = now;
}

}  // namespace

void setup() {
  Serial.begin(115200);
  delay(300);
  Serial.println("\n[KINETISM] ESP32-C3 motion telemetry v1");

  pinMode(SWITCH_PIN, INPUT);
  pinMode(LED_PIN, OUTPUT);
  pinMode(BATTERY_PIN, INPUT);
  digitalWrite(LED_PIN, HIGH);

  if (digitalRead(SWITCH_PIN) == LOW) {
    enterDeepSleep();
  }

  analogReadResolution(12);
  analogSetPinAttenuation(BATTERY_PIN, ADC_11db);
  Wire.begin(SDA_PIN, SCL_PIN);
  Wire.setClock(100000);
  Wire.setTimeOut(50);
  initializeSensor();

  if (!receiverAddress.fromString(UDP_HOST)) {
    Serial.printf("[CONFIG] Invalid UDP_HOST: %s\n", UDP_HOST);
  }

  WiFi.mode(WIFI_STA);
  WiFi.setSleep(false);
  startWifiConnection();
}

void loop() {
  if (digitalRead(SWITCH_PIN) == LOW) {
    enterDeepSleep();
  }

  serviceWifi();
  if (sensorReady) {
    sendTelemetry();
  } else if (millis() - lastSensorDataMs > SENSOR_TIMEOUT_MS) {
    recoverSensor();
  }
  printDiagnostics();
}
