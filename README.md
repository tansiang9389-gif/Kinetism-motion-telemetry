# Kinetism

ESP32-C3/BNO080 motion telemetry, a local PC dashboard, and the refined
Flutter rehabilitation prototype. This is development software, not a
clinically validated medical device.

## Download the Android prototype

[Download Kinetism for Android](https://github.com/tansiang9389-gif/Kinetism-motion-telemetry/releases/download/prototype-refinement-20261031/kinetism-prototype-1.0.0-debug.apk)
or open the [development release](https://github.com/tansiang9389-gif/Kinetism-motion-telemetry/releases/tag/prototype-refinement-20261031).

Android 7.0 or newer is required. The APK is debug-signed and intended for a
dedicated test phone, not clinical use. AI coaching and new AI report generation
are unconfigured in the public build; no private provider key is bundled.
Firebase client configuration is retained, but backend rules, persistence,
physical sensors and notifications have not been validated.

Patient selection, illustrated arm measurements, calibrated force, MMT position
capture and authenticated phone/PC patient-record sync are not implemented.
See the [app documentation](apps/medical_device_app/README.md) for the exact
feature boundaries, installation details and verified APK checksum.

## Project layout

| Folder | Purpose |
| --- | --- |
| [src](src) and [include](include) | Wi-Fi/UDP ESP32-C3 firmware and a non-secret configuration template |
| [pc](pc) | Local Python telemetry receiver and both browser dashboards |
| [tests](tests) | PC protocol regressions and saved paired-sensor Arduino reference sketches |
| [apps/medical_device_app](apps/medical_device_app) | Refined Flutter source, native projects, tests and editable branding |
| [docs/refinement](docs/refinement/design-audit-kinetism.md) | Refinement review, synthetic browser evidence and screenshots |

The original upload ZIP is retained as a sanitised **source-only** archive.
Configured Wi-Fi credentials and compiled/cache files have been removed from
that public copy. Original local files and Git history were not rewritten.
Rotate any real Wi-Fi credentials previously present in the archive and revoke
the former embedded AI API key; removing a current file does not erase history.

### Sensor integration boundaries

The [saved BLE master sketch](tests/master_1_1_1_1.ino) advertises `Physio_A`
and sends the eight quaternion CSV fields expected by the Flutter app.
The [saved ESP-NOW slave sketch](tests/C3_MS4_Slave__1___1_1.ino) is its reference
companion. These sketches were retained as supplied, not built or flashed during
this refinement; the slave reference lacks its BNO080 library header and needs
Arduino/library setup before it can be treated as a buildable firmware project.

The root PlatformIO project instead builds the **Wi-Fi/UDP** firmware used by
the PC dashboard. Its binary UDP packets are not the Flutter BLE payload.
Do not flash that firmware expecting the phone's BLE tracker to read it.
The synthetic browser preview is not a synchronised clinical PC portal.

## What the Wi-Fi prototype measures

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

## Configure the Wi-Fi firmware

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
currently installed ESP-NOW firmware. The saved reference sketches remain in
[tests](tests), but they are not selected by this PlatformIO build.

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
