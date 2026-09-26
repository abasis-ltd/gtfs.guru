import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import vm from 'node:vm';
import { test } from 'node:test';

const source = readFileSync(new URL('../crates/gtfs_validator_wasm/js/worker-mt.js', import.meta.url), 'utf8')
  .replace(/import init, \{[\s\S]*?\} from '\.\/gtfs_guru_wasm\.js';/, '');

for (const failure of [null, 'getter', 'postMessage']) {
  test(`releases each WASM result when ${failure ?? 'validation succeeds'}`, async () => {
    let freed = 0;
    let results = 0;
    const self = { navigator: { hardwareConcurrency: 2 }, postMessage(message) {
      if (message.type === 'result') {
        results++;
        if (failure === 'postMessage') throw new Error('clone failed');
      }
    } };
    vm.runInNewContext(source, {
      self, performance, Uint8Array,
      init: async () => {}, initThreadPool: async () => {},
      validate_gtfs: () => ({
        get json() { if (failure === 'getter') throw new Error('getter failed'); return '{}'; },
        html: '', free() { freed++; },
      }),
    });
    for (let id = 0; id < 3; id++) {
      await self.onmessage({ data: { id, type: 'validate', payload: { zipBytes: new ArrayBuffer(0) } } });
      assert.equal(freed, id + 1);
    }
    assert.equal(results, failure === 'getter' ? 0 : 3);
  });
}
