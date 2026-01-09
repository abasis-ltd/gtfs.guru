/* tslint:disable */
/* eslint-disable */

export class ValidationResult {
  private constructor();
  free(): void;
  [Symbol.dispose](): void;
  /**
   * Get the number of info notices
   */
  readonly info_count: number;
  /**
   * Get the number of errors
   */
  readonly error_count: number;
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
}

/**
 * Initialize the WASM module (call once on page load)
 */
export function init(): void;

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
 * Throws a JavaScript error if the file exceeds 50 MB
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

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
  readonly memory: WebAssembly.Memory;
  readonly __wbg_validationresult_free: (a: number, b: number) => void;
  readonly validate_gtfs: (a: number, b: number, c: number, d: number, e: number, f: number) => [number, number, number];
  readonly validate_gtfs_json: (a: number, b: number, c: number, d: number, e: number, f: number) => [number, number, number, number];
  readonly validationresult_error_count: (a: number) => number;
  readonly validationresult_html: (a: number) => [number, number];
  readonly validationresult_info_count: (a: number) => number;
  readonly validationresult_is_valid: (a: number) => number;
  readonly validationresult_json: (a: number) => [number, number];
  readonly validationresult_warning_count: (a: number) => number;
  readonly version: () => [number, number];
  readonly init: () => void;
  readonly __wbindgen_free: (a: number, b: number, c: number) => void;
  readonly __wbindgen_malloc: (a: number, b: number) => number;
  readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
  readonly __wbindgen_externrefs: WebAssembly.Table;
  readonly __externref_table_dealloc: (a: number) => void;
  readonly __wbindgen_start: () => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
* Instantiates the given `module`, which can either be bytes or
* a precompiled `WebAssembly.Module`.
*
* @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
*
* @returns {InitOutput}
*/
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
* If `module_or_path` is {RequestInfo} or {URL}, makes a request and
* for everything else, calls `WebAssembly.instantiate` directly.
*
* @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
*
* @returns {Promise<InitOutput>}
*/
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
