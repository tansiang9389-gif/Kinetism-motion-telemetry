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

> Prototype software. It is not a clinically validated medical device.
> Automatic MMT grades and load estimates are suggestions for physiotherapist
> review, not diagnoses.

## Run it

1. Install Rust with [rustup](https://rustup.rs) (stable, MSVC toolchain).
   Visual Studio Build Tools with the C++ workload must be installed.
2. Switch the sensors on. If the phone app is connected to `Physio_A`,
   disconnect it first: a connected sensor stops advertising.
3. From this folder:

```powershell
cargo run --release
```

4. Press **Connect Physio_A** (top right). The status chip shows the packet
   rate (about 20 Hz) and the status bar shows whether the forearm sensor is
   streaming.

No sensor at hand? Press **Simulator**. A virtual arm generates real
quaternion packets that go through the same parser and calibration as BLE
data. In the game, the mouse pointer drives the virtual hand.

## Sensor placement

| Sensor | Placement | Payload position |
| --- | --- | --- |
| Master (`Physio_A`, BLE) | Lateral upper arm, mid-way between shoulder and elbow | Values 1-4 (`x,y,z,w`) |
| Slave (ESP-NOW to master) | Back of the forearm, near the wrist | Values 5-8 (`x,y,z,w`) |

The firmware is [tests/master_1_1_1_1.ino](../../tests/master_1_1_1_1.ino)
plus [tests/C3_MS4_Slave__1___1_1.ino](../../tests/C3_MS4_Slave__1___1_1.ino).
No firmware change is needed. If the slave has not synced, the master sends
zeros for the forearm. Shoulder abduction still works with the upper-arm sensor
alone; the elbow check and the hand position then fall back to a straight-arm
estimate.

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
peak:

| Peak elevation | Grade | Note |
| --- | --- | --- |
| < 5° | 0 Zero | Physio palpates to tell 0 from 1 |
| 5-15° | 1 Trace | Flicker of movement |
| 15-80° | 2 Poor | Partial range against gravity |
| ≥ 80° | 3 Fair | Reaches the 90° test range (10° tolerance) |
| - | 4-5 | Needs manual resistance: physio confirms with one tap |

Every attempt is saved immediately. The physiotherapist can confirm or
override any grade; the record keeps both the physio grade and the original
automatic suggestion.

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
writes survive restarts and are retried. Firestore is re-read every 30 s so
phone records appear here. The top-right chip shows the sync state.

> Security: the project currently accepts unauthenticated reads and writes
> (the phone app has no sign-in). Add Firebase Authentication and restrictive
> security rules before storing real patient data.

## Environment variables

| Variable | Effect |
| --- | --- |
| `KINETISM_OFFLINE=1` | Local storage only, no Firestore |
| `KINETISM_FIREBASE_PROJECT` | Use another Firestore project |
| `KINETISM_DATA_DIR` | Folder for the local cache |
| `KINETISM_SIM=1` | Start with the simulator on |
| `KINETISM_DEMO=<scenario>` | Synthetic offline demo: `patients`, `hub`, `calibrate`, `measure`, `play`, `game0`, `game1`, `game2`, `result`, `progress`, `progress-patient`, `records`. Never syncs to Firestore. |
| `KINETISM_SCREENSHOT=<file.png>` | With `KINETISM_DEMO`: save a screenshot, then exit |
| `KINETISM_SCREENSHOT_DELAY=<s>` | Screenshot delay (default 6 s) |
| `WGPU_BACKEND=dx12\|vulkan\|gl` | Override the graphics backend (default on Windows: DX12) |

## Test

```powershell
cargo test
cargo clippy --all-targets
```

The unit tests cover packet parsing, calibration with arbitrary sensor
mounting and heading (both arms), MMT grading, the phone app's force model,
the Firestore codec (including phone-app documents), apple tracing, PNF
slicing/bombs/combos, unlocks and streaks.

## Troubleshooting

| Symptom | Fix |
| --- | --- |
| `Physio_A not found` | Switch the master on and disconnect the phone app. Check that Bluetooth is on in Windows. |
| Status bar says *Forearm sensor missing* | The slave has not synced to the master (master serial log `[RX] chunks received`). Shoulder abduction still works. |
| Invalid packets increase | Firmware payload changed; it must be 8 comma-separated values. |
| Angle drifts after several minutes | Game-rotation-vector heading drift: press **Recalibrate**. |
| Sync chip shows an error | Check the internet connection and Firestore rules. Records stay queued locally. |
