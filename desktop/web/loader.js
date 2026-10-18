import init, { run_web } from './pkg/optic_forge_ui.js';
const loading = document.getElementById('loading');
try {
  await init();
  // GPUI initializes graphics asynchronously; Rust signals when the window is open.
  window.addEventListener('optic-forge-ready', () => loading.remove(), { once: true });
  run_web();
} catch (error) {
  loading.textContent = `Could not start the GPUI app: ${error.message ?? error}. Use a browser with WebGPU or WebGL2 support.`;
}
