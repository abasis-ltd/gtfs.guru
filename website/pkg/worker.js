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

import init, { validate_gtfs, version } from './gtfs_validator_wasm.js';

let initialized = false;

/**
 * Initialize the WASM module
 * @returns {Promise<void>}
 */
async function ensureInitialized() {
  if (!initialized) {
    await init();
    initialized = true;
  }
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
        const { zipBytes, countryCode } = payload;
        const startTime = performance.now();

        const result = validate_gtfs(new Uint8Array(zipBytes), countryCode || null);

        const elapsed = performance.now() - startTime;

        self.postMessage({
          id,
          type: 'result',
          payload: {
            json: result.json,
            errorCount: result.error_count,
            warningCount: result.warning_count,
            infoCount: result.info_count,
            isValid: result.is_valid,
            validationTimeMs: elapsed,
          },
        });
        break;
      }

      case 'version': {
        self.postMessage({
          id,
          type: 'version',
          payload: version(),
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

// Signal that the worker is ready
self.postMessage({ type: 'ready' });
