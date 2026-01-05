const fs = require('fs');
const path = require('path');
const { initSync, validate_gtfs_json } = require('./crates/gtfs_validator_wasm/pkg/gtfs_validator_wasm.js');

async function main() {
    const wasmPath = path.join(__dirname, 'crates/gtfs_validator_wasm/pkg/gtfs_validator_wasm_bg.wasm');
    const wasmBuffer = fs.readFileSync(wasmPath);

    initSync(wasmBuffer);

    const zipPath = path.join(__dirname, 'tmp/gtfs_limassol_shuttle.zip');
    const zipBuffer = fs.readFileSync(zipPath);
    const zipUint8Array = new Uint8Array(zipBuffer);

    console.log('Validating with WASM...');
    const resultJson = validate_gtfs_json(zipUint8Array, "ZZ");
    const result = JSON.parse(resultJson);

    console.log(JSON.stringify(result, null, 2));
}

main().catch(console.error);
