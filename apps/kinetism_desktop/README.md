# Kinetism Desktop (Rust)

Windows desktop companion for the `Physio_A` paired-arm sensors, written in
Rust with [egui](https://github.com/emilk/egui). It adds:

1. **Shoulder abduction** measurement with an automatic MMT suggestion (0-3)
   and physiotherapist confirmation of grades 4-5.
2. **Fruit PNF game**, a Fruit-Ninja-style shoulder game: Level 0 *Draw the
   Apple*, Level 1 *PNF D1*, Level 2 *PNF D2*.
3. **Progress on muscle**: body map, daily MMT level, MMT and game trends,
   and draft next-goal guidance per MMT grade.

Elbow flexion is also measurable, using the phone app's force and MMT model, so
the biceps appear in the progress view. Patients and records are shared with the
Flutter phone app through the same Firestore project.

Two kinds of sensor hardware are supported: the `Physio_A` Bluetooth pair
(upper arm + forearm), or a single Wi-Fi sensor running the root PlatformIO
firmware (`KIN1` UDP packets).

> Prototype software. It is not a clinically validated medical device.
> Automatic MMT grades and load estimates are suggestions for physiotherapist
> review, not diagnoses.

## Run it

Download the app or build it from source:

* **Download (no Rust needed):** every version tag (`v*`) publishes
  `kinetism-desktop-<tag>-windows-x64.zip` on the
  [Releases](https://github.com/tansiang9389-gif/Kinetism-motion-telemetry/releases)
  page. Unzip it and run `kinetism_desktop.exe`. The exe is unsigned, so
  Windows SmartScreen may warn: choose **More info**, then **Run anyway**.
* **From source:** install Rust with [rustup](https://rustup.rs) (stable, MSVC
  toolchain) and Visual Studio Build Tools with the C++ workload, then run
  this from this folder:

```powershell
cargo run --release
```

Then:

1. Switch the sensors on. If the phone app is connected to `Physio_A`,
   disconnect it first: a connected sensor stops advertising.
2. Press **Connect sensor** (top right) and choose:
   * **Bluetooth · Physio_A pair**: the status chip shows about 20 Hz, and the
     status bar shows whether the forearm sensor is streaming.
   * **Wi-Fi · KIN1 sensor**: the chip shows the sensor's IP address and about
     100 Hz, and the status bar shows battery and IMU accuracy. On first use,
     Windows asks whether to allow Kinetism Desktop through the firewall: click
     **Allow**.

No sensor at hand? Choose **Simulator**. A virtual arm generates real
quaternion packets that go through the same parser and calibration as sensor
data. In the game, the mouse pointer drives the virtual hand. Simulator
attempts and game rounds are practice only: they are shown but never saved,
synced or used to unlock levels (except in `KINETISM_DEMO` mode, which uses a
throwaway store). Stopping the simulator, closing its window or connecting a
real sensor discards its calibration, peaks and any round it played, so
nothing it produced can be saved afterwards.

## Sensors

### Bluetooth pair (`Physio_A`)

| Sensor | Placement | Payload position |
| --- | --- | --- |
| Master (`Physio_A`, BLE) | Lateral upper arm, mid-way between shoulder and elbow | Values 1-4 (`x,y,z,w`) |
| Slave (ESP-NOW to master) | Back of the forearm, near the wrist | Values 5-8 (`x,y,z,w`) |

The firmware is [tests/master_1_1_1_1.ino](../../tests/master_1_1_1_1.ino)
plus [tests/C3_MS4_Slave__1___1_1.ino](../../tests/C3_MS4_Slave__1___1_1.ino);
[firmware/physio-slave](../../firmware/physio-slave/README.md) is a buildable
port of the slave. No firmware change is needed. If the slave has not synced,
the master sends zeros for the forearm. Shoulder abduction still works with the
upper-arm sensor alone; the elbow check and the hand position then fall back to
a straight-arm estimate. If the master's own IMU is missing, the status bar
says so and calibration cannot start: everything is measured relative to the
upper arm.

If the chip shows **"Physio_A connected · no readings"**, the master accepted
the connection but has nothing to send. Its serial log (USB, 115200 baud)
then shows `bno1_ok=0`, meaning the BNO080 does not answer on I2C. Check the
master's IMU wiring (3V3, GND, SDA = GPIO 6, SCL = GPIO 7, address `0x4B`) and
power-cycle the sensor.

### Wi-Fi sensor (`KIN1` over UDP)

The root PlatformIO firmware ([src/main.cpp](../../src/main.cpp)) sends one
64-byte `KIN1` packet per sample (100 Hz) to `UDP_HOST:4210`. Set `UDP_HOST`
in `include/secrets.h` to this PC's IPv4 address; the app shows it in the
tooltip while it waits for packets.

* One IMU only: strap it on the **lateral upper arm**. Shoulder abduction and
  the game work. Elbow flexion needs the Bluetooth pair.
* The app follows the first board that sends, and forwards about 25 frames/s
  so calibration and smoothing behave like the 20 Hz Bluetooth stream. Once
  that board has been silent for 2 s, another board can take over (even one
  that kept sending meanwhile); the app then drops the calibration and asks
  you to recalibrate.
* Only one program can listen on the port: close the Python PC dashboard
  (`python -m pc.app.main`) first.
* Windows Firewall must allow inbound UDP for the app. On a *Public* network,
  dismissing the first-use prompt creates a **Block** rule. Fix it in Windows
  Security → Firewall & network protection → Allow an app through firewall.

## Calibration (every session, ~10 s)

The BNO080 *game rotation vector* gives tilt relative to gravity, but each
sensor has its own arbitrary heading and an unknown mounting. Two short poses
resolve both:

1. **Rest**: patient seated upright, arm relaxed and hanging straight down,
   elbow straight. Hold still for about one second.
2. **Forward**: arm raised straight forward (at least 25°, about 60° is
   ideal), elbow straight. A helper may support a weak arm.

Step 2 can be skipped (**elevation only**). Elevation stays exact, but the
heading is then estimated from the sensor mounting, so the plane check and
game control are less precise. Recalibrate whenever the straps move.
Switching to another sensor source, another Wi-Fi board or another patient
always starts a new calibration.

## Shoulder abduction

* **Angle** = upper-arm elevation from hanging straight down (0° at rest,
  90° horizontal, 180° overhead). Gravity is the reference, so the patient
  must sit upright. Leaning the trunk sideways inflates the angle (no trunk
  sensor).
* **Plane check**: the app warns when the arm drifts forward into flexion
  (more than 40° from the side plane) or behind the body.
* **Elbow check**: with the forearm sensor, the app warns when the elbow bends
  more than 30° (the shorter lever makes the test easier).
* **Load (est.)**: whole-arm weight component perpendicular to the arm
  (`m·g·sin θ`) and the resulting shoulder torque. The arm mass comes from the
  phone app's anthropometric model. This is not muscle force.

Automatic MMT suggestion (sitting, against gravity), graded on the attempt
peak. The plane and elbow notes in the suggestion, and the plane and elbow
angles saved with the record, are taken at that peak, not where the arm is
when **Record attempt** is pressed:

| Peak elevation | Grade | Note |
| --- | --- | --- |
| < 5° | 0 Zero | Physio palpates to tell 0 from 1 |
| 5-15° | 1 Trace | Flicker of movement |
| 15-80° | 2 Poor | Partial range against gravity |
| ≥ 80° | 3 Fair | Reaches the 90° test range (10° tolerance) |
| - | 4-5 | Needs manual resistance: physio confirms with one tap |

Every attempt recorded with a sensor is saved immediately. The
physiotherapist can confirm or override any grade; the record keeps both the
physio grade and the original automatic suggestion.

Measuring the other (healthy) arm as a baseline does not change the
patient's affected side or saved arm dimensions. Set the affected side on the
patient hub; the arm dimensions entered in Measure are saved with the patient
only for the affected arm (or when none are saved yet). Choosing the affected
arm in Measure reloads its saved sizes into the form; choosing the other arm
keeps the form as it is, to be edited for that arm. Saved sizes outside the
form's ranges (for example from the phone app) are shown as they are, and
**Continue** asks for a correction instead of replacing them.

## Fruit PNF game

The blade follows the patient's **hand**, computed from both sensors and
projected onto the frontal plane. The screen is a mirror (patient's right =
screen right) and a faint body outline shows the patient's position.

| Level | Movement | Muscles trained |
| --- | --- | --- |
| 0 - Draw the Apple | Trace an apple outline in the air. Completing it slices the apple. 3 apples per round. | Middle & anterior deltoid, supraspinatus |
| 1 - PNF D1 | Swipe from the same-side hip up across the face to the opposite ear, slicing the fruit, then return to the basket. | Anterior deltoid, pectoralis major (clavicular) |
| 2 - PNF D2 | Swipe from the opposite hip up and out above the shoulder ("draw the sword"), then return to the basket. | Middle deltoid, supraspinatus, infraspinatus |

* **Unlocking**: scoring **80%** or more on a level unlocks the next one. A
  *Physio override* checkbox unlocks all levels.
* **Scoring**: Level 0 combines outline coverage, time on the line and a
  gentle time bonus. PNF reps combine fruit sliced (a clean slice follows the
  diagonal's direction) and closeness to the diagonal. Stars: 60% / 80% / 90%.
* **Bombs** sit off the diagonal to discourage compensation (-20 points, combo
  reset, no game over). They can be switched off.
* **Pause**, or a gap of more than a quarter of a second in the sensor
  frames, holds the round. The blade restarts from wherever the hand is when
  play resumes, so nothing is sliced across the gap. After 1.5 s without
  frames the canvas says the game is paused.
* **Adaptive size**: the apple size and the diagonal length scale with the
  patient's best recorded shoulder abduction.
* **Streak**: consecutive days with at least one game.
* **Voice**: spoken encouragement through Windows text-to-speech (top-right
  toggle).

## Progress on muscle

* **Patient view**: today's MMT level per muscle with change since the last
  measurement day, game streak, body map, next goal and last game.
* **Physio view**: clickable front/back body map coloured by latest MMT
  grade; a *Daily MMT level* grid (best grade per day, last 14 days, with dots
  for game sessions); MMT trend for the selected muscle; game score trend with
  the 80% unlock line; muscle summary table.
* **Recovery guideline**: draft activities and next goal for each MMT grade.
  It is marked *DRAFT* and must be reviewed by a physiotherapist.

Muscle mapping: elbow flexion → biceps brachii; shoulder abduction → middle
deltoid + supraspinatus. Anterior deltoid, pectoralis major and infraspinatus
are trained in the game and have no MMT test in this prototype.

## Data and Firestore

The app uses the phone app's Firestore project (`armtracker-84a82`) over the
REST API:

| Collection | Use | Fields added by this app |
| --- | --- | --- |
| `Patients` | Shared with the phone app (merge writes) | `affectedSide` |
| `MeasurementRecords` | Shared; phone-app fields kept unchanged | `movement`, `side`, `peakAngleDegrees`, `torqueNm`, `mmtSource`, `autoMmtGrade`, `elbowFlexionDegrees`, `planeDegrees`, `muscles`, `source` |
| `GameSessions` | New: one document per finished round | level, score, stars, reps, accuracy, max elevation, … |

Shoulder records use `bodyPart: "Shoulder"`, so they appear in the phone
app's record log. Phone records without `movement` are read as elbow flexion
(`bodyPart: "Arm"`).

Every change is saved locally first
(`%APPDATA%\Kinetism\desktop-cache.json`) and queued for Firestore. Pending
writes survive restarts; failed ones are retried every 30 s. Firestore is
re-read at start-up, every 5 minutes and when you click the sync chip, never
while a read is still running. Both timers also run while the window sits
idle. Each read lists every patient, record and game session, and the free
plan allows 50,000 document reads a day, shared with the phone app. The
top-right chip shows the sync state. A reply that is not JSON
(for example a Wi-Fi sign-in page) counts as a failure, never as a saved
write or an empty collection, and a write only counts once Firestore's reply
names the saved document. If the cache file cannot be read at start-up,
it is never overwritten: the app moves it aside to
`desktop-cache.unreadable-<date-time>.json` (records that never reached
Firestore can be recovered from it) and warns in the status bar.

Each sync target keeps its own cache file: another project
(`KINETISM_FIREBASE_PROJECT`) uses `desktop-cache-<project>.json` and
`KINETISM_OFFLINE` uses `desktop-cache-offline.json`, so test or offline data
is never sent to the shared project. Pulled numbers that are not finite are
dropped, and a record's arm dimensions outside the form's ranges are replaced
by the phone app's default arm, so one bad document cannot make the cache
unreadable. A patient's pulled arm sizes are kept as they are (see
[Shoulder abduction](#shoulder-abduction)).

> Security: the project currently accepts unauthenticated reads and writes
> (the phone app has no sign-in). Add Firebase Authentication and restrictive
> security rules before storing real patient data.

## Environment variables

| Variable | Effect |
| --- | --- |
| `KINETISM_OFFLINE=1` | Local storage only, in `desktop-cache-offline.json`. This data is never synced, not even by a later normal run. |
| `KINETISM_FIREBASE_PROJECT` | Use another Firestore project (with its own cache file) |
| `KINETISM_DATA_DIR` | Folder for the local cache |
| `KINETISM_SOURCE=wifi\|bluetooth\|sim` | Connect this sensor source at startup |
| `KINETISM_UDP_PORT` | UDP port for the Wi-Fi sensor (default 4210) |
| `KINETISM_SIM=1` | Same as `KINETISM_SOURCE=sim` |
| `KINETISM_DEMO=<scenario>` | Synthetic offline demo: `patients`, `hub`, `calibrate`, `measure`, `play`, `game0`, `game1`, `game2`, `result`, `progress`, `progress-patient`, `records`. Never syncs to Firestore. |
| `KINETISM_SCREENSHOT=<file.png>` | With `KINETISM_DEMO`: save a screenshot, then exit |
| `KINETISM_SCREENSHOT_DELAY=<s>` | Screenshot delay (default 6 s) |
| `WGPU_BACKEND=dx12\|vulkan\|gl` | Override the graphics backend (default on Windows: DX12) |
| `WGPU_POWER_PREF=low\|high` | GPU preference on PCs with two GPUs (default: `low`, usually the integrated GPU; `high` prefers a discrete GPU) |

## Test

```powershell
cargo test
cargo fmt --check
cargo clippy --all-targets -- -D warnings
```

GitHub Actions ([`kinetism-desktop.yml`](../../.github/workflows/kinetism-desktop.yml))
runs the same checks on Windows for every pull request that touches this app.
Pushing a `v*` tag also builds the release zip and attaches it to a GitHub
Release.

The unit tests cover Bluetooth and `KIN1` packet parsing (including a packet
captured from a real Wi-Fi sensor), sensor-source switching, calibration with
arbitrary sensor mounting and heading (both arms), MMT grading, the phone app's
force model, the Firestore codec (including phone-app documents), apple
tracing, PNF slicing/bombs/combos, unlocks and streaks, and the data-integrity
rules (patient switching, simulator practice, peak posture, sync write order,
pull and retry timers, per-target caches and unusable Firestore replies).

## Troubleshooting

| Symptom | Fix |
| --- | --- |
| `Physio_A not found` | Switch the master on and disconnect the phone app. Check that Bluetooth is on in Windows. |
| *Physio_A connected · no readings* | The master's IMU is not reporting (serial log `bno1_ok=0`) and no slave is synced. Check the IMU wiring and power-cycle the master. |
| Status bar says *Forearm sensor missing* | The slave has not synced to the master (master serial log `[RX] chunks received`). Shoulder abduction still works. |
| Invalid packets increase | Firmware payload changed; it must be 8 comma-separated values (Bluetooth) or 64-byte `KIN1` packets (Wi-Fi). |
| *Wi-Fi: waiting for packets* | Sensor off or on another network, `UDP_HOST` is not this PC's IP, or Windows Firewall blocks the app (see [Wi-Fi sensor](#wi-fi-sensor-kin1-over-udp)). |
| *Sensor error: Cannot listen on UDP port 4210* | Another program uses the port; close the Python PC dashboard. |
| Angle drifts after several minutes | Game-rotation-vector heading drift: press **Recalibrate**. |
| *No live sensor data* on Measure, or *Waiting for sensor data… (game paused)* | No upper-arm orientation for 1.5 s (the forearm sensor alone cannot move the shoulder angle). Recording and the game pause until it returns; check the upper-arm sensor. |
| Status bar: *Local cache unreadable … moved to …* | The cache could not be read (for example it was re-saved in another encoding). It was kept as `desktop-cache.unreadable-<date-time>.json`; the app continues from Firestore data. |
| Status bar: *… Local saving is paused …* | Another program (backup or antivirus) holds the cache open. Close it and restart the app; new records still sync to Firestore. |
| Sync chip shows an error | Check the internet connection and Firestore rules. Records stay queued locally. |
| Sync chip: *Firestore reply was not JSON …* or *… did not confirm the write …* | A proxy, firewall or Wi-Fi sign-in page answered instead of Firestore. Sign in to the network or allow `firestore.googleapis.com`; queued writes are retried. |
| Toast: *New Wi-Fi sensor … please recalibrate* | The followed board was silent for 2 s and another one took over. Recalibrate, and switch off any board that is not strapped to the patient. |
