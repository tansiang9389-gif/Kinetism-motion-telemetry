# TODOS

## Hardware

### Fix master A's upper-arm IMU

**What:** Get the BNO080 IMU on Physio_A master A detected again (its serial
log shows `bno1_ok=0`).

**Why:** Without it the Bluetooth pair sends no upper-arm orientation, so
calibration, shoulder abduction and the game cannot use the original
two-sensor setup.

**Context:** An I2C scan on master A (MAC ending B1:70) found no device on any
of 10 SDA/SCL pin pairs, although GPIO6/7 have pull-ups. That points to the
IMU module, its power or its solder joints. The spare master (MAC ending
02:50) runs the same firmware but has no IMU attached. Only the Wi-Fi KIN1
board (MAC ending F8:C8) has a working IMU today.

**Effort:** M
**Priority:** P1
**Depends on:** None

### Restore master A's original firmware

**What:** Flash master A's full-flash backup back onto the board. It still runs
the I2C scanner used for the diagnosis.

**Why:** Master A cannot work as the Physio_A Bluetooth master until its
firmware is restored.

**Context:** The 4 MB backup, its SHA-256 and the exact esptool commands are in
`RESTORE.md` in the firmware backup folder (kept outside this repo). The
ESP32-C3 USB-JTAG link sometimes reports "Corrupt data" on large reads, so
read and verify in chunks.

**Effort:** S
**Priority:** P1
**Depends on:** None

### Test the physio-slave firmware on a board

**What:** Flash `firmware/physio-slave` to a board with a working IMU and
confirm SYNC with the master, ESP-NOW forearm chunks, and a forearm reading in
Kinetism Desktop.

**Why:** The port compiles but has never run on hardware, and elbow flexion
needs the forearm sensor.

**Context:** See `firmware/physio-slave/README.md`. Flashing it onto the Wi-Fi
board (MAC ending F8:C8) replaces its KIN1 Wi-Fi firmware, so that board stops
working as a Wi-Fi sensor.

**Effort:** M
**Priority:** P2
**Depends on:** Fix master A's upper-arm IMU; Restore master A's original
firmware

## Completed
