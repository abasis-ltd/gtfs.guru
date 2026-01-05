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
 *
 * # Returns
 * A ValidationResult containing the JSON report and summary counts
 */
export function validate_gtfs(zip_bytes: Uint8Array, country_code?: string | null): ValidationResult;

/**
 * Validate GTFS and return only the JSON report (simpler API)
 */
export function validate_gtfs_json(zip_bytes: Uint8Array, country_code?: string | null): string;

/**
 * Get the validator version
 */
export function version(): string;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
  readonly memory: WebAssembly.Memory;
  readonly __wbg_validationresult_free: (a: number, b: number) => void;
  readonly init: () => void;
  readonly validate_gtfs: (a: number, b: number, c: number, d: number) => number;
  readonly validate_gtfs_json: (a: number, b: number, c: number, d: number, e: number) => void;
  readonly validationresult_error_count: (a: number) => number;
  readonly validationresult_info_count: (a: number) => number;
  readonly validationresult_is_valid: (a: number) => number;
  readonly validationresult_json: (a: number, b: number) => void;
  readonly validationresult_warning_count: (a: number) => number;
  readonly version: (a: number) => void;
  readonly __wbindgen_export: (a: number, b: number, c: number) => void;
  readonly __wbindgen_export2: (a: number, b: number) => number;
  readonly __wbindgen_export3: (a: number, b: number, c: number, d: number) => number;
  readonly __wbindgen_add_to_stack_pointer: (a: number) => number;
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
