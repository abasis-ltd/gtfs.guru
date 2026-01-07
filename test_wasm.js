const fs = require('fs');
const path = require('path');
const { validate_gtfs_json } = require('./crates/gtfs_validator_wasm/pkg/gtfs_guru_wasm.js');

async function main() {
    const zipPath = path.join(__dirname, 'test-gtfs-feeds/base-valid.zip');
    const zipBuffer = fs.readFileSync(zipPath);
    const zipUint8Array = new Uint8Array(zipBuffer);

    console.log('Validating with WASM...');
    const resultJson = validate_gtfs_json(zipUint8Array, "");

    // Write to file
    fs.writeFileSync('output_wasm.json', resultJson);
    console.log('Wrote output to output_wasm.json');
}

main().catch(console.error);
