/**
 * GTFS Validator WASM - TypeScript Type Definitions
 */

/**
 * Severity level for validation notices
 */
export type NoticeSeverity = 'ERROR' | 'WARNING' | 'INFO';

/**
 * A validation notice (error, warning, or info)
 */
export interface ValidationNotice {
  /** Notice code (e.g., "missing_required_file", "invalid_color") */
  code: string;
  /** Severity level */
  severity: NoticeSeverity;
  /** Human-readable description */
  title: string;
  /** Name of the affected file (e.g., "stops.txt") */
  filename?: string;
  /** CSV row number (1-indexed) */
  csvRowNumber?: number;
  /** Affected field name */
  fieldName?: string;
  /** Additional context as key-value pairs */
  [key: string]: unknown;
}

/**
 * Validation result returned by the WASM module
 */
export interface ValidationResult {
  /** Full validation report as JSON string */
  readonly json: string;
  /** Number of errors found */
  readonly error_count: number;
  /** Number of warnings found */
  readonly warning_count: number;
  /** Number of info notices */
  readonly info_count: number;
  /** True if no errors were found (warnings/info don't affect validity) */
  readonly is_valid: boolean;
}

/**
 * Parsed validation result with typed notices
 */
export interface ParsedValidationResult {
  notices: ValidationNotice[];
  errorCount: number;
  warningCount: number;
  infoCount: number;
  isValid: boolean;
  validationTimeMs?: number;
}

/**
 * Options for validation
 */
export interface ValidationOptions {
  /** ISO 3166-1 alpha-2 country code (e.g., "US", "DE", "RU") */
  countryCode?: string;
}

/**
 * Initialize the WASM module. Must be called once before using other functions.
 */
export function init(): Promise<void>;

/**
 * Get the validator version string
 */
export function version(): string;

/**
 * Validate a GTFS ZIP file from raw bytes
 *
 * @param zipBytes - Raw bytes of the GTFS ZIP file
 * @param countryCode - Optional ISO 3166-1 alpha-2 country code
 * @returns ValidationResult with counts and JSON report
 *
 * @example
 * ```typescript
 * const bytes = new Uint8Array(await file.arrayBuffer());
 * const result = validate_gtfs(bytes, 'US');
 * console.log(result.is_valid, result.error_count);
 * const notices = JSON.parse(result.json) as ValidationNotice[];
 * ```
 */
export function validate_gtfs(
  zipBytes: Uint8Array,
  countryCode?: string | null
): ValidationResult;

/**
 * Validate a GTFS ZIP file and return only the JSON report
 *
 * @param zipBytes - Raw bytes of the GTFS ZIP file
 * @param countryCode - Optional ISO 3166-1 alpha-2 country code
 * @returns JSON string containing array of notices
 */
export function validate_gtfs_json(
  zipBytes: Uint8Array,
  countryCode?: string | null
): string;

/**
 * GTFS Validator using Web Worker for non-blocking validation
 *
 * @example
 * ```typescript
 * const validator = new GtfsValidator();
 * await validator.waitUntilReady();
 *
 * const result = await validator.validate(file, { countryCode: 'US' });
 * console.log(result.isValid, result.errorCount);
 *
 * validator.terminate();
 * ```
 */
export class GtfsValidator {
  /**
   * Create a new validator instance
   * @param workerUrl - Optional custom URL to the worker script
   */
  constructor(workerUrl?: string);

  /**
   * Wait for the validator to be ready
   */
  waitUntilReady(): Promise<void>;

  /**
   * Validate a GTFS ZIP file
   * @param input - The GTFS ZIP file (File, Blob, ArrayBuffer, or Uint8Array)
   * @param options - Validation options
   * @returns Validation result with counts and JSON report
   */
  validate(
    input: File | Blob | ArrayBuffer | Uint8Array,
    options?: ValidationOptions
  ): Promise<ParsedValidationResult>;

  /**
   * Get the validator version
   */
  version(): Promise<string>;

  /**
   * Terminate the worker and release resources
   */
  terminate(): void;
}
