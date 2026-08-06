/**
 * GTFS Validator WASM - TypeScript Type Definitions
 */

/**
 * Severity level for validation notices
 */
export type NoticeSeverity = 'ERROR' | 'WARNING' | 'INFO';

export interface NoticeGeometryPoint {
  latitude: number;
  longitude: number;
}

export type NoticeGeometry =
  | { type: 'point'; point: NoticeGeometryPoint }
  | { type: 'line'; points: NoticeGeometryPoint[] }
  | {
      type: 'pointAndLine';
      point: NoticeGeometryPoint;
      line: NoticeGeometryPoint[];
      nearestPoint?: NoticeGeometryPoint;
    }
  | {
      type: 'boundingBox';
      southWest: NoticeGeometryPoint;
      northEast: NoticeGeometryPoint;
    };

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
  /** Renderer-neutral geographic context for map-capable notices */
  geometry?: NoticeGeometry;
  /** Additional context as key-value pairs */
  [key: string]: unknown;
}

/**
 * Validation result returned by the WASM module
 */
export interface ValidationResult {
  /** Full validation report as JSON string */
  readonly json: string;
  /** Full validation report as HTML string */
  readonly html: string;
  /** Loading and per-validator timing breakdown as JSON string */
  readonly timings_json: string;
  /** Number of errors found */
  readonly error_count: number;
  /** Number of warnings found */
  readonly warning_count: number;
  /** Number of info notices */
  readonly info_count: number;
  /** True if no errors were found (warnings/info don't affect validity) */
  readonly is_valid: boolean;
  /** Move the JSON report out of WASM without a Rust-side clone. */
  take_json(): string;
  /** Move the HTML report out of WASM without a Rust-side clone. */
  take_html(): string;
  /** Move the timing breakdown out of WASM without a Rust-side clone. */
  take_timings_json(): string;
}

export interface TimingItem {
  name: string;
  duration_ms: number;
  duration_s: number;
}

export interface TimingCategory {
  total_ms: number;
  total_s: number;
  items: TimingItem[];
}

export type TimingBreakdown = Partial<Record<
  'loading' | 'parsing' | 'indexing' | 'validation',
  TimingCategory
>>;

/**
 * Parsed validation result with typed notices
 */
export interface ParsedValidationResult {
  json: string;
  html: string;
  errorCount: number;
  warningCount: number;
  infoCount: number;
  isValid: boolean;
  validationTimeMs: number;
  timings: TimingBreakdown;
  runtime: 'single-threaded' | 'multi-threaded';
}

/**
 * Options for validation
 */
export interface ValidationOptions {
  /** ISO 3166-1 alpha-2 country code (e.g., "US", "DE", "RU") */
  countryCode?: string;
  /** Validation date in YYYY-MM-DD format */
  date?: string;
}

export interface EntityDiff {
  added: string[];
  removed: string[];
  changed: string[];
}

export interface FeedDiff {
  files: { added: string[]; removed: string[] };
  feedInfo: {
    changed: boolean;
    oldVersion?: string;
    newVersion?: string;
    oldServiceRange?: string;
    newServiceRange?: string;
  };
  agencies: EntityDiff;
  routes: EntityDiff;
  stops: EntityDiff & {
    renamed: string[];
    moved: Array<{ stopId: string; distanceMeters: number }>;
  };
  tripsByRoute: Array<{ routeId: string; oldCount: number; newCount: number }>;
  frequenciesByRoute: Array<{
    routeId: string;
    oldWindows: string[];
    newWindows: string[];
  }>;
  notices: {
    newErrors: number;
    resolvedErrors: number;
    changes: Array<{
      code: string;
      severity: NoticeSeverity;
      oldCount: number;
      newCount: number;
    }>;
  };
}

export interface ParsedDiffResult {
  json: string;
  diff: FeedDiff;
  comparisonTimeMs: number;
  runtime: 'single-threaded' | 'multi-threaded';
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
  countryCode?: string | null,
  date?: string | null
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
  countryCode?: string | null,
  date?: string | null
): string;

/** Validate and compare a previous and new GTFS ZIP file. */
export function diff_gtfs(
  oldZipBytes: Uint8Array,
  newZipBytes: Uint8Array,
  countryCode?: string | null,
  date?: string | null
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

  /** Validate both feeds and return their semantic comparison. */
  diff(
    oldInput: File | Blob | ArrayBuffer | Uint8Array,
    newInput: File | Blob | ArrayBuffer | Uint8Array,
    options?: ValidationOptions
  ): Promise<ParsedDiffResult>;

  /**
   * Get the validator version
   */
  version(): Promise<string>;

  /**
   * Terminate the worker and release resources
   */
  terminate(): void;
}
