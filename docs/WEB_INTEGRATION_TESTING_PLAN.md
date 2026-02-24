# Web Integration Testing Plan

Issue: #702

## Context and Goals

The web frontend currently has:

- JavaScript module tests via `npm test` (`node --test`)
- WASM tests via `wasm-pack test --headless --chrome -- --features wasm`

What is missing is browser-level integration coverage that verifies the real UI, real DOM, real input events, and cross-module interactions in an actual browser runtime.

The goals for this first step are to:

1. Define how browser-level integration testing should be designed for NESER Web
2. Define what concrete test cases should be created
3. Keep the initial implementation incremental and CI-friendly

## Proposed Test Stack

### Framework

Use Playwright (`@playwright/test`) for browser integration tests.

Rationale:

- Stable, modern browser automation with first-class headless CI support
- Good keyboard/mouse/gamepad event simulation support
- Strong debugging artifacts (trace/video/screenshots)
- Supports local web server orchestration per test run

### Browsers

Phase 1: Chromium only (fast and stable baseline)

Phase 2+: Optional Firefox/WebKit expansion for compatibility confidence

### Test Directory Layout

Create a new folder under `web/`:

- `web/integration/fixtures/` (ROMs or metadata used by tests)
- `web/integration/helpers/` (test utilities: bootstrap app, wait helpers, status parsing)
- `web/integration/specs/` (Playwright test specs)

Suggested naming:

- `*.integration.spec.mjs`

This keeps browser integration tests separate from existing module unit tests.

## Test Environment Design

### Server Strategy

During integration test runs:

1. Build web artifacts
   - `bash scripts/build_web.sh`
2. Serve `web/` on localhost (prefer fixed port, e.g. 8000)
3. Run Playwright tests against the served app

Prefer Playwright `webServer` configuration so test runner owns server lifecycle.

### Deterministic Execution Principles

To minimize flakiness:

- Prefer assertions on deterministic UI state (status text, button enabled/disabled state, class toggles, ARIA attributes)
- Avoid fragile visual pixel-comparison assertions in phase 1
- Use generous but bounded waits with explicit conditions (`expect(...).toHaveText(...)`)
- Avoid timing-sensitive assertions for frame-perfect rendering in early phase

### ROM Strategy

Use tiny, deterministic ROM fixtures from `roms/automated_tests/` where possible.

Selection criteria:

- Fast startup
- Deterministic behavior
- No dependency on user interaction to reach stable state

If needed, keep one dedicated small fixture in `web/integration/fixtures/` that is known to boot reliably in browser tests.

## CI Integration Design

Add a new optional workflow job in `.github/workflows/rust.yml`:

- Job name: `web-integration`
- Triggered only when web-related paths change (same paths-filter principle as current `web` job)
- Steps:
  1. Checkout
  2. Setup Rust toolchain + wasm target + wasm-bindgen/wasm-pack (reuse existing patterns)
  3. Setup Node.js
  4. Install Playwright and browser deps
  5. Build web (`bash scripts/build_web.sh`)
  6. Run Playwright integration tests
  7. Upload traces/screenshots on failure

Incremental rollout:

- Initially non-blocking can be considered if flakiness appears
- Target end-state: blocking required check for PRs touching web frontend

## Initial Test Case Matrix

Prioritize highest user value and stability first.

### Group A: Boot and Core Lifecycle

1. **App shell renders**
   - Given app is opened
   - Then essential controls are present (`#start`, `#pause`, `#stop`, `#screen`, status label)

2. **Start from bundled ROM selection**
   - Given a bundled ROM is selected from `#rom-select`
   - When clicking `Start`
   - Then status transitions away from "Load a ROM to begin" and emulation starts

3. **Pause/Resume toggles running state**
   - Given emulation is running
   - When pressing `Pause/Resume`
   - Then status reflects paused state
   - When pressing again
   - Then running state is restored

4. **Stop returns to idle state**
   - Given emulation is running
   - When clicking `Stop`
   - Then status and controls return to idle-safe state

5. **Reset keeps emulation active and resets session**
   - Given emulation is running
   - When clicking `Reset`
   - Then app remains operational and status confirms reset action

### Group B: Input Routing and Runtime Controls

6. **Keyboard input reaches emulator only while running**
   - Given running state
   - When keyboard events are sent (W/A/S/D/F/G/R/T)
   - Then no error state appears and input path remains active
   - Given stopped state
   - Then input is ignored safely

7. **Gamepad toggle updates state and label**
   - Verify `Gamepad On/Off` text and `aria-pressed` consistency across toggles

8. **Mute toggle updates state and label**
   - Verify mute button text + `aria-pressed` and no runtime errors during toggles

9. **Filter toggle cycles available filters without crashes**
   - Ensure repeated toggling does not break rendering loop

10. **Zoom controls modify canvas presentation bounds safely**
   - `Zoom +` and `Zoom -` should change expected canvas dimensions/classes

### Group C: Save State Flows

11. **Save/Load buttons disabled before ROM start**
   - Validate initial disabled state

12. **Save state enabled after run and successfully stores state**
   - Start ROM, click save, assert status/toast and persistence side effect

13. **Load state restores after save in same session**
   - Save then load, assert successful restore status

14. **Load state with no saved state is handled gracefully**
   - Ensure user-visible safe error/info path

### Group D: Autorun and Dialog UX

15. **Autorun modal opens and basic controls are present**
   - Verify file input, checkpoint select, extend checkbox, use button

16. **Autorun use action disabled until valid file is provided**
   - Validate guard rails in UI state

17. **Cancel autorun control visibility changes correctly**
   - Verify `#autorun-cancel` hidden/visible transitions as autorun state changes

### Group E: Debugger and Overlay UX

18. **Shortcut help overlay toggles with shortcut key**
   - Verify visibility class/ARIA state changes

19. **Debugger panel toggles without breaking run loop**
   - Confirm panel visibility and no fatal errors in console

20. **Shortcut reference text renders expected content**
   - Validate baseline help content is present

## Phased Delivery Plan

### Phase 0 (Setup)

- Add Playwright dependencies and config
- Add local command:
  - `npm run test:integration:web`
- Add one smoke test to prove harness works in CI

### Phase 1 (Critical path)

Implement Group A tests + Group C.11.

Exit criteria:

- Browser harness stable in CI on Chromium
- Core start/pause/stop/reset behavior covered

### Phase 2 (Controls and persistence)

Implement Group B + Group C.12-C.14.

Exit criteria:

- Runtime controls and save/load browser flows are covered

### Phase 3 (Extended UX)

Implement Group D + Group E.

Exit criteria:

- Autorun and debugger/help overlays have baseline integration coverage

## Flakiness and Reliability Controls

- Add shared helper `waitForRunningState()` and `waitForIdleState()` to reduce repeated ad-hoc waits
- Capture Playwright traces only on retry/failure
- Keep each test independent (no cross-test save-state dependence)
- Use deterministic fixture ROM and avoid random inputs
- Keep retries low (e.g., 1 in CI) to surface real regressions quickly

## Reporting and Debugging

On failure, collect:

- Playwright trace
- Screenshot
- Browser console logs (per test)

This shortens diagnosis time for CI-only failures.

## Risks and Mitigations

1. **WASM startup timing variability in CI**
   - Mitigation: explicit readiness checks on status text and controls

2. **IndexedDB behavior differences across environments**
   - Mitigation: keep save-state tests self-contained and isolate storage keys per test

3. **Headless browser input quirks**
   - Mitigation: assert UI-observable state transitions first, low-level input semantics later

4. **Test runtime growth**
   - Mitigation: keep phase-1 suite small; parallelize by file when stable

## Acceptance Criteria for Issue #702 (Planning Step)

This planning step is complete when:

- Test architecture is defined (framework, layout, CI approach)
- Initial browser integration test catalog is defined and prioritized
- Incremental rollout strategy is clear and actionable

This document satisfies those criteria and is intended to guide implementation in subsequent issue increments.
