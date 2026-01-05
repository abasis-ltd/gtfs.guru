import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';
import { initSync, validate_gtfs_json, version } from './crates/gtfs_validator_wasm/pkg/gtfs_guru_wasm.js';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

async function main() {
    const wasmPath = path.join(__dirname, 'crates/gtfs_validator_wasm/pkg/gtfs_guru_wasm_bg.wasm');
    const wasmBuffer = fs.readFileSync(wasmPath);

    initSync({ module: wasmBuffer });

    // Import version function dynamically or assume it's available in pkg exports if I update the import
    // But currently I prefer to just use the one exported.
    // Wait, I need to import it in the file top level.
    console.log('WASM Version:', version());

    const zipPath = path.join(__dirname, 'tmp/gtfs_export (6).zip');
    const zipBuffer = fs.readFileSync(zipPath);
    const zipUint8Array = new Uint8Array(zipBuffer);

    console.log('Validating with WASM...');
    const resultJson = validate_gtfs_json(zipUint8Array, "ZZ");
    const result = JSON.parse(resultJson);

    fs.writeFileSync(path.join(__dirname, 'tmp/wasm_report.json'), JSON.stringify(result, null, 2));

    const summary = {};
    result.forEach(notice => {
        summary[notice.code] = (summary[notice.code] || 0) + 1;
    });

    console.log('WASM Validation complete. Notices summary:');
    console.log(JSON.stringify(summary, null, 2));
}

main().catch(console.error);
