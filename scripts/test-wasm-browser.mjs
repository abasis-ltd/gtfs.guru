#!/usr/bin/env node

import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { readFile } from 'node:fs/promises';
import { extname, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import { chromium } from 'playwright';

const projectRoot = resolve(fileURLToPath(new URL('..', import.meta.url)));
const websiteRoot = resolve(projectRoot, 'website');
const fixturePath = resolve(projectRoot, 'test-gtfs-feeds/base-valid.zip');
// Keep in sync with MAX_FILE_SIZE_BYTES in crates/gtfs_validator_wasm/src/lib.rs.
const maxFileSizeMb = 150;

const contentTypes = {
  '.css': 'text/css; charset=utf-8',
  '.html': 'text/html; charset=utf-8',
  '.js': 'text/javascript; charset=utf-8',
  '.json': 'application/json; charset=utf-8',
  '.png': 'image/png',
  '.wasm': 'application/wasm',
  '.zip': 'application/zip',
};

function startServer({ isolated }) {
  const server = createServer(async (request, response) => {
    try {
      const pathname = decodeURIComponent(new URL(request.url, 'http://localhost').pathname);
      let filePath;
      if (pathname === '/fixture.zip') {
        filePath = fixturePath;
      } else {
        const relativePath = pathname === '/' ? 'index.html' : pathname.replace(/^\/+/, '');
        filePath = resolve(websiteRoot, relativePath);
        if (filePath !== websiteRoot && !filePath.startsWith(`${websiteRoot}${sep}`)) {
          response.writeHead(403).end('Forbidden');
          return;
        }
      }

      if (isolated) {
        response.setHeader('Cross-Origin-Opener-Policy', 'same-origin');
        response.setHeader('Cross-Origin-Embedder-Policy', 'require-corp');
      }
      response.setHeader('Cache-Control', 'no-store');
      response.setHeader('Content-Type', contentTypes[extname(filePath)] || 'application/octet-stream');
      response.writeHead(200).end(await readFile(filePath));
    } catch (error) {
      response.writeHead(error?.code === 'ENOENT' ? 404 : 500).end(String(error));
    }
  });

  return new Promise((resolveStarted, reject) => {
    server.once('error', reject);
    server.listen(0, '127.0.0.1', () => {
      const address = server.address();
      resolveStarted({ server, url: `http://127.0.0.1:${address.port}` });
    });
  });
}

async function closeServer(server) {
  await new Promise((resolveClosed, reject) => {
    server.close((error) => (error ? reject(error) : resolveClosed()));
  });
}

async function validate(page, forceSingleThreaded = false) {
  return page.evaluate(async ({ forceSingleThreaded }) => {
    const zipBytes = await fetch('/fixture.zip').then((response) => response.arrayBuffer());
    const workerUrl = `/pkg/worker.js${forceSingleThreaded ? '?threads=off' : ''}`;

    return new Promise((resolveResult, reject) => {
      const worker = new Worker(workerUrl, { type: 'module' });
      const timeout = setTimeout(() => {
        worker.terminate();
        reject(new Error(`Timed out waiting for ${workerUrl}`));
      }, 120_000);

      worker.onerror = (event) => {
        clearTimeout(timeout);
        worker.terminate();
        reject(new Error(event.message || `Worker failed: ${workerUrl}`));
      };
      worker.onmessage = ({ data }) => {
        if (data.type === 'ready') {
          worker.postMessage({
            id: 1,
            type: 'validate',
            payload: {
              zipBytes,
              date: '2026-07-22',
              includeHtml: false,
            },
          }, [zipBytes]);
          return;
        }
        if (data.type === 'error') {
          clearTimeout(timeout);
          worker.terminate();
          reject(new Error(data.payload));
          return;
        }
        if (data.type === 'result') {
          clearTimeout(timeout);
          worker.terminate();
          const notices = JSON.parse(data.payload.json);
          const noticeCounts = notices.reduce((counts, notice) => {
            const key = `${notice.code}:${notice.severity}`;
            counts[key] = (counts[key] || 0) + (notice.totalNotices || 1);
            return counts;
          }, {});
          resolveResult({
            runtime: data.payload.runtime,
            errorCount: data.payload.errorCount,
            warningCount: data.payload.warningCount,
            infoCount: data.payload.infoCount,
            noticeCounts,
          });
        }
      };
    });
  }, { forceSingleThreaded });
}

async function validateOversizedArchive(page, forceSingleThreaded = false) {
  return page.evaluate(async ({ forceSingleThreaded, maxFileSizeMb }) => {
    const zipBytes = new ArrayBuffer(maxFileSizeMb * 1024 * 1024 + 1);
    const workerUrl = `/pkg/worker.js${forceSingleThreaded ? '?threads=off' : ''}`;

    return new Promise((resolveResult, reject) => {
      const worker = new Worker(workerUrl, { type: 'module' });
      const timeout = setTimeout(() => {
        worker.terminate();
        reject(new Error(`Timed out waiting for ${workerUrl}`));
      }, 120_000);

      const finish = (callback, value) => {
        clearTimeout(timeout);
        worker.terminate();
        callback(value);
      };

      worker.onerror = (event) => {
        finish(reject, new Error(event.message || `Worker failed: ${workerUrl}`));
      };
      worker.onmessage = ({ data }) => {
        if (data.type === 'ready') {
          worker.postMessage({
            id: 1,
            type: 'validate',
            payload: {
              zipBytes,
              date: '2026-07-22',
              includeHtml: false,
            },
          }, [zipBytes]);
          return;
        }
        if (data.type === 'error') {
          finish(resolveResult, data.payload);
          return;
        }
        if (data.type === 'result') {
          finish(reject, new Error(`${workerUrl} unexpectedly accepted an oversized archive`));
        }
      };
    });
  }, { forceSingleThreaded, maxFileSizeMb });
}

const isolated = await startServer({ isolated: true });
const portable = await startServer({ isolated: false });
const browser = await chromium.launch({ headless: true });

try {
  const page = await browser.newPage();

  await page.goto(isolated.url);
  assert.equal(await page.evaluate(() => globalThis.crossOriginIsolated), true);
  const threaded = await validate(page);
  const forcedSingle = await validate(page, true);
  assert.equal(threaded.runtime, 'multi-threaded');
  assert.equal(forcedSingle.runtime, 'single-threaded');
  assert.deepEqual(threaded.noticeCounts, forcedSingle.noticeCounts);
  assert.deepEqual(
    [threaded.errorCount, threaded.warningCount, threaded.infoCount],
    [forcedSingle.errorCount, forcedSingle.warningCount, forcedSingle.infoCount],
  );
  assert.match(
    await validateOversizedArchive(page),
    new RegExp(`Maximum size for browser validation is ${maxFileSizeMb} MB`),
  );
  assert.match(
    await validateOversizedArchive(page, true),
    new RegExp(`Maximum size for browser validation is ${maxFileSizeMb} MB`),
  );

  await page.goto(portable.url);
  assert.equal(await page.evaluate(() => globalThis.crossOriginIsolated), false);
  const fallback = await validate(page);
  assert.equal(fallback.runtime, 'single-threaded');
  assert.deepEqual(fallback.noticeCounts, forcedSingle.noticeCounts);

  console.log(JSON.stringify({ threaded, forcedSingle, fallback }, null, 2));
} finally {
  await browser.close();
  await Promise.all([closeServer(isolated.server), closeServer(portable.server)]);
}
