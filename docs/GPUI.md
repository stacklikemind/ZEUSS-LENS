# GPUI application

[Run in your browser](https://zeuss-lens.pages.dev/gpui/) · [Download desktop packages](https://github.com/stacklikemind/zeuss-lens/releases)

The `desktop/` crate contains one Rust GPUI view used by the native app and the browser build. It calls the existing encoder library directly. Images, format tables, icons, and a small font are embedded; generation works without a network connection after the browser assets have loaded. No separate runtime, web server, or browser is required by the native executable.

## Native builds

Install Rust and the platform development libraries required by [GPUI Kit](https://gpui-kit.com/docs/installation/). Then:

```sh
cargo build --manifest-path desktop/Cargo.toml --locked --release --bin zeuss-lens
desktop/target/release/zeuss-lens --self-test
python3 scripts/package-desktop.py
```

The build workflow produces separate packages for Apple Silicon macOS, Intel macOS, Windows x64, and Linux x64. Native machine code is specific to its OS and processor; a single executable does not run across all platforms.

The Intel macOS package is cross-compiled on an Apple Silicon runner. The workflow tests the shared Rust code on that runner, but the Intel executable needs an Intel Mac or Rosetta for a runtime smoke test.

- macOS: extract the ZIP and double-click `ZEUSS LENS.app`. The bundle contains one executable, with all runtime assets embedded. Builds are ad-hoc signed, not Developer ID signed or notarized; macOS may require approval before opening a downloaded app.
- Windows: extract the ZIP and double-click `zeuss-lens.exe`. Builds are not Authenticode signed, so Windows may display a reputation prompt.
- Linux: use the AppImage on a compatible x64 desktop with graphics drivers. Set its executable permission if the downloader removed it, then double-click it. The TAR archive also supplies a native executable for systems with the required libraries installed. AppImage may require FUSE; `APPIMAGE_EXTRACT_AND_RUN=1 ./zeuss-lens-linux-x86_64.AppImage` is the extraction fallback.

The engine smoke test exercises payload decoding and PNG output without opening a window. It is not a GPU or hardware-enrollment test.

## Browser build

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.129 --locked
./scripts/build-gpui-web.sh
node scripts/serve.mjs
```

Open `http://localhost:8766/gpui/`. The build emits the GPUI frontend and encoder together as WASM, with a small JavaScript loader. It uses WebGPU when available and falls back to WebGL2. The font and selected icons are embedded, with no CDN dependency. The primary GPUI app is also available at `/`; the lightweight HTML frontend is at `/lightweight.html`.

The GPUI browser backend is experimental upstream. Keyboard input, graphics initialization, layout, and export behavior must be verified in the target browser. See [GPUI Kit's WebAssembly guide](https://gpui-kit.com/docs/webassembly/). The browser app needs HTTP(S); double-clicking an HTML file via `file:` is not a supported WASM loading path.

## Behavior

Inputs cover both eyes, presence, sphere, cylinder, axis, explicit VR add, lens type, native/exact axis handling, and random/fixed identities. A fixed identity tries once; fresh-identity mode retries topology failures only, up to 64 attempts, without changing the optics. Editing any input regenerates the code automatically. Incomplete or unsupported input clears the previous output. Fields show the encoder's supported minimum and maximum values; sphere and cylinder bounds depend on the other optical value, and each pair is checked against the finite table. Unsupported values produce an error rather than substitution.

These code-table bounds do not represent all prescriptions Apple Vision Pro or ZEISS can support. Apple refers users to the [ZEISS prescription guide](https://www.zeiss.com/vision-care/us/zeiss-optical-inserts/prescription-guide.html) for optical-insert eligibility. This app only checks whether it can encode a value, not whether a lens can be made or safely used.

SVG and 1760 × 1760 PNG exports preserve the complete symbol. JSON includes the request, synthetic identity, actual encoded values, calibration calculations, warnings, and checks. Native exports use a save dialog; browser exports use downloads. The app does not persist entered values automatically.

Dependency notices are included in native archives and at `/THIRD-PARTY-NOTICES.txt` in the web build. They do not occupy a section in the application interface.
