# Kinetism

Flutter rehabilitation prototype, refined in place around the existing Firebase
and paired-arm sensor integrations. This is not a clinically validated device.

## What works in this prototype

- Shared session log with search, exercise filters, actual daily repetitions and
  a chart limited to seven recorded sessions.
- Missing counts/dates stay unavailable, rather than becoming zero or today.
  Cached data and pending cloud writes are labelled separately.
- Exercise selection and validated exercise creation.
- Raw arm orientation, peak-to-rest repetition counting and explicit sensor,
  loading, video and save failures.
  On narrow screens, the sensor readout precedes optional demonstration video.
- Archived AI report drafts, exercise goals and Android daily reminders.
- Kinetism branding across app, browser and native launcher assets.

### Important clinical and product boundaries

| Requested patient-based v1 feature | Current implementation |
| --- | --- |
| Select/create patients | Not implemented. Session saves still use `patient_123`; these are shared prototype records. |
| Arm dimensions and illustrated landmarks | Not implemented. |
| Calibrated elbow angle | Raw segment orientation only, without anatomical hinge calibration. |
| Force calculation | Not implemented. A clinical mass/force method and validation are still required. |
| AI MMT suggestion and therapist confirmation | Not implemented. Exercise coaching/report drafts are not MMT grades. |
| Capture a position and retain every attempt | Existing session saves record repetitions and date, not angle/force/MMT attempts. |
| Phone/PC patient-record synchronisation | No dedicated authenticated, read-only PC portal or patient sync protocol yet. |

Do not use these records for diagnosis or treatment decisions. The separate
synthetic preview must never be mistaken for real measurements.

## Setup

Validated with Flutter 3.47.9 and Dart 3.13.7. The SDK constraint is in
[pubspec.yaml](pubspec.yaml). Restore the supplied lockfile without upgrading:

```powershell
flutter pub get --enforce-lockfile
```

### Local synthetic preview

[lib/preview.dart](lib/preview.dart) runs without initialising Firebase, BLE,
AI, video playback or device notifications. Records, reports, goals and local
additions are synthetic and temporary. The top-right menu exposes every main
screen plus loading, empty, failure, cached and pending states.

```powershell
flutter run -d chrome --target lib\preview.dart
```

Build with local Flutter web-engine resources instead of the engine CDN:

```powershell
flutter build web --target lib\preview.dart --no-web-resources-cdn
```

The resulting `build\web` directory can be served by a local static HTTP server.
This is a presentation/testing preview, not the planned PC clinical portal.

### Connected prototype

[lib/main.dart](lib/main.dart) uses the existing Firebase configuration and
Firestore collections. Review credentials, database rules, hosting and patient
data governance before using a real backend or distributing a build. Never add
private credentials to source code.

AI coaching and new AI report generation are unconfigured in public builds.
Missing configuration and provider failures are shown as errors, not fabricated
recommendations; sensor readouts and existing report drafts remain available.
The former embedded AI API-key literal has been removed. Revoke or rotate that
old key before any further private use.

For private development only, a local build can read `KINETISM_GEMINI_API_KEY`
from an ignored `.env.json` file through `--dart-define-from-file`.
**Dart defines are embedded in the compiled app**, not a secret vault. Never
publish a build containing a provider key. A production AI integration needs an
authenticated server-side service and approved data governance.

```powershell
flutter run -d <android-device-id>
```

The Android BLE library requests platform-appropriate runtime scan/connect or
location permissions when scanning. Daily notification permission is requested
when enabling reminders, not at startup. Device notifications are Android-only.

## Preserved integration contracts

| Surface | Existing contract |
| --- | --- |
| BLE advertised name | `Physio_A` |
| BLE service | `4fafc201-1fb5-459e-8fcc-c5c9c331914b` |
| Notify characteristic | `beb5483e-36e1-4688-b7f5-ea07361b26a8` |
| Telemetry | Exactly eight finite CSV quaternion values, upper arm then forearm, each in `x,y,z,w` order. Invalid/zero orientations are rejected; valid orientations are normalised. |
| `Exercises` | `name`, `targetJoint`, `restingAngle`, `peakAngle`, `videoUrl`, `dailyGoal` |
| `SessionLogs` | `patientId`, `exerciseName`, `repsCompleted`, server `date` |
| `WeeklyReports` | Existing report writer retained; archive reads `date` and `aiAnalysis`. |

Repetitions count a peak followed by return to rest, including exact threshold
boundaries. They are not strength grades. Cloud writes are not confirmed backups
until the backend acknowledges them.

## Verification

Run the refinement regressions together:

```powershell
flutter test test\refinement_models_test.dart test\refinement_widgets_test.dart test\refinement_notices_test.dart test\refinement_error_layout_test.dart test\publishing_configuration_test.dart test\clinical_workflow_test.dart
flutter analyze --no-pub --no-fatal-infos lib test\refinement_models_test.dart test\refinement_widgets_test.dart test\refinement_notices_test.dart test\refinement_error_layout_test.dart test\publishing_configuration_test.dart test\clinical_workflow_test.dart
powershell -NoProfile -ExecutionPolicy Bypass -File tool\check_branding.ps1
```

Coverage includes malformed telemetry, threshold boundaries, exact saved recipe
payloads, missing record fields, search/filter clearing, recoverable startup,
cloud failures, pending-write status, reminder state and save completion after
navigation. Principal screens are checked at narrow widths and 2x text scaling.
Tests also enforce 4.5:1 theme text contrast, readable body sizes, themed 48px
button minimums, a named reminder switch and non-zero-only data-quality notices.
Validation instructions wrap instead of ending in an ellipsis. Rendered-text
tests verify the complete goal and recipe errors at 320px and 2x text scaling;
the goal dialog scrolls when its contents exceed the available height.
Publication regressions also verify that a build without an AI key fails
explicitly instead of manufacturing coaching praise or a session recommendation.

The original [counter-template smoke test](test/widget_test.dart) remains
unchanged. It failed before refinement with Firebase `core/no-app` and still
expects the starter counter, not this app. Do not treat the full test directory
as a verified green suite. An existing brace-style lint in the untouched
repetition classifier also remains.

Synthetic checks do not validate physical sensors, real Firebase permissions,
cloud persistence, notification delivery or clinical accuracy.

## Android APK

Install an Android SDK and a compatible JDK, then confirm them with
`flutter doctor -v`. For a development-only APK:

```powershell
flutter build apk --debug
```

[Download the Android test APK](https://github.com/tansiang9389-gif/Kinetism-motion-telemetry/releases/download/prototype-refinement-20261031/kinetism-prototype-1.0.0-debug.apk)
from the [development release](https://github.com/tansiang9389-gif/Kinetism-motion-telemetry/releases/tag/prototype-refinement-20261031).
The local build output is `build\app\outputs\flutter-apk\app-debug.apk`.
It was rebuilt from the connected main entry, not the synthetic preview,
without the former private AI-key literal. AI coaching and new AI report
generation are unconfigured in this public build.

| Development package | Verified value |
| --- | --- |
| Application ID | `com.example.medical_device_app` |
| Label/version | Kinetism, `1.0.0+1` |
| Minimum Android | Android 7.0, API 24 |
| Compile/target SDK | Android 16, API 36 |
| Native runtimes | `arm64-v8a`, `armeabi-v7a`, `x86_64` |
| Size | 193,444,998 bytes, 184.5 MiB |
| Signing | Android Debug certificate; APK v2 signature verified |
| SHA-256 | `652ca70a3f0ed621bf534ba67ea76457e2dc075bacbd08b2eaced4b37e0f7b28` |

Manifest, runtime entries and signature checks passed. Physical installation,
BLE operation, backend persistence and notification delivery were not tested.
Download the APK onto a dedicated test phone. Android may ask you to allow
installation from the browser or file manager you used.
A different debug certificate may prevent updating an older
installation. Do not uninstall an app containing records just to force an update.

The build used JDK 21 and Android SDK 36. Existing Gradle/AGP/Kotlin versions
produced future-support warnings, and the SDK XML parser emitted a compatibility
warning; compilation still succeeded without bypassing dependency validation.
Flutter added its legacy Kotlin/DSL compatibility flags automatically.

For a release build, first configure a private production signing key and
approved distribution policy in [Android build configuration](android/app/build.gradle.kts).
The supplied release configuration still signs with the debug key. A successful
release compilation alone does not make it suitable for clinical distribution.
Application/bundle identifiers and native executable names were preserved.

## Branding

The original mark is in [assets/branding/kinetism-mark.svg](assets/branding/kinetism-mark.svg).
Regenerate native/web icons and launch images on Windows:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tool\generate_branding.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File tool\check_branding.ps1
```

The checker verifies PNG dimensions/opacity, Windows ICO entries, Android
adaptive-icon safe-zone geometry and resource XML.
