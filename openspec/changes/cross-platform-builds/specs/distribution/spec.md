## ADDED Requirements

### Requirement: CI verification matrix

The continuous integration workflow SHALL build the app and run its test suite on Linux, Windows and macOS for every push and pull request. Formatting, clippy, the release build and the release performance gate SHALL run on the Linux job, which is the calibration platform for the gate budgets. The performance gate SHALL NOT run on Windows or macOS.

#### Scenario: All three platforms are built

- **WHEN** a push or pull request reaches CI
- **THEN** a Linux, a Windows and a macOS job each compile the crate and run the tests that are not gated to Linux

#### Scenario: Perf gate stays on its calibration platform

- **WHEN** the Windows or macOS job runs the test suite
- **THEN** the budgeted performance phases are not part of that run, and a Linux failure still fails the workflow

### Requirement: Release artifacts per platform

On a `v*` tag the workflow SHALL attach a runnable artifact for each supported desktop platform: a Windows x86_64 zip containing `droid_tui.exe`, and a macOS universal `.app` bundle zipped after joining arm64 and x86_64 builds. Artifacts SHALL be unsigned, and the documentation SHALL state so together with how to open them.

#### Scenario: Tag produces both artifacts

- **WHEN** a tag matching `v*` is pushed
- **THEN** the workflow publishes a Windows zip and a macOS universal `.app` zip on the release

#### Scenario: Artifact starts without the repository

- **WHEN** a user unpacks the Windows or macOS artifact on a machine without the source tree
- **THEN** the app starts with embedded circuit geometry and writes its configuration under the platform data directory
