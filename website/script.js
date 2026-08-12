// Validation runs in a Web Worker so the WASM heap lives off the main thread:
// large feeds no longer freeze the UI, and a hard out-of-memory failure kills
// only the worker instead of the whole tab.
//
// On cross-origin-isolated pages (COOP: same-origin + COEP: require-corp →
// SharedArrayBuffer available) we prefer the multithreaded worker
// (pkg-mt/worker-mt.js), which runs a wasm-bindgen-rayon thread pool to
// parallelize CSV parsing and the validator run. Otherwise, or if that worker
// can't be loaded, we fall back to the single-threaded worker (pkg/worker.js),
// and finally to validating on the main thread so the tool always works — just
// without the off-thread/parallel benefits. OOM mid-validation is a distinct
// case and is surfaced to the user rather than retried.

// Keep these in sync with MAX_FILE_SIZE_BYTES / MAX_UNCOMPRESSED_BYTES in
// crates/gtfs_validator_wasm/src/lib.rs. The real memory gate is the
// UNCOMPRESSED size (wasm peak ≈ 4-5× raw); the zip cap is just a coarse
// sanity check — measured on real feeds, a sparse 129 MB zip fits fine while
// a dense 107 MB zip (1 GB raw) would OOM.
const MAX_FILE_SIZE_BYTES = 150 * 1024 * 1024;
const MAX_UNCOMPRESSED_BYTES = 700 * 1024 * 1024;

// Multithreaded (wasm threads) validation: ~5x faster on large feeds. Only
// engages on cross-origin-isolated pages (COOP/COEP headers set server-side);
// everything else falls back to the single-threaded worker automatically.
const MT_ENABLED = true;

// URL override for testing/rollout: ?mt=1 forces the multithreaded tier on,
// ?mt=0 forces it off, regardless of MT_ENABLED.
const MT_REQUESTED = (() => {
    try {
        const param = new URLSearchParams(location.search).get('mt');
        if (param === '1') return true;
        if (param === '0') return false;
    } catch (_) { /* non-browser context */ }
    return MT_ENABLED;
})();

// Ordered worker tiers to try, best first. The multithreaded tier is only
// offered when enabled AND the page is cross-origin-isolated with
// SharedArrayBuffer available.
const WORKER_TIERS = (() => {
    const tiers = [];
    if (MT_REQUESTED &&
        typeof crossOriginIsolated !== 'undefined' && crossOriginIsolated &&
        typeof SharedArrayBuffer !== 'undefined') {
        tiers.push({ name: 'mt', url: './pkg-mt/worker-mt.js' });
    }
    tiers.push({ name: 'st', url: './pkg/worker.js' });
    return tiers;
})();
let workerTierIndex = 0;        // which tier we're currently using

let validatorWorker = null;
let workerReadyPromise = null; // resolves once the worker signals 'ready'
let workerUsable = true;       // set false once all worker tiers are exhausted
let pendingValidation = null;  // { id, resolve, reject }
let nextMsgId = 1;

// Lazily import + init the WASM module on the main thread (fallback path only,
// always the single-threaded module — the main thread has no rayon pool).
let mainThreadApiPromise = null;
function getMainThreadApi() {
    if (!mainThreadApiPromise) {
        mainThreadApiPromise = import('./pkg/gtfs_guru_wasm.js').then(async (mod) => {
            await mod.default(); // init()
            return mod;
        });
    }
    return mainThreadApiPromise;
}

function getValidatorWorker() {
    if (validatorWorker) return validatorWorker;

    let resolveReady, rejectReady;
    workerReadyPromise = new Promise((res, rej) => { resolveReady = res; rejectReady = rej; });
    let becameReady = false;

    const tier = WORKER_TIERS[workerTierIndex];
    const worker = new Worker(new URL(tier.url, import.meta.url), { type: 'module' });

    worker.onmessage = (e) => {
        const { type, id, payload } = e.data || {};
        if (type === 'ready') { becameReady = true; resolveReady(); return; }
        if (!pendingValidation || id !== pendingValidation.id) return;

        const p = pendingValidation;
        pendingValidation = null;

        if (type === 'result') {
            // Shape the payload like the direct-call ValidationResult so the
            // rest of the UI (report modal, downloads) keeps working unchanged.
            p.resolve({
                json: payload.json,
                html: payload.html,
                error_count: payload.errorCount,
                warning_count: payload.warningCount,
                info_count: payload.infoCount,
                truncated: payload.truncated === true,
            });
        } else if (type === 'diff-result') {
            p.resolve({
                json: payload.json,
                comparison_time_ms: payload.comparisonTimeMs,
                runtime: payload.runtime,
            });
        } else {
            p.reject(new Error(typeof payload === 'string' ? payload : 'Validation failed'));
        }
    };

    worker.onerror = (e) => {
        if (e && e.preventDefault) e.preventDefault();
        const p = pendingValidation;
        pendingValidation = null;
        try { worker.terminate(); } catch (_) { /* ignore */ }
        if (validatorWorker === worker) validatorWorker = null;

        if (!becameReady) {
            // This tier's worker never loaded (blocked script, 403/404, syntax
            // error, or a missing pkg-mt build). Advance to the next tier; only
            // when every tier is exhausted do we give up on workers entirely.
            if (workerTierIndex < WORKER_TIERS.length - 1) {
                workerTierIndex++;
            } else {
                workerUsable = false;
            }
            rejectReady(new Error('__WORKER_UNAVAILABLE__'));
            if (p) p.reject(new Error('__WORKER_UNAVAILABLE__'));
        } else {
            // It was running and died — almost always a hard OOM on a big feed.
            if (p) p.reject(new Error('__OOM__'));
        }
    };

    validatorWorker = worker;
    return worker;
}

// ---- Stops index for the error map ----
// Built from stops.txt before the zip buffer is transferred to the worker
// (the transfer detaches the buffer on the main thread). Small: id -> coords.
let stopsIndex = null;

async function buildStopsIndex(arrayBuffer) {
    const entry = findZipEntry(new DataView(arrayBuffer), 'stops.txt');
    if (!entry) return null;
    const bytes = await readZipEntry(arrayBuffer, entry);
    if (!bytes) return null;
    return parseStopsCsv(new TextDecoder('utf-8').decode(bytes));
}

// Sum of uncompressed entry sizes from the zip central directory (no
// decompression). Returns null when the archive can't be parsed (the wasm
// side will produce a proper error) and Infinity for zip64-sized entries —
// those are far beyond the browser limit anyway.
function sumUncompressedBytes(arrayBuffer) {
    const view = new DataView(arrayBuffer);
    const len = view.byteLength;
    const minPos = Math.max(0, len - 65558); // EOCD + max comment length
    let eocd = -1;
    for (let i = len - 22; i >= minPos; i--) {
        if (view.getUint32(i, true) === 0x06054b50) { eocd = i; break; }
    }
    if (eocd < 0) return null;
    const count = view.getUint16(eocd + 10, true);
    const cdOffset = view.getUint32(eocd + 16, true);
    if (cdOffset === 0xFFFFFFFF) return Infinity; // zip64 archive
    let p = cdOffset;
    let total = 0;
    for (let i = 0; i < count && p + 46 <= len; i++) {
        if (view.getUint32(p, true) !== 0x02014b50) break;
        const uncompSize = view.getUint32(p + 24, true);
        if (uncompSize === 0xFFFFFFFF) return Infinity; // zip64 entry
        total += uncompSize;
        const nameLen = view.getUint16(p + 28, true);
        const extraLen = view.getUint16(p + 30, true);
        const commentLen = view.getUint16(p + 32, true);
        p += 46 + nameLen + extraLen + commentLen;
    }
    return total;
}

// Minimal zip central-directory reader. Feeds are capped at 150 MB zipped, so
// no zip64 handling is needed; anything unusual just means "no map", never an error.
function findZipEntry(view, wantedName) {
    const len = view.byteLength;
    const minPos = Math.max(0, len - 65558); // EOCD + max comment length
    let eocd = -1;
    for (let i = len - 22; i >= minPos; i--) {
        if (view.getUint32(i, true) === 0x06054b50) { eocd = i; break; }
    }
    if (eocd < 0) return null;
    const count = view.getUint16(eocd + 10, true);
    const cdOffset = view.getUint32(eocd + 16, true);
    if (cdOffset === 0xFFFFFFFF) return null; // zip64
    let p = cdOffset;
    let best = null;
    const nameDecoder = new TextDecoder('utf-8');
    for (let i = 0; i < count && p + 46 <= len; i++) {
        if (view.getUint32(p, true) !== 0x02014b50) break;
        const method = view.getUint16(p + 10, true);
        const compSize = view.getUint32(p + 20, true);
        const nameLen = view.getUint16(p + 28, true);
        const extraLen = view.getUint16(p + 30, true);
        const commentLen = view.getUint16(p + 32, true);
        const localOffset = view.getUint32(p + 42, true);
        const name = nameDecoder.decode(new Uint8Array(view.buffer, p + 46, nameLen));
        // Prefer the shallowest match: feeds are sometimes zipped inside a folder.
        if (name.split('/').pop() === wantedName && (!best || name.length < best.name.length)) {
            best = { name, method, compSize, localOffset };
        }
        p += 46 + nameLen + extraLen + commentLen;
    }
    return best;
}

async function readZipEntry(arrayBuffer, entry) {
    const view = new DataView(arrayBuffer);
    const p = entry.localOffset;
    if (view.getUint32(p, true) !== 0x04034b50) return null;
    const nameLen = view.getUint16(p + 26, true);
    const extraLen = view.getUint16(p + 28, true);
    const start = p + 30 + nameLen + extraLen;
    const comp = new Uint8Array(arrayBuffer.slice(start, start + entry.compSize));
    if (entry.method === 0) return comp; // stored
    if (entry.method !== 8) return null;
    const stream = new Blob([comp]).stream().pipeThrough(new DecompressionStream('deflate-raw'));
    return new Uint8Array(await new Response(stream).arrayBuffer());
}

function parseCsv(text) {
    const rows = [];
    let row = [];
    let field = '';
    let inQuotes = false;
    for (let i = 0; i < text.length; i++) {
        const c = text[i];
        if (inQuotes) {
            if (c === '"') {
                if (text[i + 1] === '"') { field += '"'; i++; }
                else inQuotes = false;
            } else {
                field += c;
            }
        } else if (c === '"') {
            inQuotes = true;
        } else if (c === ',') {
            row.push(field); field = '';
        } else if (c === '\n' || c === '\r') {
            if (c === '\r' && text[i + 1] === '\n') i++;
            row.push(field); field = '';
            if (row.length > 1 || row[0] !== '') rows.push(row);
            row = [];
        } else {
            field += c;
        }
    }
    if (field !== '' || row.length) { row.push(field); rows.push(row); }
    return rows;
}

function parseStopsCsv(text) {
    const rows = parseCsv(text);
    if (!rows.length) return null;
    const header = rows[0].map(h => h.replace(/^\uFEFF/, '').trim());
    const idI = header.indexOf('stop_id');
    const latI = header.indexOf('stop_lat');
    const lonI = header.indexOf('stop_lon');
    const nameI = header.indexOf('stop_name');
    if (idI < 0 || latI < 0 || lonI < 0) return null;
    const index = new Map();
    for (let i = 1; i < rows.length; i++) {
        const r = rows[i];
        const id = r[idI];
        const lat = parseFloat(r[latI]);
        const lon = parseFloat(r[lonI]);
        if (id && Number.isFinite(lat) && Number.isFinite(lon)) {
            index.set(id, { lat, lon, name: nameI >= 0 ? (r[nameI] || '').trim() : '' });
        }
    }
    return index.size ? index : null;
}

// ---- Shareable report links ----
// A shared link carries the report inside the URL fragment, which browsers
// never send to a server. That keeps the promise the rest of the page makes:
// the feed and its findings stay on the machine that ran the validation.
const SHARE_FRAGMENT_KEY = 'report=';

// Characters of encoded payload we are willing to put in a link. Browsers cope
// with far more, but chat clients and issue trackers start mangling long URLs.
const SHARE_URL_BUDGET = 30000;

// How many example rows per issue type to keep, tried in order until the link
// fits. The last step still keeps one row per issue type, so every rule that
// fired stays visible with an exact count next to it.
const SHARE_SAMPLE_LIMITS = [Infinity, 25, 5, 1];

// Guards against a hostile link: a few hundred KB of gzip can expand to
// gigabytes, and we would rather refuse than hang the tab.
const SHARE_MAX_ENCODED_BYTES = 2 * 1024 * 1024;
const SHARE_MAX_DECODED_BYTES = 24 * 1024 * 1024;
const SHARE_MAX_NOTICES = 50_000;
const SHARE_MAX_CODES = 4_096;
const SHARE_MAX_COUNT = 0xffff_ffff;
const SHARE_CODE_PATTERN = /^[a-z0-9][a-z0-9_.-]{0,127}$/i;
const SHARE_SEVERITIES = new Set(['ERROR', 'WARNING', 'INFO']);

function bytesToBase64url(bytes) {
    let binary = '';
    const chunkSize = 0x8000; // apply() has an argument-count ceiling
    for (let i = 0; i < bytes.length; i += chunkSize) {
        binary += String.fromCharCode.apply(null, bytes.subarray(i, i + chunkSize));
    }
    return btoa(binary).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
}

function base64urlToBytes(text) {
    const binary = atob(text.replace(/-/g, '+').replace(/_/g, '/'));
    const bytes = new Uint8Array(binary.length);
    for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
    return bytes;
}

async function gzipText(text) {
    const stream = new Blob([text]).stream().pipeThrough(new CompressionStream('gzip'));
    return new Uint8Array(await new Response(stream).arrayBuffer());
}

async function gunzipText(bytes, maxBytes) {
    const stream = new Blob([bytes]).stream().pipeThrough(new DecompressionStream('gzip'));
    const reader = stream.getReader();
    const decoder = new TextDecoder();
    const chunks = [];
    let decodedBytes = 0;

    try {
        while (true) {
            const { value, done } = await reader.read();
            if (done) break;
            decodedBytes += value.byteLength;
            if (decodedBytes > maxBytes) {
                await reader.cancel('Shared report exceeds the decoded-size limit.');
                throw new Error('This shared report is too large to open.');
            }
            chunks.push(decoder.decode(value, { stream: true }));
        }
        chunks.push(decoder.decode());
        return chunks.join('');
    } finally {
        reader.releaseLock();
    }
}

function sharedCount(value) {
    if (!Number.isSafeInteger(value) || value < 0 || value > SHARE_MAX_COUNT) {
        throw new Error('Unrecognised report link.');
    }
    return value;
}

function sharedString(value, maxLength, { optional = false } = {}) {
    if (optional && (value === undefined || value === null)) return null;
    if (typeof value !== 'string' || value.length > maxLength) {
        throw new Error('Unrecognised report link.');
    }
    return value;
}

function normalizeSharedPayload(payload) {
    if (!payload || typeof payload !== 'object' || Array.isArray(payload) || payload.v !== 1) {
        throw new Error('Unrecognised report link.');
    }
    if (!Array.isArray(payload.counts) || payload.counts.length !== 3) {
        throw new Error('Unrecognised report link.');
    }
    if (!Array.isArray(payload.codes) || payload.codes.length > SHARE_MAX_CODES) {
        throw new Error('Unrecognised report link.');
    }
    if (!Array.isArray(payload.notices) || payload.notices.length > SHARE_MAX_NOTICES) {
        throw new Error('Unrecognised report link.');
    }

    const counts = payload.counts.map(sharedCount);
    const codes = payload.codes.map((entry) => {
        if (!Array.isArray(entry) || entry.length !== 3) {
            throw new Error('Unrecognised report link.');
        }
        const code = sharedString(entry[0], 128);
        const severity = sharedString(entry[1], 16);
        if (!SHARE_CODE_PATTERN.test(code) || !SHARE_SEVERITIES.has(severity)) {
            throw new Error('Unrecognised report link.');
        }
        return [code, severity, sharedCount(entry[2])];
    });

    const notices = payload.notices.map((notice) => {
        if (!notice || typeof notice !== 'object' || Array.isArray(notice)) {
            throw new Error('Unrecognised report link.');
        }
        const code = sharedString(notice.code, 128);
        const severity = sharedString(notice.severity, 16);
        sharedString(notice.message, 16_384);
        if (!SHARE_CODE_PATTERN.test(code) || !SHARE_SEVERITIES.has(severity)) {
            throw new Error('Unrecognised report link.');
        }
        return notice;
    });

    const name = sharedString(payload.name, 512, { optional: true });
    const at = sharedString(payload.at, 32, { optional: true });
    if (at !== null && !/^\d{4}-\d{2}-\d{2}$/.test(at)) {
        throw new Error('Unrecognised report link.');
    }
    if (payload.sampleLimit !== undefined) sharedCount(payload.sampleLimit);
    if (payload.truncated !== undefined && typeof payload.truncated !== 'boolean') {
        throw new Error('Unrecognised report link.');
    }

    return {
        v: 1,
        name,
        at,
        counts,
        codes,
        notices,
        truncated: payload.truncated === true,
        sampleLimit: payload.sampleLimit,
    };
}

// The leading marker records how the payload was encoded, so a browser without
// CompressionStream can still produce links that everyone else can open.
async function encodeSharePayload(payload) {
    const json = JSON.stringify(payload);
    if (typeof CompressionStream === 'function') {
        return 'z' + bytesToBase64url(await gzipText(json));
    }
    return 'u' + bytesToBase64url(new TextEncoder().encode(json));
}

async function decodeSharePayload(encoded) {
    const marker = encoded.charAt(0);
    const bytes = base64urlToBytes(encoded.slice(1));
    if (bytes.length > SHARE_MAX_ENCODED_BYTES) {
        throw new Error('This shared report is too large to open.');
    }
    let json;
    if (marker === 'z') {
        if (typeof DecompressionStream !== 'function') {
            throw new Error('This browser cannot open compressed report links.');
        }
        json = await gunzipText(bytes, SHARE_MAX_DECODED_BYTES);
    } else if (marker === 'u') {
        json = new TextDecoder().decode(bytes);
    } else {
        throw new Error('Unrecognised report link.');
    }
    if (new TextEncoder().encode(json).byteLength > SHARE_MAX_DECODED_BYTES) {
        throw new Error('This shared report is too large to open.');
    }
    return normalizeSharedPayload(JSON.parse(json));
}

// Keep the first `limit` examples of each issue type, in the order the
// validator produced them.
function capNoticesPerCode(notices, limit) {
    if (!Number.isFinite(limit)) return notices;
    const seen = new Map();
    const kept = [];
    for (const notice of notices) {
        const code = notice.code;
        const used = seen.get(code) || 0;
        if (used >= limit) continue;
        seen.set(code, used + 1);
        kept.push(notice);
    }
    return kept;
}

function tallyByCode(notices) {
    const totals = new Map();
    for (const notice of notices) {
        const key = notice.code;
        const entry = totals.get(key) || { code: key, severity: notice.severity, count: 0 };
        entry.count += 1;
        totals.set(key, entry);
    }
    return Array.from(totals.values());
}

// Build the smallest payload that still fits the budget. Per-issue-type totals
// travel separately from the examples, so a trimmed link still reports exact
// counts rather than the number of rows that survived trimming.
async function buildShareFragment(result, fileName) {
    let notices = [];
    try {
        notices = JSON.parse(result.json) || [];
    } catch (err) {
        console.warn('Share: could not parse the report', err);
    }

    const base = {
        v: 1,
        name: fileName,
        at: new Date().toISOString().slice(0, 10),
        counts: [
            result.error_count || 0,
            result.warning_count || 0,
            result.info_count || 0,
        ],
        truncated: result.truncated === true,
        codes: tallyByCode(notices).map((entry) => [entry.code, entry.severity, entry.count]),
    };

    let encoded = '';
    let sampleLimit = Infinity;
    for (const limit of SHARE_SAMPLE_LIMITS) {
        sampleLimit = limit;
        const payload = { ...base, notices: capNoticesPerCode(notices, limit) };
        if (Number.isFinite(limit)) payload.sampleLimit = limit;
        encoded = await encodeSharePayload(payload);
        if (encoded.length <= SHARE_URL_BUDGET) break;
    }

    return { encoded, sampleLimit, trimmed: Number.isFinite(sampleLimit) };
}

async function validateInWorker(arrayBuffer, dateStr) {
    getValidatorWorker();
    // Wait until the worker is confirmed loaded BEFORE transferring the buffer.
    // If it never loads, the buffer is untouched and the caller can fall back.
    await Promise.race([
        workerReadyPromise,
        new Promise((_, rej) => setTimeout(() => rej(new Error('__WORKER_UNAVAILABLE__')), 10000)),
    ]);
    return new Promise((resolve, reject) => {
        const id = nextMsgId++;
        pendingValidation = { id, resolve, reject };
        // Transfer (not copy) the ArrayBuffer — feeds can be up to 150 MB.
        validatorWorker.postMessage(
            { type: 'validate', id, payload: { zipBytes: arrayBuffer, date: dateStr } },
            [arrayBuffer],
        );
    });
}

async function validateOnMainThread(arrayBuffer, dateStr) {
    const { validate_gtfs } = await getMainThreadApi();
    // Yield once so the processing spinner paints before the synchronous,
    // UI-blocking WASM call.
    await new Promise((r) => setTimeout(r, 30));
    const res = validate_gtfs(new Uint8Array(arrayBuffer), null, dateStr);
    return {
        json: res.json,
        html: res.html,
        error_count: res.error_count,
        warning_count: res.warning_count,
        info_count: res.info_count,
        truncated: res.truncated === true,
    };
}

async function diffInWorker(oldArrayBuffer, newArrayBuffer, dateStr) {
    getValidatorWorker();
    await Promise.race([
        workerReadyPromise,
        new Promise((_, reject) =>
            setTimeout(() => reject(new Error('__WORKER_UNAVAILABLE__')), 10000)),
    ]);
    return new Promise((resolve, reject) => {
        const id = nextMsgId++;
        pendingValidation = { id, resolve, reject };
        validatorWorker.postMessage(
            {
                type: 'diff',
                id,
                payload: {
                    oldZipBytes: oldArrayBuffer,
                    newZipBytes: newArrayBuffer,
                    date: dateStr,
                },
            },
            [oldArrayBuffer, newArrayBuffer],
        );
    });
}

async function diffOnMainThread(oldArrayBuffer, newArrayBuffer, dateStr) {
    const { diff_gtfs } = await getMainThreadApi();
    await new Promise((resolve) => setTimeout(resolve, 30));
    const startedAt = performance.now();
    return {
        json: diff_gtfs(
            new Uint8Array(oldArrayBuffer),
            new Uint8Array(newArrayBuffer),
            null,
            dateStr,
        ),
        comparison_time_ms: performance.now() - startedAt,
        runtime: 'single-threaded',
    };
}

async function diffFeeds(oldArrayBuffer, newArrayBuffer, dateStr) {
    if (workerUsable) {
        try {
            return await diffInWorker(oldArrayBuffer, newArrayBuffer, dateStr);
        } catch (err) {
            if (!err || err.message !== '__WORKER_UNAVAILABLE__') throw err;
        }
    }
    return diffOnMainThread(oldArrayBuffer, newArrayBuffer, dateStr);
}

// Validate via the worker, transparently falling back to the main thread if the
// worker can't be loaded. A mid-validation OOM ('__OOM__') is NOT retried.
async function validateFeed(arrayBuffer, dateStr) {
    if (workerUsable) {
        try {
            return await validateInWorker(arrayBuffer, dateStr);
        } catch (err) {
            if (err && err.message === '__WORKER_UNAVAILABLE__') {
                // fall through to main-thread validation with the intact buffer
            } else {
                throw err;
            }
        }
    }
    return validateOnMainThread(arrayBuffer, dateStr);
}

document.addEventListener('DOMContentLoaded', () => {
    // The .js class is set synchronously in <head>; the scroll-reveal styles
    // are scoped to it, so markup stays visible if this script never runs.
    const prefersReducedMotion =
        window.matchMedia('(prefers-reduced-motion: reduce)').matches;

    const revealEl = (el) => {
        el.classList.add('visible');
        if (el.querySelector('.stat-number')) startCounters(el);
    };

    /* --- Intersection Observer for Fade-Up Animations --- */
    const observerOptions = {
        threshold: 0.1,
        rootMargin: "0px 0px -50px 0px"
    };

    const observer = new IntersectionObserver((entries) => {
        entries.forEach(entry => {
            if (entry.isIntersecting) {
                revealEl(entry.target);
                observer.unobserve(entry.target);
            }
        });
    }, observerOptions);

    const fadeEls = document.querySelectorAll('.fade-up');
    fadeEls.forEach(el => observer.observe(el));

    // Failsafe: never let content stay invisible. If the observer hasn't
    // revealed an element (deep link landing exactly on a section edge, an
    // early script error, an unsupported environment), show it anyway.
    setTimeout(() => {
        fadeEls.forEach(el => {
            if (!el.classList.contains('visible')) revealEl(el);
        });
    }, 1500);

    /* --- Number Counters --- */
    // Time-based rather than fixed-increment-per-frame: a throttled or
    // background tab drops frames instead of stalling the count partway
    // (the old version could sit on "17x" indefinitely).
    function startCounters(container) {
        container.querySelectorAll('.stat-number').forEach(counter => {
            if (counter.dataset.counted) return;
            counter.dataset.counted = '1';

            const target = +counter.getAttribute('data-target');
            if (prefersReducedMotion || !target) {
                counter.textContent = String(target);
                return;
            }

            const duration = 1200; // ms
            const startedAt = performance.now();
            const step = (now) => {
                const p = Math.min(1, (now - startedAt) / duration);
                const eased = 1 - Math.pow(1 - p, 3);
                counter.textContent = String(Math.round(target * eased));
                if (p < 1) requestAnimationFrame(step);
                else counter.textContent = String(target);
            };
            requestAnimationFrame(step);
        });
    }

    /* --- Mobile nav toggle --- */
    const navToggle = document.getElementById('nav-toggle');
    const navLinks = document.getElementById('nav-links');
    if (navToggle && navLinks) {
        const setNav = (open) => {
            navLinks.classList.toggle('open', open);
            navToggle.setAttribute('aria-expanded', String(open));
            navToggle.setAttribute('aria-label', open ? 'Close menu' : 'Open menu');
            navToggle.innerHTML = `<i data-lucide="${open ? 'x' : 'menu'}"></i>`;
            if (typeof lucide !== 'undefined') lucide.createIcons();
        };

        navToggle.addEventListener('click', () => {
            setNav(!navLinks.classList.contains('open'));
        });

        // Close after choosing a destination, and on Escape.
        navLinks.addEventListener('click', (e) => {
            if (e.target.closest('a')) setNav(false);
        });
        document.addEventListener('keydown', (e) => {
            if (e.key === 'Escape' && navLinks.classList.contains('open')) setNav(false);
        });
    }

    /* --- Copy to Clipboard --- */
    document.querySelectorAll('.copy-btn, .copy-tiny').forEach(btn => {
        btn.addEventListener('click', () => {
            const textToCopy = btn.getAttribute('data-text') ||
                btn.parentElement.querySelector('code, pre')?.innerText;

            if (textToCopy) {
                navigator.clipboard.writeText(textToCopy).then(() => {
                    const originalIcon = btn.innerHTML;
                    const isTiny = btn.classList.contains('copy-tiny');

                    if (isTiny) {
                        btn.innerText = 'Copied!';
                    } else {
                        btn.innerHTML = '<i data-lucide="check"></i>';
                        lucide.createIcons();
                    }

                    setTimeout(() => {
                        btn.innerHTML = originalIcon;
                        if (!isTiny) lucide.createIcons();
                    }, 2000);
                });
            }
        });
    });

    /* --- Smooth Scrolling for Navigation --- */
    // A scripted scroll is not covered by the stylesheet: scroll-behavior only
    // governs CSS-initiated scrolling, so window.scrollTo() has to read the
    // preference itself. Queried per click rather than cached, so switching the
    // system setting mid-session takes effect without a reload.
    const reducedMotion = window.matchMedia('(prefers-reduced-motion: reduce)');
    document.querySelectorAll('a[href^="#"]').forEach(anchor => {
        anchor.addEventListener('click', function (e) {
            e.preventDefault();
            const targetId = this.getAttribute('href');
            if (targetId === '#') return;

            const targetElement = document.querySelector(targetId);
            if (targetElement) {
                // Account for fixed header
                const headerOffset = 80;
                const elementPosition = targetElement.getBoundingClientRect().top;
                const offsetPosition = elementPosition + window.pageYOffset - headerOffset;

                window.scrollTo({
                    top: offsetPosition,
                    behavior: reducedMotion.matches ? "auto" : "smooth"
                });
            }
        });
    });

    /* --- Validator Logic --- */
    const dropZone = document.getElementById('drop-zone');
    const fileInput = document.getElementById('file-input');
    const uploadState = document.getElementById('upload-state');
    const validateInputs = document.getElementById('validate-inputs');
    const diffInputs = document.getElementById('diff-inputs');
    const validateModeBtn = document.getElementById('validate-mode-btn');
    const diffModeBtn = document.getElementById('diff-mode-btn');
    const oldFileInput = document.getElementById('old-file-input');
    const newFileInput = document.getElementById('new-file-input');
    const oldFeedPicker = document.getElementById('old-feed-picker');
    const newFeedPicker = document.getElementById('new-feed-picker');
    const oldFeedName = document.getElementById('old-feed-name');
    const newFeedName = document.getElementById('new-feed-name');
    const diffAnalyzeBtn = document.getElementById('diff-analyze-btn');
    const processingState = document.getElementById('processing-state');
    const processingTitle = document.getElementById('processing-title');
    const processingDetail = document.getElementById('processing-detail');
    const resultState = document.getElementById('result-state');
    const diffResultState = document.getElementById('diff-result-state');
    const diffReport = document.getElementById('diff-report');
    const diffTime = document.getElementById('diff-time');
    const downloadDiffJsonBtn = document.getElementById('download-diff-json-btn');
    const diffResetBtn = document.getElementById('diff-reset-btn');
    const resetBtn = document.getElementById('reset-btn');
    const urlInput = document.getElementById('url-input');
    const urlAnalyzeBtn = document.getElementById('url-analyze-btn');
    const urlInputContainer = document.getElementById('url-input-container');
    const demoRow = document.getElementById('demo-row');
    const tryDemoBtn = document.getElementById('try-demo-btn');
    const sharedBanner = document.getElementById('shared-banner');
    const sharedBannerText = document.getElementById('shared-banner-text');
    const sharedNewScanBtn = document.getElementById('shared-new-scan-btn');
    const mcpPreview = document.getElementById('mcp-preview');
    const mcpPreviewBody = document.getElementById('mcp-preview-body');

    let currentValidatorMode = 'validate';
    let oldDiffFile = null;
    let newDiffFile = null;
    let lastDiffResult = null;

    function setUploadUiVisible(visible) {
        if (validateInputs) {
            validateInputs.classList.toggle(
                'hidden',
                !visible || currentValidatorMode !== 'validate'
            );
        }
        if (diffInputs) {
            diffInputs.classList.toggle(
                'hidden',
                !visible || currentValidatorMode !== 'diff'
            );
        }
    }

    function setValidatorMode(mode) {
        currentValidatorMode = mode;
        validateModeBtn?.classList.toggle('active', mode === 'validate');
        diffModeBtn?.classList.toggle('active', mode === 'diff');
        validateModeBtn?.setAttribute('aria-selected', String(mode === 'validate'));
        diffModeBtn?.setAttribute('aria-selected', String(mode === 'diff'));
        processingState?.classList.add('hidden');
        resultState?.classList.add('hidden');
        diffResultState?.classList.add('hidden');
        document.getElementById('error-container')?.classList.add('hidden');
        if (processingTitle) {
            processingTitle.textContent = mode === 'diff'
                ? 'Comparing feeds locally (WASM)…'
                : 'Validating locally (WASM)…';
        }
        if (processingDetail) {
            processingDetail.textContent = mode === 'diff'
                ? 'Validating both releases, then comparing semantic changes'
                : 'Large feeds (50 MB+) can take a minute or two';
        }
        setUploadUiVisible(true);
    }

    function updateDiffSelection(slot, file) {
        if (!file || !file.name.toLowerCase().endsWith('.zip')) {
            showValidationError('Please choose a GTFS ZIP file.');
            return;
        }
        if (file.size > MAX_FILE_SIZE_BYTES) {
            showTooLarge(file.size);
            return;
        }
        if (slot === 'old') {
            oldDiffFile = file;
            oldFeedName.textContent = file.name;
            oldFeedPicker.classList.add('ready');
        } else {
            newDiffFile = file;
            newFeedName.textContent = file.name;
            newFeedPicker.classList.add('ready');
        }
        diffAnalyzeBtn.disabled = !(oldDiffFile && newDiffFile);
    }

    function resetDiffSelection() {
        oldDiffFile = null;
        newDiffFile = null;
        lastDiffResult = null;
        if (oldFileInput) oldFileInput.value = '';
        if (newFileInput) newFileInput.value = '';
        if (oldFeedName) oldFeedName.textContent = 'Choose GTFS.zip';
        if (newFeedName) newFeedName.textContent = 'Choose GTFS.zip';
        oldFeedPicker?.classList.remove('ready');
        newFeedPicker?.classList.remove('ready');
        if (diffAnalyzeBtn) diffAnalyzeBtn.disabled = true;
        diffResultState?.classList.add('hidden');
        setUploadUiVisible(true);
    }

    // UI Elements for results
    const errorCountEl = document.getElementById('error-count');
    const warningCountEl = document.getElementById('warning-count');
    const scoreNumberEl = document.getElementById('score-number');
    const scoreRingEl = document.getElementById('score-ring');

    // Report modal elements
    const reportModal = document.getElementById('report-modal');
    const reportModalBody = document.getElementById('report-modal-body');
    const viewReportBtn = document.getElementById('view-report-btn');
    const closeModalBtn = document.getElementById('close-modal-btn');
    const downloadJsonBtn = document.getElementById('download-json-btn');
    const downloadHtmlBtn = document.getElementById('download-html-btn');
    const downloadJsonModalBtn = document.getElementById('download-json-modal-btn');
    const openWindowBtn = document.getElementById('open-window-btn');

    // Store validation result
    let lastValidationResult = null;
    let lastFileName = 'gtfs_validation';

    if (dropZone && fileInput) {
        validateModeBtn?.addEventListener('click', () => setValidatorMode('validate'));
        diffModeBtn?.addEventListener('click', () => setValidatorMode('diff'));

        // Drag & Drop
        dropZone.addEventListener('dragover', (e) => {
            e.preventDefault();
            dropZone.style.transform = 'scale(1.02)';
        });

        dropZone.addEventListener('dragleave', (e) => {
            e.preventDefault();
            dropZone.style.transform = 'scale(1)';
        });

        dropZone.addEventListener('drop', (e) => {
            e.preventDefault();
            dropZone.style.transform = 'scale(1)';
            if (e.dataTransfer.files.length) {
                const files = Array.from(e.dataTransfer.files);
                if (currentValidatorMode === 'diff') {
                    if (files[1]) {
                        updateDiffSelection('old', files[0]);
                        updateDiffSelection('new', files[1]);
                    } else if (!oldDiffFile) {
                        updateDiffSelection('old', files[0]);
                    } else {
                        updateDiffSelection('new', files[0]);
                    }
                } else {
                    handleFile(files[0]);
                }
            }
        });

        // Click to browse
        uploadState.addEventListener('click', () => fileInput.click());

        fileInput.addEventListener('change', (e) => {
            if (e.target.files.length) {
                handleFile(e.target.files[0]);
            }
        });

        oldFeedPicker?.addEventListener('click', () => oldFileInput?.click());
        newFeedPicker?.addEventListener('click', () => newFileInput?.click());
        oldFileInput?.addEventListener('change', (e) => {
            if (e.target.files[0]) updateDiffSelection('old', e.target.files[0]);
        });
        newFileInput?.addEventListener('change', (e) => {
            if (e.target.files[0]) updateDiffSelection('new', e.target.files[0]);
        });
        diffAnalyzeBtn?.addEventListener('click', runDiffComparison);
        diffResetBtn?.addEventListener('click', resetDiffSelection);
        downloadDiffJsonBtn?.addEventListener('click', downloadDiffJSON);

        // URL Analysis
        if (urlAnalyzeBtn && urlInput) {
            urlAnalyzeBtn.addEventListener('click', () => {
                const url = urlInput.value.trim();
                if (url) {
                    handleUrl(url);
                }
            });

            urlInput.addEventListener('keypress', (e) => {
                if (e.key === 'Enter') {
                    const url = urlInput.value.trim();
                    if (url) {
                        handleUrl(url);
                    }
                }
            });
        }

        // Example feed
        if (tryDemoBtn) {
            tryDemoBtn.addEventListener('click', handleDemoFeed);
        }

        // Reset
        function resetValidator() {
            resultState.classList.add('hidden');
            diffResultState?.classList.add('hidden');
            setUploadUiVisible(true);
            fileInput.value = '';
            urlInput.value = '';
            lastValidationResult = null;
            lastDiffResult = null;
            setSharedBanner(null);
            // A stale #report= would otherwise reopen the shared report on reload.
            if (location.hash.startsWith('#' + SHARE_FRAGMENT_KEY)) {
                history.replaceState(null, '', location.pathname + location.search);
            }
        }

        if (resetBtn) {
            resetBtn.addEventListener('click', resetValidator);
        }
        if (sharedNewScanBtn) {
            sharedNewScanBtn.addEventListener('click', resetValidator);
        }

        // View Report button
        if (viewReportBtn) {
            viewReportBtn.addEventListener('click', () => {
                if (lastValidationResult) {
                    showReportModal(lastValidationResult);
                }
            });
        }

        // Close modal
        if (closeModalBtn) {
            closeModalBtn.addEventListener('click', closeReportModal);
        }

        // Close modal on backdrop click
        if (reportModal) {
            reportModal.addEventListener('click', (e) => {
                if (e.target === reportModal) {
                    closeReportModal();
                }
            });
        }

        // Download JSON buttons
        if (downloadJsonBtn) {
            downloadJsonBtn.addEventListener('click', downloadValidationJSON);
        }
        if (downloadHtmlBtn) {
            downloadHtmlBtn.addEventListener('click', downloadValidationHTML);
        }
        if (downloadJsonModalBtn) {
            downloadJsonModalBtn.addEventListener('click', downloadValidationJSON);
        }
        const downloadHtmlModalBtn = document.getElementById('download-html-modal-btn');
        if (downloadHtmlModalBtn) {
            downloadHtmlModalBtn.addEventListener('click', downloadValidationHTML);
        }

        // Share buttons
        document.querySelectorAll('#share-btn, #share-modal-btn').forEach((btn) => {
            btn.addEventListener('click', () => shareReport(btn));
        });

        // Open in new window
        if (openWindowBtn) {
            openWindowBtn.addEventListener('click', () => {
                if (lastValidationResult) {
                    openReportWindow(lastValidationResult);
                }
            });
        }

        // Close modal on Escape (map modal first, then the report modal)
        document.addEventListener('keydown', (e) => {
            if (e.key !== 'Escape') return;
            const mapModal = document.getElementById('map-modal');
            if (mapModal && !mapModal.classList.contains('hidden')) {
                mapModal.classList.add('hidden');
                return;
            }
            if (reportModal && !reportModal.classList.contains('hidden')) {
                closeReportModal();
            }
        });

        // A #report= link opens straight into the shared result.
        restoreSharedReport();
    }

    async function handleFile(file) {
        if (!file.name.endsWith('.zip')) {
            alert('Please upload a ZIP file.');
            return;
        }

        if (file.size > MAX_FILE_SIZE_BYTES) {
            showTooLarge(file.size);
            return;
        }

        // Start compiling WASM and creating the rayon pool while the browser
        // reads the selected archive from disk.
        getValidatorWorker();

        lastFileName = file.name.replace('.zip', '');

        // Show processing
        const errorContainer = document.getElementById('error-container');
        if (errorContainer) errorContainer.classList.add('hidden');

        setUploadUiVisible(false);
        processingState.classList.remove('hidden');

        try {
            const arrayBuffer = await file.arrayBuffer();
            const rawBytes = sumUncompressedBytes(arrayBuffer);
            if (rawBytes !== null && rawBytes > MAX_UNCOMPRESSED_BYTES) {
                showTooDense(rawBytes);
                return;
            }
            await runValidation(arrayBuffer);
        } catch (err) {
            console.error("File reading error:", err);
            processingState.classList.add('hidden');
            setUploadUiVisible(true);
        }
    }

    async function handleUrl(url) {
        // Overlap WASM initialization with the network request.
        getValidatorWorker();

        // Show processing
        const errorContainer = document.getElementById('error-container');
        if (errorContainer) errorContainer.classList.add('hidden');

        setUploadUiVisible(false);
        processingState.classList.remove('hidden');

        const tryFetch = async (fetchUrl) => {
            const response = await fetch(fetchUrl);
            if (!response.ok) {
                throw new Error(`Failed to fetch: ${response.statusText}`);
            }
            return await response.arrayBuffer();
        };

        try {
            let arrayBuffer;
            try {
                // Try direct fetch first
                arrayBuffer = await tryFetch(url);
            } catch (err) {
                console.warn("Direct fetch failed, retrying through the validator service...", err);
                // The same-origin Rust endpoint only connects to public
                // addresses and enforces rate, concurrency, redirect and size limits.
                const proxyUrl = '/cors-proxy?url=' + encodeURIComponent(url);
                arrayBuffer = await tryFetch(proxyUrl);
            }

            if (arrayBuffer.byteLength > MAX_FILE_SIZE_BYTES) {
                showTooLarge(arrayBuffer.byteLength);
                return;
            }
            const rawBytes = sumUncompressedBytes(arrayBuffer);
            if (rawBytes !== null && rawBytes > MAX_UNCOMPRESSED_BYTES) {
                showTooDense(rawBytes);
                return;
            }

            // Try to extract filename from URL
            try {
                const urlObj = new URL(url);
                const pathname = urlObj.pathname;
                const filename = pathname.substring(pathname.lastIndexOf('/') + 1);
                if (filename && filename.endsWith('.zip')) {
                    lastFileName = filename.replace('.zip', '');
                } else {
                    lastFileName = 'remote_feed';
                }
            } catch (e) {
                lastFileName = 'remote_feed';
            }

            await runValidation(arrayBuffer);

        } catch (err) {
            console.error("URL fetch error:", err);
            alert(`Error loading from URL: ${err.message}\n
The feed host blocked the browser request, and fetching it through gtfs.guru also failed.
Download the .zip and drop it here instead; validation still runs locally in your browser.`);
            processingState.classList.add('hidden');
            setUploadUiVisible(true);
        }
    }

    // The example feed ships with the site: a two-route network carrying a
    // handful of deliberate mistakes, so a first-time visitor with no gtfs.zip
    // to hand still sees a real report.
    async function handleDemoFeed() {
        getValidatorWorker();

        const errorContainer = document.getElementById('error-container');
        if (errorContainer) errorContainer.classList.add('hidden');
        setSharedBanner(null);

        setUploadUiVisible(false);
        processingState.classList.remove('hidden');

        try {
            const response = await fetch('demo/gtfs-guru-demo.zip');
            if (!response.ok) {
                throw new Error(`HTTP ${response.status}`);
            }
            lastFileName = 'gtfs-guru-demo';
            await runValidation(await response.arrayBuffer());
        } catch (err) {
            console.error('Demo feed error:', err);
            showValidationError(
                'Could not load the example feed. Drop your own gtfs.zip here instead — ' +
                'validation runs the same way, locally in your browser.'
            );
        }
    }

    async function runDiffComparison() {
        if (!oldDiffFile || !newDiffFile) return;
        getValidatorWorker();
        document.getElementById('error-container')?.classList.add('hidden');
        setUploadUiVisible(false);
        resultState?.classList.add('hidden');
        diffResultState?.classList.add('hidden');
        if (processingTitle) processingTitle.textContent = 'Comparing feeds locally (WASM)…';
        if (processingDetail) {
            processingDetail.textContent =
                'Validating both releases, then comparing routes, stops, trips, and notices';
        }
        processingState.classList.remove('hidden');

        try {
            const [oldBuffer, newBuffer] = await Promise.all([
                oldDiffFile.arrayBuffer(),
                newDiffFile.arrayBuffer(),
            ]);
            const combinedZipBytes = oldBuffer.byteLength + newBuffer.byteLength;
            if (combinedZipBytes > MAX_FILE_SIZE_BYTES) {
                showValidationError(
                    `The two feeds total ${(combinedZipBytes / (1024 * 1024)).toFixed(1)} MB. ` +
                    `Browser comparison is limited to ${MAX_FILE_SIZE_BYTES / (1024 * 1024)} MB combined. ` +
                    'Use the desktop app or CLI for larger comparisons.'
                );
                return;
            }
            const oldRawBytes = sumUncompressedBytes(oldBuffer);
            const newRawBytes = sumUncompressedBytes(newBuffer);
            if (oldRawBytes !== null && newRawBytes !== null &&
                oldRawBytes + newRawBytes > MAX_UNCOMPRESSED_BYTES) {
                showValidationError(
                    `The two feeds unpack to ${Math.round((oldRawBytes + newRawBytes) / (1024 * 1024))} MB. ` +
                    `Browser comparison is limited to ${MAX_UNCOMPRESSED_BYTES / (1024 * 1024)} MB combined. ` +
                    'Use the desktop app or CLI for larger comparisons.'
                );
                return;
            }

            const dateStr = new Date().toISOString().split('T')[0];
            const result = await diffFeeds(oldBuffer, newBuffer, dateStr);
            result.report = JSON.parse(result.json);
            lastDiffResult = result;
            showDiffResults(result);
        } catch (err) {
            console.error('Feed comparison error:', err);
            const message = err?.message === '__OOM__'
                ? 'These feeds were too large to compare in this browser. Use the desktop app or CLI.'
                : (err?.message || 'Could not compare these feeds.');
            showValidationError(message);
        }
    }

    function diffSection(title, rows) {
        if (!rows.length) return '';
        const visibleRows = rows.slice(0, 200);
        const items = visibleRows.map(([label, value]) => `
            <li><code>${escapeHtml(String(label))}</code><span>${escapeHtml(String(value))}</span></li>
        `).join('');
        const remaining = rows.length - visibleRows.length;
        return `
            <details class="diff-section">
                <summary>${escapeHtml(title)} <span>${rows.length.toLocaleString()} changes</span></summary>
                <ul class="diff-list">
                    ${items}
                    ${remaining > 0 ? `<li><span>And ${remaining.toLocaleString()} more</span><span>JSON</span></li>` : ''}
                </ul>
            </details>
        `;
    }

    function entityRows(entity) {
        return [
            ...(entity.added || []).map((id) => [id, 'Added']),
            ...(entity.removed || []).map((id) => [id, 'Removed']),
            ...(entity.changed || []).map((id) => [id, 'Changed']),
        ];
    }

    function renderDiffReport(report) {
        const routeAdds = report.routes?.added?.length || 0;
        const routeRemoves = report.routes?.removed?.length || 0;
        const stopAdds = report.stops?.added?.length || 0;
        const stopRemoves = report.stops?.removed?.length || 0;
        const movedStops = report.stops?.moved?.length || 0;
        const tripChanges = report.tripsByRoute?.length || 0;

        const stopRows = [
            ...entityRows(report.stops || {}),
            ...(report.stops?.renamed || []).map((id) => [id, 'Renamed']),
            ...(report.stops?.moved || []).map((move) => [
                move.stopId,
                `Moved ${Number(move.distanceMeters).toFixed(0)} m`,
            ]),
        ];
        const tripRows = (report.tripsByRoute || []).map((change) => [
            change.routeId,
            `${change.oldCount.toLocaleString()} → ${change.newCount.toLocaleString()} trips`,
        ]);
        const frequencyRows = (report.frequenciesByRoute || []).map((change) => [
            change.routeId,
            `${change.oldWindows.length} → ${change.newWindows.length} windows`,
        ]);
        const noticeRows = (report.notices?.changes || []).map((change) => [
            change.code,
            `${change.severity} · ${change.oldCount.toLocaleString()} → ${change.newCount.toLocaleString()}`,
        ]);
        const fileRows = [
            ...(report.files?.added || []).map((name) => [name, 'Added']),
            ...(report.files?.removed || []).map((name) => [name, 'Removed']),
        ];

        const sections = [
            diffSection('Routes', entityRows(report.routes || {})),
            diffSection('Stops', stopRows),
            diffSection('Trips by route', tripRows),
            diffSection('Frequencies', frequencyRows),
            diffSection('Validation notices', noticeRows),
            diffSection('Agencies', entityRows(report.agencies || {})),
            diffSection('Files', fileRows),
        ].join('');
        const hasChanges = sections.length > 0 || report.feedInfo?.changed;

        return `
            <div class="diff-headline">
                <div class="introduced"><strong>${(report.notices?.newErrors || 0).toLocaleString()}</strong><span>New errors</span></div>
                <div class="resolved"><strong>${(report.notices?.resolvedErrors || 0).toLocaleString()}</strong><span>Resolved errors</span></div>
            </div>
            <div class="diff-metrics">
                <div class="diff-metric"><strong>+${routeAdds} / −${routeRemoves}</strong><span>Routes</span></div>
                <div class="diff-metric"><strong>+${stopAdds} / −${stopRemoves}</strong><span>Stops</span></div>
                <div class="diff-metric"><strong>${movedStops}</strong><span>Stops moved</span></div>
                <div class="diff-metric"><strong>${tripChanges}</strong><span>Trip count changes</span></div>
                <div class="diff-metric"><strong>${frequencyRows.length}</strong><span>Frequency changes</span></div>
                <div class="diff-metric"><strong>${noticeRows.length}</strong><span>Issue groups changed</span></div>
            </div>
            ${report.feedInfo?.changed ? diffSection('Feed information', [[
                report.feedInfo.oldVersion || 'Previous',
                report.feedInfo.newVersion || 'New version',
            ]]) : ''}
            ${sections}
            ${hasChanges ? '' : '<p class="sub-text">No semantic changes found.</p>'}
        `;
    }

    function showDiffResults(result) {
        processingState.classList.add('hidden');
        diffResultState.classList.remove('hidden');
        if (diffTime) {
            diffTime.textContent = `${(result.comparison_time_ms / 1000).toFixed(1)} s · ${result.runtime || 'WASM'}`;
        }
        diffReport.innerHTML = renderDiffReport(result.report);
        if (typeof lucide !== 'undefined') lucide.createIcons();
    }

    function downloadDiffJSON() {
        if (!lastDiffResult) return;
        const blob = new Blob(
            [JSON.stringify(lastDiffResult.report, null, 2)],
            { type: 'application/json' }
        );
        const url = URL.createObjectURL(blob);
        const anchor = document.createElement('a');
        anchor.href = url;
        anchor.download = `${oldDiffFile?.name || 'old'}_to_${newDiffFile?.name || 'new'}_diff.json`;
        document.body.appendChild(anchor);
        anchor.click();
        anchor.remove();
        URL.revokeObjectURL(url);
    }

    /* --- Sharing --- */

    let toastTimer = null;

    function showToast(message) {
        let toast = document.getElementById('share-toast');
        if (!toast) {
            toast = document.createElement('div');
            toast.id = 'share-toast';
            toast.className = 'toast';
            toast.setAttribute('role', 'status');
            toast.setAttribute('aria-live', 'polite');
            document.body.appendChild(toast);
        }
        toast.textContent = message;
        // Force a reflow so the transition runs when the toast is reused.
        void toast.offsetWidth;
        toast.classList.add('visible');
        clearTimeout(toastTimer);
        toastTimer = setTimeout(() => toast.classList.remove('visible'), 4000);
    }

    function setSharedBanner(text) {
        if (!sharedBanner) return;
        if (!text) {
            sharedBanner.classList.add('hidden');
            return;
        }
        if (sharedBannerText) sharedBannerText.textContent = text;
        sharedBanner.classList.remove('hidden');
        if (typeof lucide !== 'undefined') lucide.createIcons();
    }

    async function shareReport(button) {
        if (!lastValidationResult) return;

        const originalLabel = button.innerHTML;
        button.disabled = true;
        try {
            const { encoded, sampleLimit, trimmed } =
                await buildShareFragment(lastValidationResult, lastFileName);
            const url = `${location.origin}${location.pathname}#${SHARE_FRAGMENT_KEY}${encoded}`;

            let copied = false;
            try {
                await navigator.clipboard.writeText(url);
                copied = true;
            } catch (err) {
                console.warn('Clipboard unavailable, falling back to the address bar', err);
                history.replaceState(null, '', url);
            }

            const trimNote = trimmed
                ? ` Trimmed to ${sampleLimit} example${sampleLimit === 1 ? '' : 's'} per issue type; the counts stay exact.`
                : '';
            showToast(
                (copied
                    ? 'Link copied. The report travels inside the link — nothing was uploaded.'
                    : 'Link is now in the address bar. The report travels inside it — nothing was uploaded.') +
                trimNote
            );
        } catch (err) {
            console.error('Share failed:', err);
            showToast('Could not build a share link for this report.');
        } finally {
            button.disabled = false;
            button.innerHTML = originalLabel;
            if (typeof lucide !== 'undefined') lucide.createIcons();
        }
    }

    // Rebuild the result view from a shared link. There is no feed here — only
    // what the sender's browser found — so the HTML report and the stop map,
    // which both need the archive, stay out of reach.
    function applySharedReport(payload) {
        const notices = payload.notices;
        const totalsByCode = Object.create(null);
        for (const entry of payload.codes) {
            totalsByCode[entry[0]] = entry[2];
        }

        stopsIndex = null;
        lastFileName = payload.name || 'shared_report';
        lastValidationResult = {
            json: JSON.stringify(notices),
            html: null,
            error_count: payload.counts[0],
            warning_count: payload.counts[1],
            info_count: payload.counts[2],
            truncated: payload.truncated,
            totalsByCode,
            shared: true,
        };

        setUploadUiVisible(false);
        processingState.classList.add('hidden');
        showResults(lastValidationResult);

        const validatedOn = payload.at;
        setSharedBanner(
            `Shared report for ${lastFileName}` +
            (validatedOn ? `, validated on ${validatedOn}` : '') +
            '. The feed itself was never uploaded.'
        );
        showReportModal(lastValidationResult);
    }

    async function restoreSharedReport() {
        const hash = location.hash.startsWith('#') ? location.hash.slice(1) : '';
        if (!hash.startsWith(SHARE_FRAGMENT_KEY)) return;

        try {
            applySharedReport(await decodeSharePayload(hash.slice(SHARE_FRAGMENT_KEY.length)));
        } catch (err) {
            console.error('Could not open the shared report:', err);
            showValidationError(
                'This report link could not be opened — it may have been truncated on the way here. ' +
                'Ask the sender to resend it, or validate the feed yourself.'
            );
        }
    }

    // Show a message in the inline error banner and return to the upload state.
    function showValidationError(msg) {
        const errorContainer = document.getElementById('error-container');
        const errorMessage = document.getElementById('error-message');
        if (errorContainer && errorMessage) {
            errorMessage.textContent = msg;
            errorContainer.classList.remove('hidden');
            if (typeof lucide !== 'undefined') lucide.createIcons();
        } else {
            alert(msg);
        }
        processingState.classList.add('hidden');
        setUploadUiVisible(true);
    }

    function showTooLarge(sizeBytes) {
        const sizeMb = (sizeBytes / (1024 * 1024)).toFixed(1);
        showValidationError(
            `This feed is ${sizeMb} MB. In-browser validation is capped at 150 MB (zipped). ` +
            `For larger feeds use the free desktop app or CLI (see the Download section) — they handle any size.`
        );
    }

    function showTooDense(rawBytes) {
        const rawMb = Number.isFinite(rawBytes) ? Math.round(rawBytes / (1024 * 1024)) + ' MB' : 'over 4 GB';
        showValidationError(
            `This feed unpacks to ${rawMb} of data — more than a browser tab can hold in memory ` +
            `(limit: ${MAX_UNCOMPRESSED_BYTES / (1024 * 1024)} MB uncompressed). ` +
            `Use the free desktop app or CLI (see the Download section) — they handle any size.`
        );
    }

    async function runValidation(arrayBuffer) {
        const dateStr = new Date().toISOString().split('T')[0];
        // Build the stop_id -> coordinates index before the buffer is
        // transferred to the worker (transfer detaches it on this thread).
        stopsIndex = null;
        try {
            stopsIndex = await buildStopsIndex(arrayBuffer);
        } catch (e) {
            console.warn('Stops index unavailable (map disabled):', e);
        }
        try {
            const result = await validateFeed(arrayBuffer, dateStr);
            lastValidationResult = result;
            showResults(result);
        } catch (err) {
            console.error("Validation error:", err);
            if (err && err.message === '__OOM__') {
                showValidationError(
                    "This feed was too large to validate in the browser on this device. " +
                    "Try a Chrome/Firefox-based desktop browser, or use the free desktop app or CLI for large feeds."
                );
                return;
            }
            let msg = "Error processing file. See console for details.";
            if (typeof err === 'string') {
                msg = err;
            } else if (err && err.message) {
                msg = err.message;
            }
            showValidationError(msg);
        }
    }

    // Close error button
    const closeErrorBtn = document.getElementById('close-error-btn');
    if (closeErrorBtn) {
        closeErrorBtn.addEventListener('click', () => {
            const errorContainer = document.getElementById('error-container');
            if (errorContainer) errorContainer.classList.add('hidden');
        });
    }

    function countLabel(count, singular, plural = `${singular}s`) {
        return `${count.toLocaleString()} ${count === 1 ? singular : plural}`;
    }

    function noticeContextValue(notice, key) {
        const value = notice?.context?.[key];
        return value === null || value === undefined || value === '' ? null : value;
    }

    function renderMcpPreview(result) {
        if (!mcpPreview || !mcpPreviewBody) return;

        let notices = [];
        try {
            notices = JSON.parse(result.json) || [];
        } catch (error) {
            console.warn('MCP preview: could not parse notices', error);
        }

        const groups = new Map();
        notices
            .filter((notice) => notice.severity === 'ERROR' || notice.severity === 'WARNING')
            .forEach((notice) => {
                const key = `${notice.severity}:${notice.code}`;
                let group = groups.get(key);
                if (!group) {
                    group = {
                        code: notice.code,
                        severity: notice.severity,
                        examples: [],
                        stored: 0,
                        total: 0,
                    };
                    groups.set(key, group);
                }
                group.stored += 1;
                if (group.examples.length < 3) group.examples.push(notice);
                const exactTotal = Number(
                    notice.totalNotices ?? result.totalsByCode?.[notice.code] ?? group.stored
                );
                group.total = Math.max(group.total, Number.isFinite(exactTotal) ? exactTotal : group.stored);
            });

        const priority = { ERROR: 0, WARNING: 1 };
        const featuredGroups = [...groups.values()]
            .sort((left, right) =>
                priority[left.severity] - priority[right.severity]
                || right.total - left.total
                || left.code.localeCompare(right.code)
            )
            .slice(0, 3);

        const errors = Number(result.error_count) || 0;
        const warnings = Number(result.warning_count) || 0;
        const feedName = lastFileName || 'this feed';
        const verdict = errors > 0
            ? `I checked ${feedName}. It has ${countLabel(errors, 'validation error')} and ${countLabel(warnings, 'warning')}. Fix the errors before publication.`
            : warnings > 0
                ? `I checked ${feedName}. It has no validation errors and ${countLabel(warnings, 'warning')} to review.`
                : `I checked ${feedName}. The validator found no errors or warnings.`;

        const identifierKeys = [
            'stopId',
            'routeId',
            'tripId',
            'serviceId',
            'shapeId',
            'agencyId',
            'parentStation',
            'locationId',
        ];
        const issuesHtml = featuredGroups.map((group) => {
            const notice = group.examples[0];
            const file = notice.file || noticeContextValue(notice, 'filename');
            const row = notice.row ?? noticeContextValue(notice, 'csvRowNumber');
            const field = notice.field || noticeContextValue(notice, 'fieldName');
            const location = [];
            if (file) location.push(String(file));
            if (row !== null && row !== undefined) location.push(`row ${row}`);
            if (field) location.push(String(field));
            for (const key of identifierKeys) {
                const value = noticeContextValue(notice, key);
                if (value !== null && location.length < 5) location.push(`${key}=${value}`);
            }
            const locationHtml = location.length
                ? `<div class="mcp-example-location">${location.map((part) =>
                    `<code>${escapeHtml(String(part))}</code>`).join('')}</div>`
                : '';
            const fix = notice.fix?.description
                ? `<p class="mcp-example-fix"><strong>Suggested fix:</strong> ${escapeHtml(notice.fix.description)}</p>`
                : '';

            return `
                <li class="mcp-example ${group.severity.toLowerCase()}">
                    <div class="mcp-example-topline">
                        <code>${escapeHtml(group.code)}</code>
                        <span>${escapeHtml(group.severity)} · ${escapeHtml(countLabel(group.total, 'occurrence'))}</span>
                    </div>
                    <p>${escapeHtml(notice.message || group.code.replaceAll('_', ' '))}</p>
                    ${locationHtml}
                    ${fix}
                </li>
            `;
        }).join('');

        mcpPreviewBody.innerHTML = `
            <p class="mcp-verdict">${escapeHtml(verdict)}</p>
            ${issuesHtml
                ? `<ol class="mcp-examples">${issuesHtml}</ol>
                   <p class="mcp-sample-note">Compact preview: MCP can return up to three examples for every issue type.</p>`
                : '<p class="mcp-clean-note"><i data-lucide="circle-check"></i> There are no error or warning examples to send.</p>'}
        `;
        mcpPreview.classList.remove('is-ready');
        requestAnimationFrame(() => mcpPreview.classList.add('is-ready'));
    }

    function showResults(result) {
        processingState.classList.add('hidden');
        resultState.classList.remove('hidden');

        const errors = result.error_count;
        const warnings = result.warning_count;

        errorCountEl.innerText = errors;
        warningCountEl.innerText = warnings;
        renderMcpPreview(result);

        // A shared report has no feed behind it, so there is no HTML report to
        // hand out — hide the button rather than let it fail on click.
        const htmlAvailable = typeof result.html === 'string' && result.html.length > 0;
        document.querySelectorAll('#download-html-btn, #download-html-modal-btn').forEach((btn) => {
            btn.classList.toggle('hidden', !htmlAvailable);
        });

        // Re-create icons for new buttons
        if (typeof lucide !== 'undefined') {
            lucide.createIcons();
        }

        console.log("Validation Result:", result);
    }

    function showReportModal(result) {
        if (!reportModal || !reportModalBody) return;

        let notices = [];
        try {
            notices = JSON.parse(result.json);
        } catch (e) {
            console.error('Failed to parse notices:', e);
        }

        // Group by severity
        const groups = {
            error: notices.filter(n => n.severity === 'ERROR'),
            warning: notices.filter(n => n.severity === 'WARNING'),
            info: notices.filter(n => n.severity === 'INFO')
        };

        let html = '';

        if (notices.length === 0) {
            html = `
                <div class="empty-state">
                    <i data-lucide="check-circle"></i>
                    <h4>Perfect!</h4>
                    <p>No issues found in your GTFS feed.</p>
                </div>
            `;
        } else {
            html += renderTruncationNote(result, notices.length);
            // Render each group
            if (groups.error.length > 0) {
                html += renderNoticeGroup('Errors', 'error', groups.error, result.error_count, result.totalsByCode);
            }
            if (groups.warning.length > 0) {
                html += renderNoticeGroup('Warnings', 'warning', groups.warning, result.warning_count, result.totalsByCode);
            }
            if (groups.info.length > 0) {
                html += renderNoticeGroup('Info', 'info', groups.info, result.info_count, result.totalsByCode);
            }
        }

        reportModalBody.innerHTML = html;
        reportModal.classList.remove('hidden');

        // Add event listeners for accordion toggles
        const headers = reportModalBody.querySelectorAll('.notice-group-header');
        headers.forEach(header => {
            header.addEventListener('click', () => {
                const isDetails = header.getAttribute('data-type') === 'details';
                if (isDetails) {
                    // Toggle this specific details group
                    const details = header.nextElementSibling;
                    const icon = header.querySelector('.toggle-icon');

                    if (details.classList.contains('open')) {
                        details.classList.remove('open');
                        header.classList.remove('active');
                        if (icon) icon.style.transform = 'rotate(0deg)';
                    } else {
                        details.classList.add('open');
                        header.classList.add('active');
                        if (icon) icon.style.transform = 'rotate(180deg)';
                    }
                }
            });
        });

        reportModalBody.querySelectorAll('.geometry-map-btn').forEach(btn => {
            btn.addEventListener('click', (e) => {
                e.stopPropagation();
                try {
                    openNoticeMap(
                        JSON.parse(btn.getAttribute('data-geometry')),
                        btn.getAttribute('data-title')
                    );
                } catch (err) {
                    console.error('Invalid notice geometry:', err);
                }
            });
        });

        // Stop references without notice geometry still use the stops.txt index.
        reportModalBody.querySelectorAll('.map-pin-btn').forEach(btn => {
            btn.addEventListener('click', (e) => {
                e.stopPropagation();
                openStopMap(btn.getAttribute('data-stop-id'), btn.getAttribute('data-code'));
            });
        });

        // Initialize lucide icons in modal
        if (typeof lucide !== 'undefined') {
            lucide.createIcons();
        }
    }

    /* --- Geographic notice map (MapLibre, lazy-loaded) --- */
    let mapLibreLoading = null;
    let noticeMap = null;
    let noticeMapReady = false;
    let pendingNoticeGeometry = null;
    let noticeMapMarkers = [];

    function ensureMapLibre() {
        if (window.maplibregl) return Promise.resolve();
        if (mapLibreLoading) return mapLibreLoading;
        mapLibreLoading = new Promise((resolve, reject) => {
            const css = document.createElement('link');
            css.rel = 'stylesheet';
            css.href = 'https://unpkg.com/maplibre-gl@5.12.0/dist/maplibre-gl.css';
            document.head.appendChild(css);
            const script = document.createElement('script');
            script.src = 'https://unpkg.com/maplibre-gl@5.12.0/dist/maplibre-gl.js';
            script.onload = resolve;
            script.onerror = () => {
                mapLibreLoading = null;
                reject(new Error('Failed to load MapLibre'));
            };
            document.head.appendChild(script);
        });
        return mapLibreLoading;
    }

    function emptyGeoJson() {
        return { type: 'FeatureCollection', features: [] };
    }

    function mapCoordinate(point) {
        if (!point || !Number.isFinite(point.latitude) || !Number.isFinite(point.longitude)) {
            return null;
        }
        return [point.longitude, point.latitude];
    }

    function lineGeoJson(points) {
        const coordinates = (points || []).map(mapCoordinate).filter(Boolean);
        if (coordinates.length < 2) return emptyGeoJson();
        return {
            type: 'FeatureCollection',
            features: [{
                type: 'Feature',
                properties: {},
                geometry: { type: 'LineString', coordinates }
            }]
        };
    }

    function setNoticeMapData(sourceId, data) {
        const source = noticeMap && noticeMap.getSource(sourceId);
        if (source) source.setData(data);
    }

    function clearNoticeMapMarkers() {
        noticeMapMarkers.forEach(marker => marker.remove());
        noticeMapMarkers = [];
    }

    function addNoticeMapMarker(point, kind, label) {
        const position = mapCoordinate(point);
        if (!position) return;
        const element = document.createElement('div');
        element.className = `notice-map-marker notice-map-marker--${kind}`;
        element.setAttribute('aria-label', label);
        const popupText = document.createElement('strong');
        popupText.textContent = label;
        const popup = new maplibregl.Popup({ offset: 18, closeButton: false })
            .setDOMContent(popupText);
        noticeMapMarkers.push(
            new maplibregl.Marker({ element, anchor: 'center' })
                .setLngLat(position)
                .setPopup(popup)
                .addTo(noticeMap)
        );
    }

    function renderNoticeMapGeometry(geometry) {
        if (!noticeMapReady || !geometry) {
            pendingNoticeGeometry = geometry;
            return;
        }

        pendingNoticeGeometry = null;
        clearNoticeMapMarkers();
        setNoticeMapData('notice-shape', emptyGeoJson());
        setNoticeMapData('notice-connector', emptyGeoJson());
        setNoticeMapData('notice-bounds', emptyGeoJson());

        let positions = [];
        if (geometry.type === 'point') {
            addNoticeMapMarker(geometry.point, 'point', 'Affected location');
            const point = mapCoordinate(geometry.point);
            if (point) positions.push(point);
        } else if (geometry.type === 'line') {
            setNoticeMapData('notice-shape', lineGeoJson(geometry.points));
            positions = (geometry.points || []).map(mapCoordinate).filter(Boolean);
        } else if (geometry.type === 'pointAndLine') {
            setNoticeMapData('notice-shape', lineGeoJson(geometry.line));
            addNoticeMapMarker(geometry.point, 'point', 'Affected stop');
            addNoticeMapMarker(geometry.nearestPoint, 'nearest', 'Closest point on shape');
            const point = mapCoordinate(geometry.point);
            const nearest = mapCoordinate(geometry.nearestPoint);
            if (point && nearest) {
                setNoticeMapData(
                    'notice-connector',
                    lineGeoJson([geometry.point, geometry.nearestPoint])
                );
            }
            positions = (geometry.line || []).map(mapCoordinate).filter(Boolean);
            if (point) positions.push(point);
            if (nearest) positions.push(nearest);
        } else if (geometry.type === 'boundingBox') {
            const southWest = mapCoordinate(geometry.southWest);
            const northEast = mapCoordinate(geometry.northEast);
            if (southWest && northEast) {
                const northWest = [southWest[0], northEast[1]];
                const southEast = [northEast[0], southWest[1]];
                setNoticeMapData('notice-bounds', {
                    type: 'FeatureCollection',
                    features: [{
                        type: 'Feature',
                        properties: {},
                        geometry: {
                            type: 'Polygon',
                            coordinates: [[southWest, northWest, northEast, southEast, southWest]]
                        }
                    }]
                });
                positions = [southWest, northEast];
            }
        }

        noticeMap.resize();
        if (positions.length === 1) {
            noticeMap.flyTo({ center: positions[0], zoom: 16, duration: 650 });
        } else if (positions.length > 1) {
            const bounds = positions.reduce(
                (result, point) => result.extend(point),
                new maplibregl.LngLatBounds(positions[0], positions[0])
            );
            noticeMap.fitBounds(bounds, { padding: 72, maxZoom: 17, duration: 700 });
        }
    }

    function createNoticeMap() {
        if (noticeMap) return;
        noticeMap = new maplibregl.Map({
            container: 'stop-map',
            center: [0, 20],
            zoom: 1.5,
            attributionControl: false,
            style: {
                version: 8,
                sources: {
                    basemap: {
                        type: 'raster',
                        tiles: ['https://a.basemaps.cartocdn.com/dark_all/{z}/{x}/{y}.png'],
                        tileSize: 256,
                        attribution: '&copy; OpenStreetMap contributors &copy; CARTO'
                    }
                },
                layers: [{ id: 'basemap', type: 'raster', source: 'basemap' }]
            }
        });
        noticeMap.addControl(new maplibregl.NavigationControl({ showCompass: false }), 'top-right');
        noticeMap.addControl(new maplibregl.AttributionControl({ compact: true }), 'bottom-right');
        noticeMap.on('load', () => {
            noticeMap.addSource('notice-shape', { type: 'geojson', data: emptyGeoJson() });
            noticeMap.addSource('notice-connector', { type: 'geojson', data: emptyGeoJson() });
            noticeMap.addSource('notice-bounds', { type: 'geojson', data: emptyGeoJson() });
            noticeMap.addLayer({
                id: 'notice-bounds-fill',
                type: 'fill',
                source: 'notice-bounds',
                paint: { 'fill-color': '#7c3aed', 'fill-opacity': 0.2 }
            });
            noticeMap.addLayer({
                id: 'notice-bounds-line',
                type: 'line',
                source: 'notice-bounds',
                paint: { 'line-color': '#a78bfa', 'line-width': 3 }
            });
            noticeMap.addLayer({
                id: 'notice-shape',
                type: 'line',
                source: 'notice-shape',
                layout: { 'line-cap': 'round', 'line-join': 'round' },
                paint: { 'line-color': '#22d3ee', 'line-width': 5, 'line-opacity': 0.95 }
            });
            noticeMap.addLayer({
                id: 'notice-connector',
                type: 'line',
                source: 'notice-connector',
                paint: {
                    'line-color': '#fb7185',
                    'line-width': 3,
                    'line-dasharray': [1.5, 1.5]
                }
            });
            noticeMapReady = true;
            if (pendingNoticeGeometry) renderNoticeMapGeometry(pendingNoticeGeometry);
        });
    }

    async function openNoticeMap(geometry, title) {
        const mapModal = document.getElementById('map-modal');
        const mapTitle = document.getElementById('map-modal-title');
        if (!mapModal || !geometry) return;
        if (mapTitle) mapTitle.textContent = title || 'Geographic notice';
        mapModal.classList.remove('hidden');
        try {
            await ensureMapLibre();
            createNoticeMap();
            renderNoticeMapGeometry(geometry);
            setTimeout(() => noticeMap.resize(), 50);
        } catch (err) {
            console.error(err);
            if (mapTitle) mapTitle.textContent = 'Map could not be loaded';
        }
    }

    function openStopMap(stopId, code) {
        const info = stopsIndex && stopsIndex.get(stopId);
        if (!info) return;
        const title = info.name ? `${info.name} (${stopId})` : stopId;
        openNoticeMap(
            {
                type: 'point',
                point: { latitude: info.lat, longitude: info.lon }
            },
            code ? `${title} · ${code}` : title
        );
    }

    function closeStopMap() {
        const mapModal = document.getElementById('map-modal');
        if (mapModal) mapModal.classList.add('hidden');
    }

    const closeMapBtn = document.getElementById('close-map-btn');
    if (closeMapBtn) closeMapBtn.addEventListener('click', closeStopMap);
    const mapModalEl = document.getElementById('map-modal');
    if (mapModalEl) {
        mapModalEl.addEventListener('click', (e) => {
            if (e.target === mapModalEl) closeStopMap();
        });
    }

    // Very large reports are capped in the validator: it stores the first
    // 10,000 notices per issue type and keeps exact counters for the rest,
    // so the tab doesn't run out of memory. Tell the user when that happened.
    function renderTruncationNote(result, shownCount) {
        const totalCount = (result.error_count || 0) + (result.warning_count || 0) + (result.info_count || 0);
        const isTruncated = result.truncated === true || shownCount < totalCount;
        if (!isTruncated) return '';
        const reason = result.shared
            ? 'A shared link carries a sample of each issue type so it stays short enough to send'
            : 'Long issue lists are capped per issue type to keep the browser responsive';
        return `
            <div style="margin-bottom: 1rem; padding: 0.75rem 1rem; border: 1px solid var(--border, #d0d7de); border-radius: 8px; font-size: 0.9rem; opacity: 0.9;">
                Showing ${shownCount.toLocaleString()} of ${totalCount.toLocaleString()} notices.
                ${reason} — the summary counts are exact.
            </div>
        `;
    }

    // `totalsByCode` carries the exact per-issue-type counts when the notice
    // list itself has been sampled (a shared link), so the group headers keep
    // showing what the feed really contains.
    function renderNoticeGroup(title, severity, notices, totalOverride, totalsByCode) {
        // First, group notices by CODE
        const noticesByCode = Object.create(null);
        notices.forEach(notice => {
            if (!noticesByCode[notice.code]) {
                noticesByCode[notice.code] = [];
            }
            noticesByCode[notice.code].push(notice);
        });

        const sortedCodes = Object.keys(noticesByCode).sort((a, b) => {
            return noticesByCode[b].length - noticesByCode[a].length;
        });

        let sectionsHtml = '';

        sortedCodes.forEach(code => {
            const codeNotices = noticesByCode[code];
            const count = totalsByCode?.[code] ?? codeNotices.length;
            const sample = codeNotices[0];

            // Prepare flattened data for display
            // We'll process only the first 50 displayed
            const displayNotices = codeNotices.slice(0, 50).map(n => {
                const flat = Object.assign(Object.create(null), n);
                // Flatten context if present (and handle [object Object] issue)
                if (flat.context && typeof flat.context === 'object') {
                    Object.assign(flat, flat.context);
                    delete flat.context;
                }
                return flat;
            });

            // Extract dynamic keys for table headers (exclude standard and internal ones)
            const excludeKeys = [
                'message',
                'code',
                'severity',
                'totalNotices',
                'field_order',
                'context',
                'geometry'
            ];
            const allKeys = new Set();
            displayNotices.forEach(n => {
                Object.keys(n).forEach(k => {
                    if (!excludeKeys.includes(k) && n[k] !== null && n[k] !== undefined && n[k] !== "") {
                        allKeys.add(k);
                    }
                });
            });

            // Sort keys: csvRowNumber first, then file/row/field, then others alpha
            const headers = Array.from(allKeys).sort((a, b) => {
                const priority = ['csvRowNumber', 'file', 'row', 'field', 'stopId', 'routeId', 'tripId'];
                const idxA = priority.indexOf(a);
                const idxB = priority.indexOf(b);
                if (idxA !== -1 && idxB !== -1) return idxA - idxB;
                if (idxA !== -1) return -1;
                if (idxB !== -1) return 1;
                return a.localeCompare(b);
            });

            // Generate table headers
            const thHtml = headers.map(h => `<th>${escapeHtml(h)}</th>`).join('');
            const hasNoticeGeometry = displayNotices.some(notice => notice.geometry);
            const mapHeaderHtml = hasNoticeGeometry ? '<th>Map</th>' : '';

            // Generate table rows
            const stopKeyRe = /^(stopId\d*|childStopId|parentStopId|parentStation|locationId)$/;
            const rowsHtml = displayNotices.map(notice => {
                const tdHtml = headers.map(h => {
                    let val = notice[h];
                    // Handle objects that might still remain (e.g. nested objects)
                    let valStr = '';
                    if (val === null || val === undefined) {
                        valStr = '';
                    } else if (typeof val === 'object') {
                        valStr = JSON.stringify(val);
                    } else {
                        valStr = String(val);
                    }
                    // Stop references become clickable pins that open the map.
                    if (stopsIndex && stopKeyRe.test(h) && stopsIndex.has(valStr)) {
                        return `<td><code>${escapeHtml(valStr)}</code><button class="map-pin-btn" data-stop-id="${escapeAttr(valStr)}" data-code="${escapeAttr(code)}" title="Show on map"><i data-lucide="map-pin"></i></button></td>`;
                    }
                    return `<td><code>${escapeHtml(valStr)}</code></td>`;
                }).join('');
                const geometryTitle = notice.stopName
                    ? `${notice.stopName} · ${code}`
                    : code;
                const mapCellHtml = hasNoticeGeometry
                    ? (notice.geometry
                        ? `<td><button class="geometry-map-btn" data-geometry="${escapeAttr(JSON.stringify(notice.geometry))}" data-title="${escapeAttr(geometryTitle)}" title="View affected geometry"><i data-lucide="map"></i><span>View</span></button></td>`
                        : '<td></td>')
                    : '';
                return `<tr>${tdHtml}${mapCellHtml}</tr>`;
            }).join('');

            const moreCount = Math.max(0, count - displayNotices.length);
            const moreNote = moreCount > 0 ?
                `<div style="text-align: center; padding: 0.5rem; color: var(--text-secondary); font-size: 0.85rem; border-top: 1px solid var(--border);">
                    + ${escapeHtml(moreCount)} more records (download full report to see all)
                 </div>` : '';

            sectionsHtml += `
                <div class="notice-group">
                    <div class="notice-group-header ${severity}" data-type="details">
                        <div class="notice-group-title">
                            <i data-lucide="chevron-right" class="toggle-icon"></i>
                            <span style="font-family: 'Fira Code', monospace; font-size: 0.95rem;">${escapeHtml(code)}</span>
                        </div>
                        <span class="notice-group-count">${escapeHtml(count)}</span>
                    </div>
                    <div class="notice-group-details" style="padding: 0;">
                        <div style="padding: 1rem; background: rgba(0,0,0,0.2); border-bottom: 1px solid var(--border);">
                             <div style="margin-bottom: 0.5rem; color: var(--text-primary); font-size: 0.95rem;">
                                ${escapeHtml(sample.message)}
                             </div>
                        </div>
                        <div style="overflow-x: auto;">
                            <table class="report-table" style="width: 100%; border-collapse: collapse; font-size: 0.85rem;">
                                <thead>
                                    <tr style="text-align: left; background: rgba(255,255,255,0.05); color: var(--text-secondary);">
                                        ${thHtml}
                                        ${mapHeaderHtml}
                                    </tr>
                                </thead>
                                <tbody>
                                    ${rowsHtml}
                                </tbody>
                            </table>
                        </div>
                        ${moreNote}
                    </div>
                </div>
            `;
        });

        return `
            <div style="margin-bottom: 2rem;">
                <h3 style="margin-bottom: 1rem; color: var(--${severity}); display: flex; align-items: center; gap: 0.5rem;">
                    ${title} <span style="background: rgba(255,255,255,0.1); padding: 0.1rem 0.6rem; border-radius: 20px; font-size: 0.8rem;">${typeof totalOverride === 'number' ? totalOverride : notices.length}</span>
                </h3>
                ${sectionsHtml}
            </div>
        `;
    }

    function escapeHtml(text) {
        if (!text) return '';
        const div = document.createElement('div');
        div.textContent = text;
        return div.innerHTML;
    }

    function escapeAttr(text) {
        return String(text ?? '')
            .replace(/&/g, '&amp;')
            .replace(/"/g, '&quot;')
            .replace(/</g, '&lt;')
            .replace(/>/g, '&gt;');
    }

    function closeReportModal() {
        if (reportModal) {
            reportModal.classList.add('hidden');
        }
    }

    function downloadValidationHTML() {
        if (!lastValidationResult || !lastValidationResult.html) return;

        const blob = new Blob([lastValidationResult.html], { type: 'text/html' });
        const url = URL.createObjectURL(blob);
        const a = document.createElement('a');
        a.href = url;
        a.download = `${lastFileName}_validation_report.html`;
        document.body.appendChild(a);
        a.click();
        document.body.removeChild(a);
        URL.revokeObjectURL(url);
    }



    function downloadValidationJSON() {
        if (!lastValidationResult) return;

        const blob = new Blob([lastValidationResult.json], { type: 'application/json' });
        const url = URL.createObjectURL(blob);
        const a = document.createElement('a');
        a.href = url;
        a.download = `${lastFileName}_validation_report.json`;
        document.body.appendChild(a);
        a.click();
        document.body.removeChild(a);
        URL.revokeObjectURL(url);
    }

    function openReportWindow(result) {
        let notices = [];
        try {
            notices = JSON.parse(result.json);
        } catch (e) {
            console.error('Failed to parse notices:', e);
            return;
        }

        const groups = {
            error: notices.filter(n => n.severity === 'ERROR'),
            warning: notices.filter(n => n.severity === 'WARNING'),
            info: notices.filter(n => n.severity === 'INFO')
        };

        const win = window.open('', '_blank');
        if (!win) {
            alert('Pop-up blocked. Please allow pop-ups for this site.');
            return;
        }

        let reportContent = '';

        // Prefer the validator's exact counters: the notice list itself may be
        // capped per issue type for very large reports.
        let headerTotals = [];
        const errorTotal = typeof result.error_count === 'number' ? result.error_count : groups.error.length;
        const warningTotal = typeof result.warning_count === 'number' ? result.warning_count : groups.warning.length;
        const infoTotal = typeof result.info_count === 'number' ? result.info_count : groups.info.length;
        if (errorTotal > 0) headerTotals.push(`${errorTotal} Errors`);
        if (warningTotal > 0) headerTotals.push(`${warningTotal} Warnings`);
        if (infoTotal > 0) headerTotals.push(`${infoTotal} Info`);

        const summaryText = headerTotals.join(', ') || 'No issues found';

        if (notices.length === 0) {
            reportContent = `
                <div class="empty-state">
                    <div style="font-size: 48px; color: var(--success); margin-bottom: 1rem;">✓</div>
                    <h4>Perfect!</h4>
                    <p>No issues found in your GTFS feed.</p>
                </div>
            `;
        } else {
            reportContent += renderTruncationNote(result, notices.length);
            if (groups.error.length > 0) {
                reportContent += renderNoticeGroup('Errors', 'error', groups.error, result.error_count, result.totalsByCode);
            }
            if (groups.warning.length > 0) {
                reportContent += renderNoticeGroup('Warnings', 'warning', groups.warning, result.warning_count, result.totalsByCode);
            }
            if (groups.info.length > 0) {
                reportContent += renderNoticeGroup('Info', 'info', groups.info, result.info_count, result.totalsByCode);
            }
        }

        const html = `
<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Validation Report - GTFS.guru</title>
    <link rel="stylesheet" href="style.css">
    <link href="https://fonts.googleapis.com/css2?family=Outfit:wght@400;500;600;700&family=Inter:wght@300;400;500;600&family=Fira+Code:wght@400;500&display=swap" rel="stylesheet">
    <style>
        body {
            background-color: var(--bg-dark);
            color: var(--text-primary);
            padding: 2rem;
            max-width: 1200px;
            margin: 0 auto;
        }
        .report-header-window {
            margin-bottom: 2rem;
            border-bottom: 1px solid rgba(255,255,255,0.1);
            padding-bottom: 1rem;
            display: flex;
            justify-content: space-between;
            align-items: center;
        }
        .file-name {
            color: var(--text-secondary);
            font-size: 0.9rem;
        }
    </style>
</head>
<body>
    <div class="report-header-window">
        <div>
            <h1>Validation Report</h1>
            <div class="file-name">File: ${escapeHtml(lastFileName)}.zip</div>
        </div>
        <div style="text-align: right;">
            <div style="font-size: 1.2rem; font-weight: bold;">${summaryText}</div>
            <div style="font-size: 0.9rem; color: var(--text-secondary);">${new Date().toLocaleString()}</div>
        </div>
    </div>

    <div class="report-body">
        ${reportContent}
    </div>

    <script>
        // Auto-scroll logic if needed
    </script>
</body>
</html>
        `;

        win.document.open();
        win.document.write(html);
        win.document.close();
    }
});
