# Release DMG Stability Design

Date: 2026-04-15
Topic: Stabilize macOS release packaging for dual-architecture DMG builds

## Problem

The current release flow runs `tauri build --target <target> --bundles dmg` for both:

- `aarch64-apple-darwin`
- `x86_64-apple-darwin`

The arm64 DMG build succeeds consistently. The x64 DMG build fails consistently inside the generated `bundle_dmg.sh` script while running the Finder-prettifying AppleScript step (`osascript`). Because the release script requires both targets to succeed before it commits, tags, pushes, and uploads assets, the entire `release:patch` flow aborts.

## Goal

Keep dual-architecture macOS release output, but remove the unstable dependency on Finder/AppleScript DMG customization so the automated release flow can complete reliably.

## Non-Goals

- Preserve custom DMG background, icon positioning, or Finder window layout.
- Change versioning policy, changelog generation, or GitHub Release semantics.
- Change non-macOS release behavior.

## Chosen Approach

Keep the existing two-target build loop in `scripts/build-release-bundles.mjs`, but pass a stable DMG configuration that skips the Finder-prettifying AppleScript step for macOS DMG creation.

The expected implementation is:

1. Detect that the release bundle build is invoking macOS DMG packaging.
2. Supply the generated `create-dmg` script with the equivalent of `--skip-jenkins`.
3. Leave the rest of the release flow unchanged:
   - version bump
   - changelog update
   - dual-target DMG build
   - release commit
   - git tag and push
   - GitHub Release creation/update
   - DMG asset upload

## Trade-Offs

Pros:

- Stabilizes the release path at the known failure point.
- Preserves both arm64 and x64 artifacts.
- Minimizes code change and release-process risk.

Cons:

- Produced DMGs lose Finder visual polish.
- The fix addresses reliability, not the root cause of the AppleScript failure.

## Implementation Notes

- Prefer a scoped release-build fix over broad Tauri config changes.
- If Tauri exposes a bundle argument path that can pass `--skip-jenkins`, use that directly.
- If not, patch the generated DMG bundling invocation in the release build pipeline with the smallest practical change surface.
- Avoid changing `release-auto.mjs` unless necessary; the build step failure is isolated to `scripts/build-release-bundles.mjs` and downstream DMG tooling.

## Validation

Success criteria:

- `npm run release:patch -- --ci` completes successfully.
- Both DMGs exist after the build:
  - `src-tauri/target/aarch64-apple-darwin/release/bundle/dmg/Open Recorder_<version>_aarch64.dmg`
  - `src-tauri/target/x86_64-apple-darwin/release/bundle/dmg/Open Recorder_<version>_x64.dmg`
- The release flow creates and pushes tag `v<version>`.
- GitHub Release `v<version>` exists and both DMGs are uploaded.

## Risks

- Tauri may not expose a clean flag pass-through for the generated DMG script, requiring a targeted workaround in the repo.
- If the x64 failure is caused by a later `bundle_dmg.sh` step rather than AppleScript alone, we may need one additional iteration after first implementation.
