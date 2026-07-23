/* tslint:disable */
/* eslint-disable */

/**
 * Validation result returned to JavaScript
 */
export class ValidationResult {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    /**
     * Get the number of errors
     */
    readonly error_count: number;
    /**
     * Get the full validation report as HTML
     */
    readonly html: string;
    /**
     * Get the number of info notices
     */
    readonly info_count: number;
    /**
     * Check if validation passed (no errors)
     */
    readonly is_valid: boolean;
    /**
     * Get the full validation report as JSON
     */
    readonly json: string;
    /**
     * Get the number of warnings
     */
    readonly warning_count: number;
}

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
 * Throws a JavaScript error if the file exceeds 100 MB
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
    build(): void;
    numThreads(): number;
    receiver(): number;
}

export function wbg_rayon_start_worker(receiver: number): void;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
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
    readonly __wbg_wbg_rayon_poolbuilder_free: (a: number, b: number) => void;
    readonly initThreadPool: (a: number) => any;
    readonly wbg_rayon_poolbuilder_build: (a: number) => void;
    readonly wbg_rayon_poolbuilder_numThreads: (a: number) => number;
    readonly wbg_rayon_poolbuilder_receiver: (a: number) => number;
    readonly wbg_rayon_start_worker: (a: number) => void;
    readonly memory: WebAssembly.Memory;
    readonly __wbindgen_free: (a: number, b: number, c: number) => void;
    readonly __wbindgen_malloc: (a: number, b: number) => number;
    readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
    readonly __externref_table_alloc: () => number;
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __externref_table_dealloc: (a: number) => void;
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
