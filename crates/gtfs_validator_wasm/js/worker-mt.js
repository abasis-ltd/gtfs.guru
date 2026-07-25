/**
 * GTFS Validator Web Worker — multithreaded (wasm threads) variant.
 *
 * Same message protocol as worker.js, but after init() it spins up a
 * wasm-bindgen-rayon thread pool (backed by nested Web Workers). The rayon
 * pool parallelizes CSV parsing (stop_times) and the validator run.
 *
 * Requires a cross-origin-isolated page (COOP: same-origin + COEP:
 * require-corp) so SharedArrayBuffer is available. The site only loads this
 * worker when crossOriginIsolated is true; otherwise it uses ../pkg/worker.js.
 */

import init, { validate_gtfs, version, initThreadPool } from './gtfs_guru_wasm.js';

let initialized = false;

// Cap the pool: beyond ~8 workers the marginal gain fades while per-thread
// stack memory keeps growing, which matters for large feeds in wasm32.
const MAX_THREADS = 8;

async function ensureInitialized() {
  if (initialized) return;
  await init();
  const threads = Math.max(
    1,
    Math.min(MAX_THREADS, self.navigator?.hardwareConcurrency || 4),
  );
  // Must run before any rayon parallel iterator, or the first one panics.
  await initThreadPool(threads);
  initialized = true;
}

self.onmessage = async (event) => {
  const { type, payload, id } = event.data;

  try {
    await ensureInitialized();

    switch (type) {
      case 'validate': {
        const { zipBytes, countryCode, date } = payload;
        const startTime = performance.now();

        const result = validate_gtfs(
          new Uint8Array(zipBytes),
          countryCode || null,
          date || null,
        );

        const elapsed = performance.now() - startTime;

        self.postMessage({
          id,
          type: 'result',
          payload: {
            json: result.json,
            html: result.html,
            errorCount: result.error_count,
            warningCount: result.warning_count,
            infoCount: result.info_count,
            isValid: result.is_valid,
            truncated: result.truncated,
            validationTimeMs: elapsed,
          },
        });
        break;
      }

      case 'version': {
        self.postMessage({ id, type: 'version', payload: version() });
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

// Signal that the worker is ready (module loaded). Note: the thread pool is
// initialized lazily on the first 'validate' message, not here.
self.postMessage({ type: 'ready' });
