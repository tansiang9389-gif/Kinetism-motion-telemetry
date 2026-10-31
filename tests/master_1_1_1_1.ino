#include <WiFi.h>
#include <esp_now.h>
#include <esp_wifi.h>
#include <BLEDevice.h>
#include <BLEServer.h>
#include <BLEUtils.h>
#include <BLE2902.h>
#include "esp_bt.h"
#include <Wire.h>
#include "SparkFun_BNO080_Arduino_Library.h"
#include <esp_sleep.h>
#include "esp_system.h"

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
// Same fix as the slave: a still-but-healthy BNO080 jitters at the noise
// floor, so exact float equality wrongly flags it as stale. Use an epsilon.
#define STALE_EPSILON 0.00001f
bool isSame(Quat a, Quat b) {
  return (fabs(a.x - b.x) < STALE_EPSILON &&
          fabs(a.y - b.y) < STALE_EPSILON &&
          fabs(a.z - b.z) < STALE_EPSILON &&
          fabs(a.w - b.w) < STALE_EPSILON);
}

// --- UUIDs for BLE ---
#define SERVICE_UUID        "4fafc201-1fb5-459e-8fcc-c5c9c331914b"
#define CHARACTERISTIC_UUID "beb5483e-36e1-4688-b7f5-ea07361b26a8"

// BLE Variables
BLEServer* pServer = NULL;
BLECharacteristic* pCharacteristic = NULL;
bool deviceConnected = false;

// --- FreeRTOS Queue for Slave Data ---
QueueHandle_t slaveQueue;

// --- 60Hz Sampling Variables ---
unsigned long lastSensorRead = 0;
const unsigned long SENSOR_INTERVAL = 16;
unsigned long lastMasterBleSend = 0;
const unsigned long BLE_INTERVAL = 50;

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
uint8_t slaveMac[6];
bool slaveSynced = false;
uint8_t myId = 0;

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
  static unsigned long lastDataMs = 0;
  if (lastDataMs == 0) lastDataMs = millis();

  // Diagnostic: once per second, report WHERE this is failing.
  static unsigned long lastSensDbg = 0;
  if (millis() - lastSensDbg > 1000) {
    lastSensDbg = millis();
    bool avail = bno1_ok ? bno1.dataAvailable() : false;
    Serial.printf("[MSENS] bno1_ok=%d  dataAvailable=%d\n", bno1_ok, avail);
  }

  if (!bno1_ok) return false;

  // If the report stream has gone quiet for >2s, the feature-enable was
  // likely dropped (common on BNO080 when begin() is followed by radio
  // init). Re-issue the enable without a full bus teardown. This is the
  // fix for "bno1_ok=1 but dataAvailable=0 forever".
  if (millis() - lastDataMs > 2000) {
    Serial.println("[MSENS] no data >2s -> re-enabling rotation vector");
    bno1.enableGameRotationVector(50);
    lastDataMs = millis();   // give it time to start before retrying
  }

  if (bno1.dataAvailable()) {
    lastDataMs = millis();
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
      outSample.timestamp = millis();
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

String macToStr(const uint8_t* mac) {
  char buf[18];
  snprintf(buf, sizeof(buf), "%02X:%02X:%02X:%02X:%02X:%02X", mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]);
  return String(buf);
}

// ================= ESP-NOW RECEIVE =================
void OnDataRecv(const esp_now_recv_info_t *esp_now_info, const uint8_t *incomingData, int len) {
  const uint8_t *mac_addr = esp_now_info->src_addr;
  uint8_t msgType = incomingData[0];

  if (msgType == MSG_ANNOUNCE) {
    AnnounceMsg msg;
    memcpy(&msg, incomingData, sizeof(msg));
    memcpy(slaveMac, msg.mac, 6);
    slaveSynced = true;
    if (!esp_now_is_peer_exist(msg.mac)) {
      esp_now_peer_info_t peerInfo;
      memset(&peerInfo, 0, sizeof(peerInfo));
      memcpy(peerInfo.peer_addr, msg.mac, 6);
      peerInfo.channel = 1;
      esp_now_add_peer(&peerInfo);
    }
    SyncMsg syncMsg;
    syncMsg.type = MSG_SYNC;
    syncMsg.masterTime = millis();
    syncMsg.assignedId = 1;
    esp_now_send(msg.mac, (uint8_t*)&syncMsg, sizeof(syncMsg));
    Serial.printf("Synced Slave %s as ID 1\n", macToStr(msg.mac).c_str());
  }
  else if (msgType == MSG_DATA_CHUNK) {
    DataChunkMsg chunk;
    memcpy(&chunk, incomingData, sizeof(chunk));
    for (int i = 0; i < chunk.count; i++) {
      xQueueSend(slaveQueue, &chunk.samples[i], 0);
    }
    // Diagnostic: count chunks actually received over ESP-NOW.
    static unsigned long rxChunks = 0;
    static unsigned long lastRxLog = 0;
    rxChunks++;
    if (millis() - lastRxLog > 1000) {
      lastRxLog = millis();
      Serial.printf("[RX] chunks received from slave: %lu\n", rxChunks);
    }
  }
}

// ================= BLE CALLBACKS =================
class MyServerCallbacks: public BLEServerCallbacks {
  void onConnect(BLEServer* pServer) {
    deviceConnected = true;
    // The BLE connection event can move the radio off our ESP-NOW channel;
    // re-assert it so slave chunks keep arriving.
    esp_wifi_set_channel(1, WIFI_SECOND_CHAN_NONE);
  };
  void onDisconnect(BLEServer* pServer) {
    deviceConnected = false;
    pServer->startAdvertising();
  }
};

// ================= BLE SETUP =================
void setupBLE() {
  BLEDevice::init("Physio_A");
  // Approximate 10% TX power (-9 dBm is ~12.6%, the closest available level to 10%)
  BLEDevice::setPower(ESP_PWR_LVL_N9);
  pServer = BLEDevice::createServer();
  pServer->setCallbacks(new MyServerCallbacks());

  BLEService *pService = pServer->createService(SERVICE_UUID);
  pCharacteristic = pService->createCharacteristic(
                        CHARACTERISTIC_UUID,
                        BLECharacteristic::PROPERTY_READ |
                        BLECharacteristic::PROPERTY_NOTIFY
                      );
  pCharacteristic->addDescriptor(new BLE2902());
  pService->start();

  BLEAdvertising *pAdvertising = BLEDevice::getAdvertising();
  pAdvertising->addServiceUUID(SERVICE_UUID);
  // Ask the phone for a slower connection interval (units of 1.25ms:
  // 0x18=30ms min, 0x28=50ms max). A tight interval makes BLE hog the
  // single C3 radio and starves ESP-NOW reception; a slower one leaves
  // time slices for the slave's data chunks to arrive.
  pAdvertising->setMinPreferred(0x18);
  pAdvertising->setMaxPreferred(0x28);

  BLEAdvertisementData advData;
  advData.setFlags(0x06);
  advData.setName("Physio_A");
  pAdvertising->setAdvertisementData(advData);

  pAdvertising->start();
  Serial.println("BLE Started as 'Physio_A'. Waiting for mobile...");
}

// ================= SETUP =================
void setup() {
  Serial.begin(115200);
  delay(200);
  // Why did we just (re)start? This single line distinguishes the causes
  // of a "sudden disconnect": deep sleep (switch path fired), brownout
  // (power sag), a crash/watchdog, or a clean power-on/reset.
  esp_reset_reason_t rr = esp_reset_reason();
  Serial.print(">>> BOOT reset_reason = ");
  switch (rr) {
    case ESP_RST_POWERON:   Serial.println("POWERON (clean power-up / reset btn)"); break;
    case ESP_RST_DEEPSLEEP: Serial.println("DEEPSLEEP (woke from sleep - switch path fired)"); break;
    case ESP_RST_BROWNOUT:  Serial.println("BROWNOUT (power sag - fix power!)"); break;
    case ESP_RST_PANIC:     Serial.println("PANIC (firmware crash)"); break;
    case ESP_RST_INT_WDT:   Serial.println("INT_WDT (interrupt watchdog crash)"); break;
    case ESP_RST_TASK_WDT:  Serial.println("TASK_WDT (task watchdog crash)"); break;
    case ESP_RST_SW:        Serial.println("SW (software reset)"); break;
    default:                Serial.printf("OTHER (code %d)\n", (int)rr); break;
  }
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

  Serial.println("Attempting to initialize Master Sensor...");
  if (bno1.begin(BNO_ADDR, Wire)) {
    Serial.println("BNO1: SUCCESS (Master sensor found)");
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

  // Help BLE and ESP-NOW play nice
  esp_bt_controller_mem_release(ESP_BT_MODE_CLASSIC_BT);

  Wire.setClock(100000);
  WiFi.mode(WIFI_STA);
  if (esp_now_init() != ESP_OK) {
    Serial.println("ESP-NOW init failed!");
    return;
  }
  // Pin ESP-NOW to a fixed channel so master and slave always match.
  // Without this, BLE/WiFi coexistence can leave the master on a
  // different channel than the slave, so chunks send OK but never arrive.
  esp_wifi_set_channel(1, WIFI_SECOND_CHAN_NONE);
  esp_now_register_recv_cb(OnDataRecv);

  esp_now_peer_info_t peerInfo;
  memset(&peerInfo, 0, sizeof(peerInfo));
  memcpy(peerInfo.peer_addr, broadcastMac, 6);
  peerInfo.channel = 1;
  esp_now_add_peer(&peerInfo);

  slaveQueue = xQueueCreate(100, sizeof(SensorSample));

  setupBLE();

  Serial.println("Master setup complete. Waiting for slave...");
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

  static SensorSample masterLatest;
  static SensorSample slaveLatest;
  static bool freshMaster = false;
  static bool freshSlave = false;

  // 1. Read master sensor
  SensorSample tempRead;
  if (readMySensor(tempRead, myId)) {
    masterLatest = tempRead;
    freshMaster = true;
  }

  // 2. Pull latest slave data from queue
  SensorSample tempSlave;
  if (xQueueReceive(slaveQueue, &tempSlave, 0) == pdTRUE) {
    slaveLatest = tempSlave;
    freshSlave = true;
  }

  // --- DEBUG: print status once per second ---
  static unsigned long lastDebugPrint = 0;
  if (millis() - lastDebugPrint > 1000) {
    lastDebugPrint = millis();
    // Safety net: re-pin the ESP-NOW channel in case an active BLE
    // connection nudged the radio onto a different channel.
    if (deviceConnected) {
      esp_wifi_set_channel(1, WIFI_SECOND_CHAN_NONE);
    }
    Serial.printf("[DEBUG] bleConnected=%d  freshMaster=%d  freshSlave=%d  slaveSynced=%d  queueWaiting=%u\n",
                  deviceConnected, freshMaster, freshSlave, slaveSynced,
                  uxQueueMessagesWaiting(slaveQueue));
  }

  // 3. Transmit packaged data via BLE when EITHER source is fresh
  //    (was previously gated on BOTH being fresh, which silenced BLE
  //     entirely whenever the slave wasn't delivering data)
  if (deviceConnected && (freshMaster || freshSlave) && (millis() - lastMasterBleSend > BLE_INTERVAL)) {
    lastMasterBleSend = millis();

    // Payload is exactly 8 fields: master quaternion (x,y,z,w) then
    // slave quaternion (x,y,z,w). The app parses indices 0..3 and 4..7,
    // so battery must NOT be inserted here or every field after it shifts
    // and the forearm quaternion becomes garbage (looked like "no movement").
    char payload[128];
    snprintf(payload, sizeof(payload),
             "%.3f,%.3f,%.3f,%.3f,%.3f,%.3f,%.3f,%.3f",
             masterLatest.x, masterLatest.y, masterLatest.z, masterLatest.w,
             slaveLatest.x, slaveLatest.y, slaveLatest.z, slaveLatest.w);

    pCharacteristic->setValue(payload);
    pCharacteristic->notify();

    // Diagnostic: count BLE notifies actually sent.
    static unsigned long notifyCount = 0;
    static unsigned long lastNotifyLog = 0;
    notifyCount++;
    if (millis() - lastNotifyLog > 1000) {
      lastNotifyLog = millis();
      Serial.printf("[TX] BLE notifies sent: %lu\n", notifyCount);
    }

    freshMaster = false;
    freshSlave = false;
  }
}
