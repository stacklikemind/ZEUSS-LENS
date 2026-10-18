# Browser and delivery verification

Verified on 2026-10-18 in the Codex in-app browser at `http://localhost:8766/` using the compiled Rust WASM module.

- Initial two-eye zero-power generation completed and displayed a circular symbol with successful in-memory checks.
- An unsupported left-eye pair (+3.75 sphere, −4.00 cylinder, axis 160°) was rejected without substitution. Previous output was marked stale and downloads disabled.
- Synthetic right-eye input −1.00 sphere, −0.50 cylinder, axis 21° displayed its encoded axis 20° and an explicit warning in native-axis mode. Exact-axis mode rejected it.
- Left-eye +2.25 sphere, −4.00 cylinder, axis 160° remained unchanged in the encoded-value report; its separate calibration sphere +1.75 was clearly shown as clamped.
- The browser was reloaded after the final arc-order correction, and all three download controls produced files. The browser download-event notification timed out, so file creation and content were verified directly in the local Downloads directory instead of relying on that notification.
- The exported PNG was 1760 × 1760 pixels. SVG and PNG payloads matched the JSON report in separate raster checks.
- The corrected held-out wraparound regression passed through the delivered WASM ABI. The 399,858-byte module has zero imports.
- The interface was visually inspected at the browser's default 1280-pixel width. No narrow-viewport visual verification is claimed.
- A screenshot of the final neutral example is saved as `browser-preview.jpg`.

See `INTEROPERABILITY_REPORT.json` for 64 held-out symbol comparisons, 24 raster comparisons and 64 prescription comparisons; `STANDALONE_BUILD_REPORT.json` records a successful offline/locked build and 14 passing tests from a separate temporary source copy. The installed Rust toolchain lacks Clippy, so no Clippy result is claimed. Hardware enrollment and camera recognition remain unverified.
