# AGENTS.md

Guidance for coding agents working in this repository.

## Testing

Run all three suites before shipping. Every suite must pass.

| Suite | Directory | Command |
| --- | --- | --- |
| Kinetism Desktop (Rust; unit tests live in `src/`) | `apps/kinetism_desktop` | `cargo test` |
| PC dashboard and telemetry protocol (Python) | repository root | `python -m pytest -q` |
| Clinician phone app (Flutter) | `apps/medical_device_app` | `flutter test` |

The desktop app must also stay clean under `cargo fmt --check` and
`cargo clippy --all-targets -- -D warnings`. GitHub Actions
(`.github/workflows/kinetism-desktop.yml`) runs those checks and `cargo test`
on Windows for every pull request that touches the app.

Expectations:

- New logic gets a test next to it: a Rust `#[cfg(test)] mod tests` block,
  `tests/test_*.py` for Python, or `test/*_test.dart` for Flutter.
- Every bug fix gets a regression test.
- Never commit code that makes an existing suite fail.
