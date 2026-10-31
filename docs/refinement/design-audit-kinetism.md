# Kinetism refinement review

Status: DONE_WITH_CONCERNS. Existing prototype/UI and development APK verified;
physical-device, backend and clinical validation remain open.

## Scope and evidence

OPERATE: an existing Flutter rehabilitation prototype, not a marketing site.
Flutter/Firebase and the existing sensor contract were retained. The dark
presentation follows the supplied prototype; suitability for clinic daylight
has not been validated.

Six screens were rendered at verified CSS viewports of 375x812, 768x1024 and
1440x900, with three completed interaction workflows and five recovery states.
All data is synthetic. The final browser driver is locally installed
Chrome controlled by Playwright, with non-local requests blocked. The integrated
browser's screenshot operation reset viewport emulation; its captures are not
used as responsive evidence. The packaged gstack daemon was unavailable after
three startup attempts with missing runtime modules.

There is no rendered original pre-change baseline. No original-to-refined score
delta or before-change screenshots are claimed. The pairs below concern only
the visual QA adjustments made after the first refinement implementation.
The original audit used local-only evidence. This portable report and its
synthetic screenshots are included in the requested GitHub publication.

## First impression

I see a working exercise log, not a product landing page. My eye goes to
**Session log**, **Start exercise**, then the actual repetition totals. The
exercise chart and session rows have distinct jobs. On the phone, the repeated
preview warnings take too much space before the records.

One word: practical.

## Inferred design system

- Native Material UI typography, not a marketing display face. Measured theme
  sizes: 28/22/18px headings, 16px body and 14px secondary text.
- Cool dark ground, raised surfaces and one teal accent. Theme tokens include
  background `#101B22`, surface `#192A33`, accent `#57DBC4`, secondary text
  `#B7C9D1` and main text `#F0F6F8`. Five tested text-color pairings meet 4.5:1.
- A 4px spacing rhythm, 8px form/button corners and constrained workspaces.
  Phone layouts are single-column; wide forms/readouts retain useful columns.
- Buttons have themed minimum dimensions of 48x48px. Labels stay visible when
  fields contain values. Status uses words/icons as well as color.

Flutter paints text into its view. Ordinary DOM font/color inventories are not
a faithful census of painted styles; no exact font-usage counts are claimed.
The theme and original branding assets remain the design source of truth.
No unrequested design-system document was added.

## Visual QA findings and fixes

| ID | Impact/category | Observation and fix | Before | After | Fix status |
| --- | --- | --- | --- | --- | --- |
| F001 | Medium/responsive | Optional video preceded the live readout on narrow screens. Raw orientation now comes first; desktop columns remain. | [Phone](screenshots/tracker-mobile.png), [tablet](screenshots/tracker-tablet.png) | [Phone](screenshots/tracker-mobile-after.png), [tablet](screenshots/tracker-tablet-after.png) | Verified |
| F002 | Medium/content | Duplicate preview wording and zero-issue warning clauses pushed records down. Record-specific copy is shorter and lists only non-zero missing fields, retaining uncertainty. | [Log](screenshots/home-mobile.png) | [Log](screenshots/home-mobile-after.png) | Verified |
| F003 | Polish/layout | Form/tracker content vertically floated within the shared grid. Both now start at the top. | [Form](screenshots/add-desktop.png), [tracker](screenshots/tracker-desktop.png) | [Form](screenshots/add-desktop-after.png), [tracker](screenshots/tracker-desktop-after.png) | Verified |
| F004 | Medium/interaction | Reminder switch had no accessible name. Its existing title is now associated with the control. | [Reminder](screenshots/goals-mobile.png) | [Reminder](screenshots/goals-mobile-after.png) | Verified by semantics test and named browser interaction; pixels alone do not prove this fix |
| F005 | Medium/interaction | Matching searches lacked a clear action. Named search clearing now preserves the selected exercise filter. | [Missing action](screenshots/finding-005-before.png) | [Visible clear action](screenshots/flow-log-details.png), [cleared](screenshots/flow-log-cleared.png) | Verified, including selected-filter preservation regression |
| F006 | Medium/content | Goal validation cut off "greater than zero". Shared form errors now wrap; the goal dialog scrolls for long/enlarged content. | [Clipped instruction](screenshots/finding-006-before.png) | [Complete instruction](screenshots/flow-goal-submitted.png), [phone capture](screenshots/flow-goal-invalid-mobile.png) | Verified by desktop image and rendered-text tests at 320px, 1x/2x |

The original review used a supplied non-Git Flutter folder. The source is now
published under `apps/medical_device_app`; per-fix historical commits are not
invented. The visual QA changes touch:

| Findings | Files changed |
| --- | --- |
| F001/F003 | [arm_tracker_view.dart](../../apps/medical_device_app/lib/arm_tracker_view.dart) |
| F002/F005 | [dashboard.dart](../../apps/medical_device_app/lib/dashboard.dart) |
| F003 | [add_exercise.dart](../../apps/medical_device_app/lib/add_exercise.dart) |
| F004/F006 | [prescriptions.dart](../../apps/medical_device_app/lib/prescriptions.dart) |
| F006 | [app_design.dart](../../apps/medical_device_app/lib/app_design.dart), [rendered-error regressions](../../apps/medical_device_app/test/refinement_error_layout_test.dart) |

## Page and flow review

| Surface | Scan/wayfinding result | Final responsive evidence |
| --- | --- | --- |
| Session log | PASS: brand, page, primary action, search and filters are clear | [Phone](screenshots/home-mobile-after.png), [tablet](screenshots/home-tablet-after.png), [desktop](screenshots/home-desktop-after.png) |
| Exercise selector | PARTIAL: task and back path clear; search is not relevant to this short list | [Phone](screenshots/exercises-mobile-after.png), [tablet](screenshots/exercises-tablet-after.png), [desktop](screenshots/exercises-desktop-after.png) |
| Exercise form | PARTIAL: persistent labels, explicit save, local validation and back path | [Phone](screenshots/add-mobile-after.png), [tablet](screenshots/add-tablet-after.png), [desktop](screenshots/add-desktop-after.png) |
| Tracker | PARTIAL: raw data leads on phone; connection/save actions and prototype limits remain | [Phone](screenshots/tracker-mobile-after.png), [tablet](screenshots/tracker-tablet-after.png), [desktop](screenshots/tracker-desktop-after.png) |
| Report archive | PARTIAL: drafts are distinguished from clinical grades; unusable report dates disable opening | [Phone](screenshots/reports-mobile-after.png), [tablet](screenshots/reports-tablet-after.png), [desktop](screenshots/reports-desktop-after.png) |
| Goals/reminders | PARTIAL: targets and reminder have distinct jobs; edit action is named | [Phone](screenshots/goals-mobile-after.png), [tablet](screenshots/goals-tablet-after.png), [desktop](screenshots/goals-desktop-after.png) |

The subpages use native page titles/back navigation rather than a repeated
marketing header. The preview menu is test navigation, not a PC portal design.

1. I search for "wrist", open the specific record and see **8 repetitions** and
   the stored ID. Closing it and clearing search returns the other records.
   [Details](screenshots/flow-log-details.png), [reset](screenshots/flow-log-cleared.png).
2. I try a zero goal. Validation stays near the field, then corrected recipe
   values save to temporary data and the selector shows "Browser check curl".
   Confirmation explicitly says no cloud upload.
   [Invalid](screenshots/flow-recipe-invalid.png), [saved](screenshots/flow-recipe-saved.png).
3. I toggle the named synthetic reminder and edit a goal. Zero is rejected with
   the complete instruction; 20 is accepted and appears as the target.
   [Validation](screenshots/flow-goal-submitted.png), [saved](screenshots/flow-goal-saved.png).

Heuristic goodwill, not a user-study measurement:

```text
Start                     70/100
Obvious start/search      70 -> 80
Truthful preview limits   80 -> 85
Named search clearing     85 -> 90
Recoverable validation    90 -> 100
FINAL: healthy; no claim about real clinic usability
```

Accessibility-label inventories contain 137/35/27/67/39/36 words for
log/selector/form/tracker/archive/goals. These are not precise pixel-visible
word counts. No promotional happy talk was identified; safety/status text is
useful content, not advertising.

## Verification and scores

- **65 selected regressions passed** for publication: the original 63 parsing,
  threshold, record, navigation and layout cases, plus two tests that reject
  missing AI configuration without fabricated recommendations.
- **7 PC telemetry protocol tests passed** for the unchanged PC/firmware source.
- RenderParagraph checks confirm full goal/recipe instructions at **320px and
  200% text**, not merely their presence in the accessibility tree.
- Analysis: **no errors or warnings**; one pre-existing classifier style info.
- Final synthetic web build succeeded.
- [Browser checks](browser-checks.json): **18 CSS-viewport-verified principal
  captures, three real synthetic workflows, zero console/page errors and zero
  attempted non-local requests**.
- [Recovery evidence](screenshots/state-error.png) includes
  [empty](screenshots/state-empty.png), [loading](screenshots/state-loading.png),
  [cached](screenshots/state-cache.png) and [pending](screenshots/state-pending.png).
- Branding checker passed 36 opaque PNGs, seven ICO entries and Android
  adaptive safe-zone/resource XML checks.
- Android SDK/JDK restoration and final connected-entry debug APK build
  succeeded. Package metadata, runtime entries and APK v2 signature verified;
  physical installation is not claimed.

### Development APK

[Download the Android test APK](https://github.com/tansiang9389-gif/Kinetism-motion-telemetry/releases/download/prototype-refinement-20261031/kinetism-prototype-1.0.0-debug.apk):
Kinetism `1.0.0+1`, application ID `com.example.medical_device_app`, Android 7.0+
(minimum API 24, target/compile API 36). Universal arm64/32-bit ARM/x86-64
runtime entries were inspected. Size: 193,364,032 bytes, 184.4 MiB.

SHA-256: `ceeaf9b83a50d830a38f91989faa96ee8c4240696815c48c622cf045f7452464`.
Android Debug certificate, verified APK v2 signing. This is a development
package, not a clinical or production release. It was rebuilt without the
embedded private AI-key literal, and all 312 APK entries were checked for that
removed credential. AI coaching/new AI report generation are unconfigured.
The package is distributed through the linked GitHub development prerelease.
Use a dedicated test phone/profile; do not erase an older installation's records
to work around a signing mismatch.

Existing Gradle/AGP/Kotlin future-support warnings and SDK XML compatibility
warning did not prevent compilation. No dependency-validation bypass was used.
Flutter's automatic legacy Kotlin/DSL property migration was inspected.

Grades concern this bounded synthetic UI only. Each category starts at A;
medium findings subtract half a grade and polish does not change the grade.
The "QA start" column is the first implemented refinement, not the original app.

| Category | Weight | QA start | Final |
| --- | --- | --- | --- |
| Hierarchy | 15% | A | A |
| Typography | 15% | A | A |
| Layout | 15% | A | A |
| Color/contrast | 10% | A | A |
| Interaction | 10% | B | A |
| Responsive | 10% | A- | A |
| Content | 10% | B | A |
| AI slop | 5% | A | A |
| Motion | 5% | A | A |
| Local performance feel | 5% | A | A |

Design score: **A (3.75/4) -> A (4/4)** for the final QA subset.
AI slop score: **A -> A**. Calm native task UI; no decorative feature-card
mosaic or marketing theater. Native state transitions serve feedback.
Performance feel is qualitative/local; LCP, CLS and physical-device performance
were not measured. No original-to-refined grade delta is available.

Litmus checks: brand on first screen **yes**; clear visual anchor **yes**;
headline scanability **yes**; one job per area **yes**; cards earned by distinct
media/readout interactions **yes**; state motion useful **yes**; understandable
without decorative shadows **yes**. This is primary-review judgment, not
independent consensus.

### Quick wins completed

1. Put telemetry before optional video on phones.
2. Remove duplicate/zero-issue notice wording.
3. Top-align workspaces.
4. Name the reminder and search-clear controls.
5. Wrap validation instructions and let the goal editor scroll.

Summary: **6 findings; 6 verified fixes; 0 best-effort, reverted or deferred
design fixes**. Detector: **not run**, not a clean mechanical scan. No
independent outside reviewer ran; coverage is primary-only.

> Design review found 6 issues, fixed 6. Design score A -> A, AI slop A -> A
> (final visual-QA subset only; no original rendered baseline).

Six observed operational learnings were logged locally: startup callback
return values, async route ownership, dialog-controller exit lifetime,
validation visibility, Windows SDK ZIP extraction and native-browser viewport
verification. Automated review telemetry remains off; GitHub publication of
source, the APK and synthetic evidence was explicitly requested.

## Separate product and delivery concerns

- Patient selection, anthropometric landmarks, position-capture attempts,
  calibrated force and therapist-confirmed MMT are not implemented.
- Prototype saves still use the shared prototype patient identifier.
- The former AI key and previously archived Wi-Fi credentials should be
  revoked/rotated. Current-source removal does not erase old Git history.
- The browser preview is not an authenticated PC clinical portal.
- Initial APK preflight lacked an Android SDK. A checksum-verified local JDK/SDK
  was restored; the development APK now builds and passes structural/signing checks.
- Physical sensors, real Firebase writes/rules and Android notifications have
  not been exercised. Clinical validation and production signing remain open.
- The original counter-template test already failed before refinement.
- Independent outside-review coverage and a mechanical design detector are absent.
