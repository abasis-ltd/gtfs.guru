#!/usr/bin/env node

import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { readFile } from 'node:fs/promises';
import { extname, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import { gzipSync } from 'node:zlib';
import { chromium } from 'playwright';

const projectRoot = resolve(fileURLToPath(new URL('..', import.meta.url)));
const websiteRoot = resolve(projectRoot, 'website');
const fixturePath = resolve(projectRoot, 'test-gtfs-feeds/base-valid.zip');
// Keep in sync with MAX_FILE_SIZE_BYTES in crates/gtfs_validator_wasm/src/lib.rs.
const maxFileSizeMb = 150;
// Keep in sync with SHARE_MAX_DECODED_BYTES in website/script.js.
const maxSharedReportDecodedBytes = 24 * 1024 * 1024;

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

async function compare(page, forceSingleThreaded = false) {
  return page.evaluate(async ({ forceSingleThreaded }) => {
    const [oldZipBytes, newZipBytes] = await Promise.all([
      fetch('/fixture.zip').then((response) => response.arrayBuffer()),
      fetch('/fixture.zip').then((response) => response.arrayBuffer()),
    ]);
    const workerUrl = `/pkg/worker.js${forceSingleThreaded ? '?threads=off' : ''}`;

    return new Promise((resolveResult, reject) => {
      const worker = new Worker(workerUrl, { type: 'module' });
      const timeout = setTimeout(() => {
        worker.terminate();
        reject(new Error(`Timed out waiting for comparison from ${workerUrl}`));
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
            id: 2,
            type: 'diff',
            payload: {
              oldZipBytes,
              newZipBytes,
              date: '2026-07-22',
            },
          }, [oldZipBytes, newZipBytes]);
          return;
        }
        if (data.type === 'error') {
          finish(reject, new Error(data.payload));
          return;
        }
        if (data.type === 'diff-result') {
          finish(resolveResult, {
            runtime: data.payload.runtime,
            report: JSON.parse(data.payload.json),
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

async function testResponsiveLayout(page, baseUrl) {
  await page.setViewportSize({ width: 375, height: 812 });
  await page.goto(baseUrl);
  await page.locator('footer').scrollIntoViewIfNeeded();

  const layout = await page.evaluate(() => ({
    clientWidth: document.documentElement.clientWidth,
    scrollWidth: document.documentElement.scrollWidth,
    installCommands: [...document.querySelectorAll('.install-cmd-row code')]
      .map((element) => element.textContent?.trim()),
    navToggleVisible: getComputedStyle(document.querySelector('#nav-toggle')).display !== 'none',
  }));
  assert.equal(
    layout.scrollWidth,
    layout.clientWidth,
    `mobile page overflows by ${layout.scrollWidth - layout.clientWidth}px`,
  );
  assert.equal(layout.installCommands.includes('cargo install gtfs-guru'), true);
  assert.equal(layout.navToggleVisible, true);

  await page.setViewportSize({ width: 1280, height: 720 });
}

let sharedNavigationId = 0;

function sharedReportUrl(baseUrl, payload, marker = 'u') {
  const json = Buffer.from(JSON.stringify(payload));
  const encoded = (marker === 'z' ? gzipSync(json) : json).toString('base64url');
  sharedNavigationId += 1;
  return `${baseUrl}/?shared-test=${sharedNavigationId}#report=${marker}${encoded}`;
}

async function testSharedReportBoundary(page, baseUrl) {
  const pageErrors = [];
  const recordPageError = (error) => pageErrors.push(error.message);
  page.on('pageerror', recordPageError);
  const legitimate = {
    v: 1,
    name: '<b>sample feed</b>',
    at: '2026-07-25',
    counts: [3, 0, 0],
    codes: [['audit_code', 'ERROR', 3]],
    notices: [{
      code: 'audit_code',
      severity: 'ERROR',
      message: 'A legitimate shared finding',
      file: 'stops.txt',
      row: 2,
    }],
    truncated: true,
    sampleLimit: 1,
  };
  await page.goto(sharedReportUrl(baseUrl, legitimate));
  await page.waitForTimeout(1_000);
  const legitimateState = await page.evaluate(() => ({
    modalVisible: !document.querySelector('#report-modal')?.classList.contains('hidden'),
    errorVisible: !document.querySelector('#error-container')?.classList.contains('hidden'),
  }));
  assert.equal(
    legitimateState.modalVisible || legitimateState.errorVisible,
    true,
    `shared report did not finish loading: ${pageErrors.join('; ')}`,
  );
  assert.equal(
    await page.locator('#error-container').getAttribute('class'),
    'error-container hidden',
    await page.locator('#error-message').textContent(),
  );
  assert.equal(await page.locator('.notice-group-count').textContent(), '3');
  assert.match(await page.locator('#shared-banner-text').textContent(), /<b>sample feed<\/b>/);
  assert.equal(await page.locator('#shared-banner-text b').count(), 0);

  await page.goto(sharedReportUrl(baseUrl, legitimate, 'z'));
  await page.locator('#report-modal:not(.hidden)').waitFor();
  assert.equal(await page.locator('.notice-group-count').textContent(), '3');

  const malicious = {
    ...legitimate,
    codes: [[
      'audit_code',
      'ERROR',
      '<img src=x onerror="document.body.dataset.xss=\'yes\'">',
    ]],
  };
  await page.goto(sharedReportUrl(baseUrl, malicious));
  await page.locator('#error-container:not(.hidden)').waitFor();
  assert.equal(await page.evaluate(() => document.body.dataset.xss), undefined);
  assert.equal(await page.locator('img[src="x"]').count(), 0);

  await page.addInitScript(({ decodedLimit }) => {
    globalThis.__shareStreamCancelled = false;
    Object.defineProperty(globalThis, 'DecompressionStream', {
      configurable: true,
      value: class {
        constructor() {
          this.readable = new ReadableStream({
            start(controller) {
              controller.enqueue(new Uint8Array(decodedLimit + 1));
            },
            cancel() {
              globalThis.__shareStreamCancelled = true;
            },
          });
          this.writable = new WritableStream();
        }
      },
    });
  }, { decodedLimit: maxSharedReportDecodedBytes });
  await page.goto(`${baseUrl}/?shared-test=bomb#report=zAA`);
  await page.locator('#error-container:not(.hidden)').waitFor();
  assert.equal(await page.evaluate(() => globalThis.__shareStreamCancelled), true);
  page.off('pageerror', recordPageError);
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
  const threadedDiff = await compare(page);
  const forcedSingleDiff = await compare(page, true);
  assert.equal(threaded.runtime, 'multi-threaded');
  assert.equal(forcedSingle.runtime, 'single-threaded');
  assert.equal(threadedDiff.runtime, 'multi-threaded');
  assert.equal(forcedSingleDiff.runtime, 'single-threaded');
  assert.deepEqual(threadedDiff.report, forcedSingleDiff.report);
  assert.equal(threadedDiff.report.notices.newErrors, 0);
  assert.equal(threadedDiff.report.routes.added.length, 0);
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
  const fallbackDiff = await compare(page);
  assert.equal(fallback.runtime, 'single-threaded');
  assert.equal(fallbackDiff.runtime, 'single-threaded');
  assert.deepEqual(fallback.noticeCounts, forcedSingle.noticeCounts);
  assert.deepEqual(fallbackDiff.report, forcedSingleDiff.report);

  await page.goto(portable.url);
  await page.locator('#diff-mode-btn').click();
  await page.locator('#old-file-input').setInputFiles(fixturePath);
  await page.locator('#new-file-input').setInputFiles(fixturePath);
  await page.locator('#diff-analyze-btn').click();
  await page.locator('#diff-result-state:not(.hidden)').waitFor({ timeout: 120_000 });
  assert.equal(await page.locator('.diff-headline .introduced strong').textContent(), '0');

  await testResponsiveLayout(page, portable.url);
  await testSharedReportBoundary(page, portable.url);

  console.log(JSON.stringify({
    threaded,
    forcedSingle,
    fallback,
    diffRuntime: {
      threaded: threadedDiff.runtime,
      forcedSingle: forcedSingleDiff.runtime,
      fallback: fallbackDiff.runtime,
    },
  }, null, 2));
} finally {
  await browser.close();
  await Promise.all([closeServer(isolated.server), closeServer(portable.server)]);
}
