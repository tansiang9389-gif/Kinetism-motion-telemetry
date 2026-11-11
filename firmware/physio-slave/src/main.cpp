// Physio_A forearm slave (ESP32-C3 + BNO080).
//
// Buildable port of tests/C3_MS4_Slave__1___1_1.ino. Protocol, pins and timing
// are unchanged. Changes: the missing BNO080 include, an ESP-NOW receive
// callback that compiles on Arduino-ESP32 2.x and 3.x, and a length check
// before copying a received SYNC message.
#include <Arduino.h>
#include <WiFi.h>
#include <esp_now.h>
#include <esp_wifi.h>
#include <Wire.h>
#include "SparkFun_BNO080_Arduino_Library.h"

// ================= CONFIGURATION =================
#define SDA_1 6
#define SCL_1 7
#define BNO_ADDR 0x4B
#define SWITCH_PIN 3
#define LED_PIN 20
#define BATT_PIN 4

// ================= BATTERY CONFIGURATION =================
#define ADC_MAX 4095.0
#define VREF 2.95
#define DIVIDER_RATIO 2.0
#define BATTERY_MAX 4.2
#define BATTERY_MIN 3.0

// ================= MATH STRUCTS =================
struct Quat { float x, y, z, w; };

// ================= OBJECTS =================
BNO080 bno1;
bool bno1_ok = false;

// Watchdog Vars
Quat prevQ1 = {0,0,0,0};
int staleCount1 = 0;

// ================= MATH HELPERS =================
// A perfectly still BNO080 still jitters at the noise floor, so exact
// float equality wrongly flags a healthy stationary sensor as "stale".
// Only treat readings as identical if they match within a tiny epsilon,
// which is what a genuinely frozen/hung sensor produces.
#define STALE_EPSILON 0.00001f
bool isSame(Quat a, Quat b) {
  return (fabs(a.x - b.x) < STALE_EPSILON &&
          fabs(a.y - b.y) < STALE_EPSILON &&
          fabs(a.z - b.z) < STALE_EPSILON &&
          fabs(a.w - b.w) < STALE_EPSILON);
}

// --- Message Structures ---
enum MsgType { MSG_ANNOUNCE, MSG_SYNC, MSG_DATA_CHUNK };

typedef struct __attribute__((packed)) {
  uint8_t id;
  uint32_t timestamp;
  uint8_t battery;
  float x;
  float y;
  float z;
  float w;
} SensorSample;

#define SAMPLES_PER_CHUNK 10
typedef struct __attribute__((packed)) {
  uint8_t type;
  uint8_t count;
  SensorSample samples[SAMPLES_PER_CHUNK];
} DataChunkMsg;

typedef struct __attribute__((packed)) {
  uint8_t type;
  uint8_t mac[6];
} AnnounceMsg;

typedef struct __attribute__((packed)) {
  uint8_t type;
  uint32_t masterTime;
  uint8_t assignedId;
} SyncMsg;

bool readMySensor(SensorSample &outSample, uint8_t myId);

uint8_t broadcastMac[] = {0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF};
uint8_t masterMac[6];
bool isSynced = false;
uint8_t mySlaveId = 0;
uint32_t timeOffset = 0;

// --- FreeRTOS Queue ---
QueueHandle_t espnowQueue;

// --- Timing Variables ---
unsigned long lastSensorRead = 0;
const unsigned long SENSOR_INTERVAL = 16;
unsigned long lastBlinkTime = 0;
bool currentLedState = HIGH;
unsigned long lastSlaveBroadcast = 0;

// ================= BATTERY READING =================
float readBatteryVoltage() {
  int adcValue = 0;
  for (int i = 0; i < 10; i++) {
    adcValue += analogRead(BATT_PIN);
  }
  adcValue /= 10;
  float voltage = (adcValue / ADC_MAX) * VREF;
  return voltage * DIVIDER_RATIO;
}

uint8_t getBatteryPercentage() {
  float v = readBatteryVoltage();
  float p = (v - BATTERY_MIN) / (BATTERY_MAX - BATTERY_MIN) * 100.0;
  if (p > 100.0) p = 100.0;
  if (p < 0.0) p = 0.0;
  return (uint8_t)p;
}

// ================= SENSOR WATCHDOG =================
void resetSensor(int id) {
  Serial.print(">>> WATCHDOG: Full Reset Sensor "); Serial.println(id);
  Wire.end();
  Wire.begin(SDA_1, SCL_1);
  Wire.setClock(400000);
  delay(100);
  if (bno1.begin(BNO_ADDR, Wire)) {
    Serial.println(">>> Sensor Recovered!");
    bno1.enableGameRotationVector(50);
    bno1_ok = true;
    staleCount1 = 0;
  } else {
    Serial.println(">>> Sensor Recovery Failed.");
  }
}

// ================= SENSOR READ HELPER =================
bool readMySensor(SensorSample &outSample, uint8_t myId) {
  if (!bno1_ok) return false;
  if (bno1.dataAvailable()) {
    float qi = bno1.getQuatI();
    float qj = bno1.getQuatJ();
    float qk = bno1.getQuatK();
    float qr = bno1.getQuatReal();
    if (qi != 0.0 || qj != 0.0 || qk != 0.0 || qr != 0.0) {
      Quat raw = {qi, qj, qk, qr};
      if (isSame(raw, prevQ1)) staleCount1++;
      else { staleCount1 = 0; prevQ1 = raw; }
      if (staleCount1 >= 200) { resetSensor(1); return false; }
      outSample.id = myId;
      outSample.timestamp = millis() + timeOffset;
      outSample.battery = getBatteryPercentage();
      outSample.x = raw.x;
      outSample.y = raw.y;
      outSample.z = raw.z;
      outSample.w = raw.w;
      return true;
    }
  }
  return false;
}

// ================= ESP-NOW RECEIVE =================
static void handleRecv(const uint8_t *mac_addr, const uint8_t *incomingData, int len) {
  if (len < 1) return;
  uint8_t msgType = incomingData[0];

  if (msgType == MSG_SYNC && len >= (int)sizeof(SyncMsg)) {
    SyncMsg msg;
    memcpy(&msg, incomingData, sizeof(msg));
    memcpy(masterMac, mac_addr, 6);
    mySlaveId = msg.assignedId;
    isSynced = true;
    timeOffset = msg.masterTime - millis();

    if (!esp_now_is_peer_exist(masterMac)) {
      esp_now_peer_info_t peerInfo;
      memset(&peerInfo, 0, sizeof(peerInfo));
      memcpy(peerInfo.peer_addr, masterMac, 6);
      peerInfo.channel = 1;
      esp_now_add_peer(&peerInfo);
    }

    digitalWrite(LED_PIN, HIGH); // Solid LED when synced
    Serial.printf("Synced with Master! My ID is %d. Time Offset: %d ms\n", mySlaveId, (int)timeOffset);
  }
}

#if ESP_ARDUINO_VERSION_MAJOR >= 3
void OnDataRecv(const esp_now_recv_info_t *esp_now_info, const uint8_t *incomingData, int len) {
  handleRecv(esp_now_info->src_addr, incomingData, len);
}
#else
void OnDataRecv(const uint8_t *mac_addr, const uint8_t *incomingData, int len) {
  handleRecv(mac_addr, incomingData, len);
}
#endif

// ================= SETUP =================
void setup() {
  Serial.begin(115200);
  analogReadResolution(12);
  analogSetPinAttenuation(BATT_PIN, ADC_11db);
  Wire.begin(SDA_1, SCL_1);
  Wire.setClock(100000);
  Wire.setTimeOut(50);
  pinMode(BATT_PIN, INPUT);
  pinMode(SWITCH_PIN, INPUT);
  pinMode(LED_PIN, OUTPUT);

  int state = digitalRead(SWITCH_PIN);
  if (state == HIGH) {
    digitalWrite(LED_PIN, HIGH);
    Serial.println("Switch ON -> Awake, LED ON");
  } else {
    Serial.println("Switch OFF -> Sleeping...");
    esp_deep_sleep_enable_gpio_wakeup((1ULL << SWITCH_PIN), ESP_GPIO_WAKEUP_GPIO_HIGH);
    delay(100);
    esp_deep_sleep_start();
  }

  Serial.println("Attempting to initialize Slave Sensor...");
  if (bno1.begin(BNO_ADDR, Wire)) {
    Serial.println("BNO1: SUCCESS (Slave sensor found)");
    bno1.enableGameRotationVector(50);
    bno1_ok = true;
  } else {
    Serial.println("BNO1: FAILED! Check pins 6(SDA) and 7(SCL) and Power.");
    delay(500);
    if (bno1.begin(BNO_ADDR, Wire)) {
      Serial.println("BNO1: Success on second attempt");
      bno1_ok = true;
    }
  }

  Wire.setClock(100000);
  WiFi.mode(WIFI_STA);
  if (esp_now_init() != ESP_OK) {
    Serial.println("ESP-NOW init failed!");
    return;
  }
  // Pin ESP-NOW to the same fixed channel the master uses (channel 1).
  esp_wifi_set_channel(1, WIFI_SECOND_CHAN_NONE);
  esp_now_register_recv_cb(OnDataRecv);

  esp_now_peer_info_t peerInfo;
  memset(&peerInfo, 0, sizeof(peerInfo));
  memcpy(peerInfo.peer_addr, broadcastMac, 6);
  peerInfo.channel = 1;
  esp_now_add_peer(&peerInfo);

  espnowQueue = xQueueCreate(100, sizeof(SensorSample));

  Serial.println("Slave setup complete. Broadcasting for master...");
}

// ================= LOOP =================
void loop() {
  if (digitalRead(SWITCH_PIN) == LOW) {
    Serial.println("Switch turned OFF -> Sleeping...");
    digitalWrite(LED_PIN, LOW);
    esp_deep_sleep_enable_gpio_wakeup((1ULL << SWITCH_PIN), ESP_GPIO_WAKEUP_GPIO_HIGH);
    delay(100);
    esp_deep_sleep_start();
  }

  if (!isSynced) {
    // Blink while searching
    if (millis() - lastBlinkTime > 250) {
      lastBlinkTime = millis();
      currentLedState = !currentLedState;
      digitalWrite(LED_PIN, currentLedState);
    }

    if (millis() - lastSlaveBroadcast > 2000) {
      lastSlaveBroadcast = millis();
      AnnounceMsg msg;
      msg.type = MSG_ANNOUNCE;
      WiFi.macAddress(msg.mac);
      esp_now_send(broadcastMac, (uint8_t*)&msg, sizeof(msg));
      Serial.println("Broadcasting presence...");
    }
  } else {
    static unsigned long readCount = 0;
    static unsigned long failCount = 0;
    static unsigned long chunkCount = 0;
    static unsigned long lastDbg = 0;

    // Read sensor and queue for ESP-NOW
    if (millis() - lastSensorRead >= SENSOR_INTERVAL) {
      lastSensorRead = millis();
      SensorSample myData;
      if (readMySensor(myData, mySlaveId)) {
        xQueueSend(espnowQueue, &myData, 0);
        readCount++;
      } else {
        failCount++;
      }
    }

    // Send chunk to master
    if (uxQueueMessagesWaiting(espnowQueue) >= SAMPLES_PER_CHUNK) {
      DataChunkMsg chunk;
      chunk.type = MSG_DATA_CHUNK;
      chunk.count = SAMPLES_PER_CHUNK;
      for (int i = 0; i < SAMPLES_PER_CHUNK; i++) {
        xQueueReceive(espnowQueue, &chunk.samples[i], 0);
      }
      esp_err_t res = esp_now_send(masterMac, (uint8_t*)&chunk, sizeof(chunk));
      chunkCount++;
      Serial.printf("[SLAVE] chunk sent #%lu  esp_now_send result=%d (0=OK)\n",
                    chunkCount, res);
    }

    // Once-per-second summary
    if (millis() - lastDbg > 1000) {
      lastDbg = millis();
      Serial.printf("[SLAVE] reads=%lu  readFails=%lu  queueWaiting=%u  masterMac=%02X:%02X:%02X:%02X:%02X:%02X\n",
                    readCount, failCount, uxQueueMessagesWaiting(espnowQueue),
                    masterMac[0], masterMac[1], masterMac[2],
                    masterMac[3], masterMac[4], masterMac[5]);
    }
  }
}
