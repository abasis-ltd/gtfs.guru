const fs = require('fs');
const path = require('path');
const { performance } = require('perf_hooks');

// Parse arguments manually since we don't want a heavy dependency like yargs for this simple script
const args = process.argv.slice(2);
let inputPath = null;
let outputPath = null;

for (let i = 0; i < args.length; i++) {
    if (args[i] === '--input') {
        inputPath = args[i + 1];
        i++;
    } else if (args[i] === '--output') {
        outputPath = args[i + 1];
        i++;
    }
}

if (!inputPath || !outputPath) {
    console.error("Usage: node run_wasm_single.js --input <zip> --output <dir>");
    process.exit(1);
}

const wasmPath = path.resolve(__dirname, '../crates/gtfs_validator_wasm/pkg-node/gtfs_guru_wasm.js');

try {
    const wasm = require(wasmPath);

    console.log(`Reading input file: ${inputPath}...`);
    const data = fs.readFileSync(inputPath);

    console.log("Running WASM validation...");
    const startTime = performance.now();

    // Validate with current date to match other validators if possible, 
    // or use a fixed date if that's what we want for reproducible tests.
    // For now, using a fixed date to ensure consistency or let's try to match CLI.
    // CLI uses current date by default. 
    const dateStr = new Date().toISOString().slice(0, 10);

    const result = wasm.validate_gtfs(new Uint8Array(data), null, dateStr);
    const endTime = performance.now();

    console.log("Generating JSON report...");
    // result.json is a property that returns the JSON string
    const jsonReport = result.json;

    if (!fs.existsSync(outputPath)) {
        fs.mkdirSync(outputPath, { recursive: true });
    }

    const reportFile = path.join(outputPath, 'report.json');
    fs.writeFileSync(reportFile, jsonReport);

    console.log(`WASM report written to ${reportFile}`);
    console.log(`Duration: ${((endTime - startTime) / 1000).toFixed(2)}s`);

} catch (e) {
    console.error("Error during WASM validation:", e);
    process.exit(1);
}
