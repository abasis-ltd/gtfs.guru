# WebAssembly (WASM) Usage Guide

The GTFS Validator can run entirely in the browser using WebAssembly. No server required - all validation happens locally in the user's browser.

## Installation

### npm / yarn / pnpm

```bash
npm install gtfs.guru
# or
yarn add gtfs.guru
# or
pnpm add gtfs.guru
```

### CDN

```html
<script type="module">
  import init, { validate_gtfs } from 'https://unpkg.com/gtfs.guru/gtfs_validator_wasm.js';
</script>
```

Or use jsDelivr:

```html
<script type="module">
  import init, { validate_gtfs } from 'https://cdn.jsdelivr.net/npm/gtfs.guru/gtfs_validator_wasm.js';
</script>
```

## Quick Start

### Browser (ES Modules)

```javascript
import init, { validate_gtfs, version } from 'gtfs.guru';

async function main() {
  // Initialize WASM module (required once)
  await init();

  console.log('Validator version:', version());

  // Get file from input
  const input = document.getElementById('gtfs-file');
  const file = input.files[0];
  const bytes = new Uint8Array(await file.arrayBuffer());

  // Validate
  const result = validate_gtfs(bytes, 'US');

  console.log('Valid:', result.is_valid);
  console.log('Errors:', result.error_count);
  console.log('Warnings:', result.warning_count);

  // Parse detailed notices
  const notices = JSON.parse(result.json);
  notices.forEach(notice => {
    console.log(`[${notice.severity}] ${notice.code}: ${notice.title}`);
  });
}

main();
```

### Node.js

```javascript
const fs = require('fs');
const { init, validate_gtfs } = require('gtfs.guru');

async function main() {
  await init();

  const bytes = fs.readFileSync('gtfs.zip');
  const result = validate_gtfs(new Uint8Array(bytes));

  console.log('Valid:', result.is_valid);
  console.log(JSON.parse(result.json));
}

main();
```

### Using Web Worker (Recommended for UI)

For better user experience, use the Web Worker to avoid blocking the main thread:

```javascript
import { GtfsValidator } from 'gtfs.guru';

const validator = new GtfsValidator();

// Wait for initialization
await validator.waitUntilReady();

// Validate a file (non-blocking)
const result = await validator.validate(file, { countryCode: 'US' });

console.log('Valid:', result.isValid);
console.log('Errors:', result.errorCount);
console.log('Time:', result.validationTimeMs, 'ms');

// Clean up when done
validator.terminate();
```

## API Reference

### `init(): Promise<void>`

Initialize the WASM module. Must be called once before using other functions.

### `version(): string`

Returns the validator version string.

### `validate_gtfs(bytes, countryCode?): ValidationResult`

Validates a GTFS ZIP file.

**Parameters:**

- `bytes: Uint8Array` - Raw bytes of the GTFS ZIP file
- `countryCode?: string` - Optional ISO 3166-1 alpha-2 country code (e.g., "US", "DE", "RU")

**Returns:** `ValidationResult` object with:

- `json: string` - Full validation report as JSON
- `error_count: number` - Number of errors
- `warning_count: number` - Number of warnings
- `info_count: number` - Number of info notices
- `is_valid: boolean` - True if no errors
- `timings_json: string` - Aggregate loading, per-table parsing, indexing, and
  per-validator timings as JSON

### `validate_gtfs_json(bytes, countryCode?): string`

Same as `validate_gtfs` but returns only the JSON string.

### `GtfsValidator` Class

Web Worker wrapper for non-blocking validation:

```typescript
class GtfsValidator {
  constructor(workerUrl?: string);
  waitUntilReady(): Promise<void>;
  validate(input: File | Blob | ArrayBuffer | Uint8Array, options?: { countryCode?: string }): Promise<ValidationResult>;
  version(): Promise<string>;
  terminate(): void;
}
```

## Bundler Configuration

### Webpack 5

```javascript
// webpack.config.js
module.exports = {
  experiments: {
    asyncWebAssembly: true,
  },
};
```

### Vite

```javascript
// vite.config.js
import { defineConfig } from 'vite';

export default defineConfig({
  optimizeDeps: {
    exclude: ['gtfs.guru']
  }
});
```

### Next.js

```javascript
// next.config.js
module.exports = {
  webpack: (config) => {
    config.experiments = {
      ...config.experiments,
      asyncWebAssembly: true,
    };
    return config;
  }
};
```

### Rollup

```javascript
// rollup.config.js
import { wasm } from '@rollup/plugin-wasm';

export default {
  plugins: [wasm()]
};
```

## TypeScript Support

TypeScript definitions are included. Import types like this:

```typescript
import type { ValidationResult, ValidationNotice, NoticeSeverity } from 'gtfs.guru';
```

## Memory Considerations

WASM runs in a limited memory environment. For large GTFS feeds:

1. **File Size Limits**: Browser validation accepts up to 100 MB compressed and
   512 MB total uncompressed data. Both limits are checked before parsing.

2. **Web Worker**: Always use the Web Worker for files over 10MB to avoid UI freezing.

3. **Server-Side**: For very large feeds (>200MB), consider server-side validation with the CLI or REST API.

The uncompressed limit is essential because ZIP size alone is misleading. For
example, the 75 MB `ch.zip` benchmark expands to 701 MB and exhausted the WASM
heap before the central-directory guard was added.

CSV members are streamed from the ZIP directly into the sequential parser, so
the browser does not retain a second full uncompressed copy of large tables.
Validation notices are bounded to 1,000 retained samples per code and severity;
`totalNotices` and the summary severity counts remain exact.

## Threads benchmark

On Apple M3 Pro / Chrome 150, three runs of `paris.zip` (10 MB compressed,
89 MB uncompressed) averaged 2.242 s single-threaded and 1.818 s with parallel
validators, a 1.23x speedup. Reports matched exactly by SHA-256. Parallel CSV
deserialization was disabled for WASM after it exceeded a five-minute timeout
on the same feed due to shared interner contention; native builds retain it.

## Browser Compatibility

| Browser | Minimum Version |
|---------|----------------|
| Chrome  | 57+ |
| Firefox | 52+ |
| Safari  | 11+ |
| Edge    | 16+ |

## Error Handling

```javascript
try {
  await init();
  const result = validate_gtfs(bytes);
  // ...
} catch (error) {
  if (error instanceof WebAssembly.RuntimeError) {
    console.error('WASM runtime error:', error.message);
  } else if (error.message.includes('memory')) {
    console.error('Out of memory - file too large');
  } else {
    console.error('Validation error:', error);
  }
}
```

## Building from Source

```bash
# Install wasm-pack
curl https://rustwasm.github.io/wasm-pack/installer/init.sh -sSf | sh

# Build for web
wasm-pack build crates/gtfs_validator_wasm --target web --release

# Build for browsers with WebAssembly threads (nightly + rust-src required)
RUSTFLAGS='-C target-feature=+atomics,+bulk-memory,+simd128 -C link-arg=--shared-memory -C link-arg=--max-memory=4294967296 -C link-arg=--import-memory -C link-arg=--export=__wasm_init_tls -C link-arg=--export=__tls_size -C link-arg=--export=__tls_align -C link-arg=--export=__tls_base' \
  rustup run nightly-2025-11-15 wasm-pack build crates/gtfs_validator_wasm --target web \
  --release --out-dir pkg-mt -- \
  --features threads -Z build-std=panic_abort,std

# Build for Node.js
wasm-pack build crates/gtfs_validator_wasm --target nodejs --release --out-dir pkg-node

# Or use the build script
./scripts/build-wasm.sh
```

The threaded package is selected automatically when the page is cross-origin
isolated (`COOP: same-origin` and `COEP: require-corp`) and
`SharedArrayBuffer` is available. Otherwise the worker uses the portable
single-threaded package. GitHub Pages therefore remains on the fallback; use the
included Caddy, Nginx, or Cloudflare Pages headers to enable threads.

## Demo

See the interactive demo at `crates/gtfs_validator_wasm/demo/index.html` or try it online (after npm publish).
