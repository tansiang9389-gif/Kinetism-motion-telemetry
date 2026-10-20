# Kinetism

ESP32-C3/BNO080 motion telemetry over Wi-Fi with a local browser dashboard.

## What it measures

- Fused relative orientation from the BNO080 game rotation vector
- Gravity-free linear acceleration on X/Y/Z
- Angular velocity
- Moving/stationary state
- Current and previous swing peak acceleration in `g`
- Right elbow flexion/extension angle, maximum flexion, and session range of
  motion when one IMU is mounted on the right forearm
- Estimated forearm/hand inertial force in newtons using a patient-specific
  moving mass

The swing metric is acceleration intensity, not force. This project does not
double-integrate acceleration into position because MEMS drift makes that
result unreliable.

## Hardware configuration

| Signal | ESP32-C3 pin |
| --- | --- |
| BNO080 SDA | GPIO 6 |
| BNO080 SCL | GPIO 7 |
| Switch | GPIO 3 |
| LED | GPIO 20 |
| Battery ADC | GPIO 4 |

The BNO080 address is `0x4B`.

## Configure

1. Copy `include/secrets.example.h` to `include/secrets.h`.
2. Set the 2.4 GHz Wi-Fi SSID and password.
3. Keep `UDP_HOST` set to the PC's current IPv4 address
   (`192.168.43.253`) or update it after checking `ipconfig`.
4. Keep UDP port `4210` consistent between firmware and receiver.

`include/secrets.h` is ignored and must not be committed.

If Windows prompts, allow Python on private networks. Otherwise create inbound
rules for UDP port `4210` and TCP port `8000`.

## Build and flash firmware

```powershell
pio run
pio run --target upload --upload-port COM3
pio device monitor --port COM3 --baud 115200
```

Confirm COM3 is still the ESP32-C3 before uploading. Uploading replaces the
currently installed ESP-NOW slave firmware; the original sketch remains in the
Downloads folder.

## Run the dashboard

From the project root:

```powershell
python -m pip install -r pc\requirements.txt
python -m pc.app.main
```

Two dashboard variants are stored in separate folders and use the same
telemetry receiver:

- Current right-elbow dashboard: <http://127.0.0.1:8000/elbow/>
- Initial motion prototype: <http://127.0.0.1:8000/initial-prototype/>

The root URL redirects to the elbow dashboard. Keep the sensor stationary for
roughly 1.5 seconds after telemetry starts so the movement detector can
measure its noise floor.

For the one-IMU right-elbow view:

1. Secure the IMU to the right forearm with its +X axis pointing toward the
   wrist.
2. Keep the right upper arm fixed throughout the measurement.
3. Fully extend the elbow, then press **Set full extension (0°)**.
4. Flex and extend the elbow. The dashboard shows live angle, maximum flexion,
   session range of motion, and a 10-second angle trace.
5. Enter the patient's estimated forearm-plus-hand mass in kilograms. The
   displayed inertial force uses `force = mass × gravity-free acceleration`.
6. Set the force refresh threshold. At the default `1.0 N`, the displayed peak
   increases only when a new estimate exceeds the held peak by at least 1.0 N.
   The highest value remains visible after movement stops.
7. Press **Reset force peak** before a new trial. Setting full extension also
   resets the force peak.

One forearm IMU cannot separate elbow motion from shoulder or upper-arm
movement. Use a second upper-arm IMU later if the upper arm must move freely.
The newton value is not muscle force, elbow-joint force, torque, or force
against an external object; those require additional biomechanical inputs or a
load cell.

Optional ports:

```powershell
python -m pc.app.main --udp-port 4210 --web-port 8000
```

## Test

```powershell
python -m pip install -r pc\requirements-dev.txt
python -m pytest -q
pio run
```

## Protocol

Firmware sends one packed 64-byte little-endian UDP datagram per sample.

| Field | Type |
| --- | --- |
| Magic (`KIN1`) | `uint32` |
| Version, flags, packet size | `uint8`, `uint8`, `uint16` |
| Sequence, device timestamp ms | `uint32`, `uint32` |
| Quaternion `(w,x,y,z)` | 4 x `float32` |
| Linear acceleration `(x,y,z)` m/s² | 3 x `float32` |
| Gyro `(x,y,z)` rad/s | 3 x `float32` |
| Battery, quaternion accuracy, reserved | `uint8`, `uint8`, `uint16` |
| Measured sample rate Hz | `float32` |

Sequence gaps are reported on the dashboard rather than concealed.
