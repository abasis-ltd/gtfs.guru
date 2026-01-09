# Scripts

Utility scripts for building, testing, and deploying the GTFS Validator.

## Build & Deploy

| Script | Description |
|--------|-------------|
| `bootstrap.sh` | Creates the full project structure from scratch. Useful for onboarding new developers. |
| `build-installers.sh` | Builds platform-specific installers (DMG, MSI, DEB) using Tauri. |
| `build-wasm.sh` | Builds WASM packages for web and Node.js targets with wasm-pack. |
| `deploy-to-hetzner.sh` | Deploys the validator to a Hetzner server via SSH + Docker Compose. |
| `generate_all_icons.sh` | Generates all app icons (macOS, Windows, iOS, Android) from a source PNG. |
| `check.sh` | Pre-commit checks: cargo fmt, clippy, and tests. |

## Benchmarking

| Script | Description |
|--------|-------------|
| `benchmark_compare.py` | Compares Rust vs Java validator performance with timing statistics. |
| `run_benchmark_comparison.py` | Runs benchmark comparisons across multiple GTFS feeds. |
| `run_google_tests.py` | Runs the validator against Google's transitfeed test suite. |

## Golden Testing

Golden tests compare validator output against expected reference files.

| Script | Description |
|--------|-------------|
| `golden.py` | Main entry point for golden test workflows (suite/single/validate/update). |
| `run_golden_suite.py` | Runs all golden tests from a TSV manifest. |
| `run_golden_compare.py` | Compares a single feed against expected output. |
| `run_golden_suite.sh` | Shell wrapper for `run_golden_suite.py`. |
| `run_golden_compare.sh` | Shell wrapper for `run_golden_compare.py`. |
| `golden.sh` | Shell wrapper for `golden.py`. |
| `golden_single.py` | Runs a single golden test case. |
| `golden_single.sh` | Shell wrapper for `golden_single.py`. |
| `ci_golden.sh` | CI-optimized golden test runner. |
| `validate_golden_manifest.py` | Validates the golden test manifest for correctness. |
| `update_expected_from_manifest.py` | Updates expected outputs from actual validator results. |
| `compare_reports.py` | Compares two validator output directories (JSON + HTML). |

### Golden Test Configuration Files

| File | Description |
|------|-------------|
| `golden.env.example` | Example environment variables for golden tests. |
| `golden_manifest.example.tsv` | Example TSV manifest format for golden tests. |
| `expected_layout.example.txt` | Example expected output directory structure. |

## Usage Examples

```bash
# Run pre-commit checks
./scripts/check.sh

# Build WASM package
./scripts/build-wasm.sh

# Build desktop installers
./scripts/build-installers.sh

# Run benchmarks comparing Rust vs Java
python3 scripts/benchmark_compare.py benchmark-feeds/nl.zip --iterations 3

# Run golden test suite
python3 scripts/golden.py suite golden_manifest.tsv tmp/actual

# Deploy to production server
./scripts/deploy-to-hetzner.sh validator.example.com
```
