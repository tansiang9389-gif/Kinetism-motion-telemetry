# Physio_A forearm slave firmware

Buildable PlatformIO port of the ESP-NOW slave sketch
[tests/C3_MS4_Slave__1___1_1.ino](../../tests/C3_MS4_Slave__1___1_1.ino). It
reads the forearm BNO080 and sends its orientation to the `Physio_A` master
over ESP-NOW (channel 1). The master forwards both sensors to the phone app and
Kinetism Desktop over Bluetooth.

Protocol, pins (SDA GPIO 6, SCL GPIO 7, IMU address `0x4B`) and timing are
unchanged from the sketch. The port adds:

* the missing `SparkFun_BNO080_Arduino_Library.h` include and library dependency;
* an ESP-NOW receive callback that compiles on Arduino-ESP32 2.x (this
  repository's PlatformIO toolchain) as well as 3.x;
* a length check before copying a received SYNC message;
* the rotation-vector report enabled when the IMU only answers on the second
  boot attempt (the sketch marked it ready but it never streamed);
* a retry every 2 s, logged as `BNO1: still not found`, while the IMU is
  missing after boot or after a failed watchdog reset (the sketch left a
  failed reset silent for good).

## Build and flash

Flashing replaces whatever the board runs now, for example the Wi-Fi `KIN1`
firmware. Back it up first if you may want it back:

```powershell
pio pkg exec --package platformio/tool-esptoolpy -- esptool.py --chip esp32c3 --port COMx read_flash 0 0x400000 backup.bin
```

```powershell
pio run -d firmware/physio-slave
pio run -d firmware/physio-slave -t upload --upload-port COMx
pio device monitor --port COMx --baud 115200
```

## Pairing check

1. Power the master. Its BNO080 must work: the master's serial log must show
   `bno1_ok=1`.
2. Power the slave. Its LED blinks while it broadcasts, then stays on once the
   master answers. The slave log shows `Synced with Master!`, then a `[SLAVE]
   reads=… readFails=…` line every second. Both are running totals. The slave
   polls the IMU every 16 ms but the IMU reports every 50 ms, so most polls
   find nothing new: a healthy sensor adds about 20 `reads` and about 40
   `readFails` a second. A fault shows as `reads` no longer rising while
   `readFails` keeps climbing, or as `>>> WATCHDOG` lines.
3. The master log shows `Synced Slave … as ID 1` and `[RX] chunks received`.
4. In Kinetism Desktop choose **Connect sensor → Bluetooth**. The status bar
   shows `Upper arm ✓  Forearm ✓`.

Status: compiled with PlatformIO `espressif32@6.12.0`; not yet run on hardware.
