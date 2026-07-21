/**
 * GTFS Validator Web Worker
 *
 * This worker runs the WASM validator in a separate thread to avoid blocking the main UI.
 *
 * Usage:
 *   const worker = new Worker(new URL('./worker.js', import.meta.url), { type: 'module' });
 *   worker.postMessage({ type: 'validate', payload: { zipBytes, countryCode }, id: 1 });
 *   worker.onmessage = (e) => console.log(e.data);
 */

let wasm;
let runtime = 'single-threaded';
let initializationPromise;

function canUseThreads() {
  // Diagnostic override for golden comparisons and benchmarks. It never
  // enables threads when the browser or hosting environment cannot support
  // them; it only forces the portable fallback.
  if (new URL(self.location.href).searchParams.get('threads') === 'off') {
    return false;
  }

  // iOS has a much tighter WASM memory budget and nested-worker support varies
  // across versions, so keep the conservative single-threaded path there.
  const isIOS = /iPad|iPhone|iPod/.test(navigator.userAgent)
    || (navigator.platform === 'MacIntel' && navigator.maxTouchPoints > 1);

  return !isIOS
    && self.crossOriginIsolated === true
    && typeof SharedArrayBuffer !== 'undefined'
    && typeof WebAssembly.Memory === 'function';
}

/**
 * Initialize the WASM module
 * @returns {Promise<void>}
 */
async function ensureInitialized() {
  if (!initializationPromise) {
    initializationPromise = (async () => {
      if (canUseThreads()) {
        try {
          const threaded = await import('../pkg-mt/gtfs_guru_wasm.js');
          await threaded.default();
          const hardwareThreads = navigator.hardwareConcurrency || 2;
          await threaded.initThreadPool(Math.min(Math.max(hardwareThreads, 1), 8));
          wasm = threaded;
          runtime = 'multi-threaded';
        } catch (error) {
          console.warn('Multi-threaded WASM initialization failed; using fallback.', error);
        }
      }

      if (!wasm) {
        const singleThreaded = await import('./gtfs_guru_wasm.js');
        await singleThreaded.default();
        wasm = singleThreaded;
      }
    })();
  }
  return initializationPromise;
}

/**
 * Handle incoming messages from the main thread
 */
self.onmessage = async (event) => {
  const { type, payload, id } = event.data;

  try {
    await ensureInitialized();

    switch (type) {
      case 'validate': {
        const { zipBytes, countryCode, date, includeHtml = true } = payload;
        const startTime = performance.now();

        const result = wasm.validate_gtfs(
          new Uint8Array(zipBytes),
          countryCode || null,
          date || null,
        );

        const elapsed = performance.now() - startTime;
        try {
          const json = result.take_json();
          const html = includeHtml ? result.take_html() : '';
          const timingsJson = result.take_timings_json();
          self.postMessage({
            id,
            type: 'result',
            payload: {
              json,
              html,
              errorCount: result.error_count,
              warningCount: result.warning_count,
              infoCount: result.info_count,
              isValid: result.is_valid,
              validationTimeMs: elapsed,
              timings: JSON.parse(timingsJson),
              runtime,
            },
          });
        } finally {
          // Release Rust-owned strings immediately instead of waiting for JS
          // finalization, which is important after large-feed validation.
          result.free();
        }
        break;
      }

      case 'version': {
        self.postMessage({
          id,
          type: 'version',
          payload: wasm.version(),
        });
        break;
      }

      default:
        self.postMessage({
          id,
          type: 'error',
          payload: `Unknown message type: ${type}`,
        });
    }
  } catch (error) {
    self.postMessage({
      id,
      type: 'error',
      payload: error instanceof Error ? error.message : String(error),
    });
  }
};

// Initialize eagerly. Consumers only receive "ready" after the WASM module and
// (when available) its Rayon worker pool are ready for validation.
ensureInitialized()
  .then(() => self.postMessage({ type: 'ready', payload: { runtime } }))
  .catch((error) => self.postMessage({
    type: 'error',
    payload: error instanceof Error ? error.message : String(error),
  }));
