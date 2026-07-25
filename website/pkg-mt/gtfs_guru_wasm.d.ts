/* tslint:disable */
/* eslint-disable */

export class ValidationResult {
  private constructor();
  free(): void;
  [Symbol.dispose](): void;
  /**
   * Move the timing report into JavaScript without cloning it in Rust.
   */
  take_timings_json(): string;
  /**
   * Move the HTML report into JavaScript without cloning it in Rust.
   */
  take_html(): string;
  /**
   * Move the JSON report into JavaScript without cloning it in Rust.
   */
  take_json(): string;
  /**
   * Get the number of info notices
   */
  readonly info_count: number;
  /**
   * Get the number of errors
   */
  readonly error_count: number;
  /**
   * Get the loading and per-validator timing breakdown as JSON.
   */
  readonly timings_json: string;
  /**
   * Get the number of warnings
   */
  readonly warning_count: number;
  /**
   * Get the full validation report as HTML
   */
  readonly html: string;
  /**
   * Get the full validation report as JSON
   */
  readonly json: string;
  /**
   * Check if validation passed (no errors)
   */
  readonly is_valid: boolean;
  /**
   * True when the notice list in `json` was capped per issue type to keep
   * memory bounded. Counts (`error_count` etc.) are always exact.
   */
  readonly truncated: boolean;
}

/**
 * Validate and compare two GTFS ZIP files, returning a semantic diff as JSON.
 */
export function diff_gtfs(old_zip_bytes: Uint8Array, new_zip_bytes: Uint8Array, country_code?: string | null, date?: string | null): string;

/**
 * Initialize the WASM module (call once on page load)
 */
export function init(): void;

export function initThreadPool(num_threads: number): Promise<any>;

/**
 * Validate a GTFS ZIP file from bytes
 *
 * # Arguments
 * * `zip_bytes` - The raw bytes of a GTFS ZIP file
 * * `country_code` - Optional ISO 3166-1 alpha-2 country code for country-specific validation
 * * `date` - Optional validation date in YYYY-MM-DD format
 *
 * # Returns
 * A ValidationResult containing the JSON report and summary counts
 *
 * # Errors
 * Throws a JavaScript error if the feed exceeds the browser size limits
 * (150 MB zipped / 700 MB uncompressed)
 */
export function validate_gtfs(zip_bytes: Uint8Array, country_code?: string | null, date?: string | null): ValidationResult;

/**
 * Validate GTFS and return only the JSON report (simpler API)
 */
export function validate_gtfs_json(zip_bytes: Uint8Array, country_code?: string | null, date?: string | null): string;

/**
 * Get the validator version
 */
export function version(): string;

export class wbg_rayon_PoolBuilder {
  private constructor();
  free(): void;
  [Symbol.dispose](): void;
  numThreads(): number;
  build(): void;
  mainJS(): string;
  receiver(): number;
}

export function wbg_rayon_start_worker(receiver: number): void;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
  readonly __wbg_validationresult_free: (a: number, b: number) => void;
  readonly diff_gtfs: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number) => void;
  readonly validate_gtfs: (a: number, b: number, c: number, d: number, e: number, f: number, g: number) => void;
  readonly validate_gtfs_json: (a: number, b: number, c: number, d: number, e: number, f: number, g: number) => void;
  readonly validationresult_error_count: (a: number) => number;
  readonly validationresult_html: (a: number, b: number) => void;
  readonly validationresult_info_count: (a: number) => number;
  readonly validationresult_is_valid: (a: number) => number;
  readonly validationresult_json: (a: number, b: number) => void;
  readonly validationresult_take_html: (a: number, b: number) => void;
  readonly validationresult_take_json: (a: number, b: number) => void;
  readonly validationresult_take_timings_json: (a: number, b: number) => void;
  readonly validationresult_timings_json: (a: number, b: number) => void;
  readonly validationresult_truncated: (a: number) => number;
  readonly validationresult_warning_count: (a: number) => number;
  readonly version: (a: number) => void;
  readonly init: () => void;
  readonly __wbg_wbg_rayon_poolbuilder_free: (a: number, b: number) => void;
  readonly initThreadPool: (a: number) => number;
  readonly wbg_rayon_poolbuilder_build: (a: number) => void;
  readonly wbg_rayon_poolbuilder_mainJS: (a: number) => number;
  readonly wbg_rayon_poolbuilder_numThreads: (a: number) => number;
  readonly wbg_rayon_poolbuilder_receiver: (a: number) => number;
  readonly wbg_rayon_start_worker: (a: number) => void;
  readonly memory: WebAssembly.Memory;
  readonly __wbindgen_export: (a: number) => void;
  readonly __wbindgen_export2: (a: number, b: number, c: number) => void;
  readonly __wbindgen_export3: (a: number, b: number) => number;
  readonly __wbindgen_export4: (a: number, b: number, c: number, d: number) => number;
  readonly __wbindgen_add_to_stack_pointer: (a: number) => number;
  readonly __wbindgen_thread_destroy: (a?: number, b?: number, c?: number) => void;
  readonly __wbindgen_start: (a: number) => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
* Instantiates the given `module`, which can either be bytes or
* a precompiled `WebAssembly.Module`.
*
* @param {{ module: SyncInitInput, memory?: WebAssembly.Memory, thread_stack_size?: number }} module - Passing `SyncInitInput` directly is deprecated.
* @param {WebAssembly.Memory} memory - Deprecated.
*
* @returns {InitOutput}
*/
export function initSync(module: { module: SyncInitInput, memory?: WebAssembly.Memory, thread_stack_size?: number } | SyncInitInput, memory?: WebAssembly.Memory): InitOutput;

/**
* If `module_or_path` is {RequestInfo} or {URL}, makes a request and
* for everything else, calls `WebAssembly.instantiate` directly.
*
* @param {{ module_or_path: InitInput | Promise<InitInput>, memory?: WebAssembly.Memory, thread_stack_size?: number }} module_or_path - Passing `InitInput` directly is deprecated.
* @param {WebAssembly.Memory} memory - Deprecated.
*
* @returns {Promise<InitOutput>}
*/
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput>, memory?: WebAssembly.Memory, thread_stack_size?: number } | InitInput | Promise<InitInput>, memory?: WebAssembly.Memory): Promise<InitOutput>;
