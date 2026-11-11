# Changelog

All notable changes to this project are documented in this file.

## [0.1.0.0] - 2026-11-11

### Added

- **Kinetism Desktop for Windows** (`apps/kinetism_desktop`), a new app for
  rehab sessions with the Kinetism sensors:
  - Measure **shoulder abduction** live. A two-pose calibration works with any
    sensor mounting. The app warns about compensation, suggests an MMT grade
    (0-3) that the physiotherapist confirms or overrides (up to 5), and saves
    every attempt. Elbow flexion uses the phone app's force and MMT model.
  - Play the **Fruit PNF game**. Level 0 traces an apple in the air, Level 1
    slices fruit along the PNF D1 diagonal, and Level 2 along D2. It has bombs
    off the diagonal, combos, stars, a daily streak and voice encouragement.
    Scoring 80% unlocks the next level, and a physio override unlocks all
    levels. Stars, the saved score and unlocking always agree.
  - Follow **progress per muscle**: a body map coloured by the latest MMT
    grade, a daily MMT grid, MMT and game-score trends, and draft recovery
    guidelines for each grade.
  - Connect sensors from one menu: the Physio_A Bluetooth pair, a Wi-Fi KIN1
    sensor (UDP port 4210) or a built-in simulator. The status bar shows the
    streaming rate, battery and IMU accuracy, and says when the upper-arm or
    forearm sensor is missing.
  - Records, patients and game sessions sync with the phone app's Firestore
    project. Everything is saved on the PC first and queued until Firestore
    accepts it, so sessions work offline. Firestore is read at start-up,
    every 5 minutes and from the sync chip, staying well inside the free
    plan's daily read quota.
  - Patient data stays trustworthy:
    - Measurements and the game pause when upper-arm data stops, so a frozen
      angle or a sensor dropout is never saved as a result.
    - Opening another patient, or another Wi-Fi sensor taking over, always
      starts a new calibration.
    - Grades and posture notes are judged at the peak of the attempt.
    - A baseline on the healthy arm leaves the patient's affected side and
      arm sizes unchanged.
    - Simulator attempts and rounds are practice only: never saved or synced.
    - Offline (`KINETISM_OFFLINE`) and test-project data keep their own
      local files and never reach the shared project.
    - A newer correction is never lost behind an older queued save, a save
      only counts once Firestore confirms the document (a proxy or Wi-Fi
      sign-in page cannot swallow it), and an unreadable local cache is kept
      as a backup instead of being replaced.
- Download the app without installing Rust: every `v*` tag publishes
  `kinetism-desktop-<tag>-windows-x64.zip` on GitHub Releases. Pull requests
  that touch the app run its 114 unit tests, formatting and lint checks on
  Windows, with a pinned Rust toolchain and pinned actions.
- `firmware/physio-slave`: a buildable PlatformIO version of the Physio_A
  forearm (slave) sensor firmware. It starts streaming even when its IMU only
  answers on a retry, and resets an IMU that goes silent. It compiles but has
  not been tested on a board yet.
- `AGENTS.md` lists the Rust, Python and Flutter test suites and how to run
  them.

### Fixed

- The clinician phone app's Flutter test suite passes again: an outdated
  counter template test was replaced with smoke tests for the app shell.
