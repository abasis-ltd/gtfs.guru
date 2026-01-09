const fs = require('fs');
try {
    const wasm = require('./crates/gtfs_validator_wasm/pkg-node/gtfs_guru_wasm.js');

    console.log("Reading input file...");
    const data = fs.readFileSync('benchmark-feeds/alexandria.zip');

    console.log("Running WASM validation...");
    // Pass Uint8Array, country_code (null), date ("2026-01-09")
    const result = wasm.validate_gtfs(new Uint8Array(data), null, "2026-01-09");

    console.log("Generating JSON report...");
    const jsonReport = result.json; // getter property, not function

    fs.writeFileSync('wasm_report.json', jsonReport);
    console.log("WASM report written to wasm_report.json");
} catch (e) {
    console.error("Error:", e);
    process.exit(1);
}
