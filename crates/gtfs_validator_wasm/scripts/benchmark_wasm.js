const fs = require('fs');
const path = require('path');
const { validate_gtfs } = require('../pkg-node/gtfs_guru_wasm.js');

async function main() {
    const args = process.argv.slice(2);
    if (args.length < 1) {
        console.log("Usage: node benchmark_wasm.js <path_to_gtfs.zip>");
        process.exit(1);
    }

    const gtfsPath = args[0];
    console.log(`Benchmarking WASM validator on ${gtfsPath}...`);

    const zipBytes = fs.readFileSync(gtfsPath);

    const startTime = Date.now();
    const result = validate_gtfs(zipBytes);
    const totalTime = (Date.now() - startTime) / 1000;

    console.log(`Validation complete.`);
    console.log(`Total time (including overhead): ${totalTime.toFixed(4)}s`);
    console.log(`Errors: ${result.error_count}`);
    console.log(`Warnings: ${result.warning_count}`);
}

main().catch(console.error);
