/**
 * GTFS Validator - Main Thread Wrapper
 *
 * Provides a Promise-based API for validating GTFS feeds using a Web Worker.
 *
 * Usage:
 *   import { GtfsValidator } from 'gtfs.guru';
 *
 *   const validator = new GtfsValidator();
 *   const result = await validator.validate(file, { countryCode: 'US' });
 *   console.log(result.isValid, result.errorCount);
 *   validator.terminate();
 */

/**
 * @typedef {Object} ValidationResult
 * @property {string} json - Full validation report as JSON string
 * @property {number} errorCount - Number of errors found
 * @property {number} warningCount - Number of warnings found
 * @property {number} infoCount - Number of info notices
 * @property {boolean} isValid - True if no errors were found
 * @property {number} validationTimeMs - Time taken for validation in milliseconds
 */

/**
 * @typedef {Object} ValidationOptions
 * @property {string} [countryCode] - ISO 3166-1 alpha-2 country code (e.g., "US", "DE")
 */

/**
 * GTFS Validator using Web Worker for non-blocking validation
 */
export class GtfsValidator {
  /**
   * Create a new validator instance
   * @param {string} [workerUrl] - Custom URL to the worker script
   */
  constructor(workerUrl) {
    this.worker = new Worker(
      workerUrl || new URL('./worker.js', import.meta.url),
      { type: 'module' }
    );
    this.pending = new Map();
    this.nextId = 0;
    this.ready = false;

    this.readyPromise = new Promise((resolve) => {
      const handler = (event) => {
        if (event.data.type === 'ready') {
          this.ready = true;
          this.worker.removeEventListener('message', handler);
          resolve();
        }
      };
      this.worker.addEventListener('message', handler);
    });

    this.worker.onmessage = (event) => {
      const { id, type, payload } = event.data;

      // Skip 'ready' messages in main handler
      if (type === 'ready') return;

      const handler = this.pending.get(id);
      if (handler) {
        this.pending.delete(id);
        if (type === 'error') {
          handler.reject(new Error(payload));
        } else {
          handler.resolve(payload);
        }
      }
    };

    this.worker.onerror = (error) => {
      // Reject all pending promises on worker error
      for (const [id, handler] of this.pending) {
        handler.reject(error);
        this.pending.delete(id);
      }
    };
  }

  /**
   * Wait for the validator to be ready
   * @returns {Promise<void>}
   */
  async waitUntilReady() {
    return this.readyPromise;
  }

  /**
   * Validate a GTFS ZIP file
   * @param {File|Blob|ArrayBuffer|Uint8Array} input - The GTFS ZIP file
   * @param {ValidationOptions} [options] - Validation options
   * @returns {Promise<ValidationResult>}
   */
  async validate(input, options = {}) {
    await this.readyPromise;

    let zipBytes;

    if (input instanceof Uint8Array) {
      zipBytes = input.buffer;
    } else if (input instanceof ArrayBuffer) {
      zipBytes = input;
    } else if (input instanceof Blob || input instanceof File) {
      zipBytes = await input.arrayBuffer();
    } else {
      throw new Error('Input must be a File, Blob, ArrayBuffer, or Uint8Array');
    }

    return this._send('validate', {
      zipBytes,
      countryCode: options.countryCode,
    });
  }

  /**
   * Get the validator version
   * @returns {Promise<string>}
   */
  async version() {
    await this.readyPromise;
    return this._send('version');
  }

  /**
   * Terminate the worker
   */
  terminate() {
    this.worker.terminate();
    // Reject any pending promises
    for (const [id, handler] of this.pending) {
      handler.reject(new Error('Validator terminated'));
      this.pending.delete(id);
    }
  }

  /**
   * Send a message to the worker and return a promise
   * @private
   */
  _send(type, payload) {
    return new Promise((resolve, reject) => {
      const id = this.nextId++;
      this.pending.set(id, { resolve, reject });
      this.worker.postMessage({ type, payload, id });
    });
  }
}

// Re-export direct WASM functions for synchronous usage
export { default as init, validate_gtfs, validate_gtfs_json, version } from './gtfs_validator_wasm.js';
