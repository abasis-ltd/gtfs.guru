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

// Keep this in sync with MAX_FILE_SIZE_BYTES in crates/gtfs_validator_wasm/src/lib.rs.
const MAX_FILE_SIZE_BYTES = 100 * 1024 * 1024;

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
let mainThreadValidatePromise = null;
function getMainThreadValidate() {
    if (!mainThreadValidatePromise) {
        mainThreadValidatePromise = import('./pkg/gtfs_guru_wasm.js').then(async (mod) => {
            await mod.default(); // init()
            return mod.validate_gtfs;
        });
    }
    return mainThreadValidatePromise;
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

// Minimal zip central-directory reader. Feeds are capped at 100 MB, so no
// zip64 handling is needed; anything unusual just means "no map", never an error.
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
        // Transfer (not copy) the ArrayBuffer — feeds can be up to 100 MB.
        validatorWorker.postMessage(
            { type: 'validate', id, payload: { zipBytes: arrayBuffer, date: dateStr } },
            [arrayBuffer],
        );
    });
}

async function validateOnMainThread(arrayBuffer, dateStr) {
    const validate_gtfs = await getMainThreadValidate();
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
    };
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
    // Enable fade-up animations by removing the no-js class
    document.documentElement.classList.remove('no-js');

    /* --- Intersection Observer for Fade-Up Animations --- */
    const observerOptions = {
        threshold: 0.1,
        rootMargin: "0px 0px -50px 0px"
    };

    const observer = new IntersectionObserver((entries) => {
        entries.forEach(entry => {
            if (entry.isIntersecting) {
                entry.target.classList.add('visible');

                // Trigger counters if this is the hero stats section
                if (entry.target.querySelector('.stat-number')) {
                    startCounters(entry.target);
                }

                observer.unobserve(entry.target);
            }
        });
    }, observerOptions);

    document.querySelectorAll('.fade-up').forEach(el => {
        observer.observe(el);
    });

    /* --- Number Counters --- */
    function startCounters(container) {
        container.querySelectorAll('.stat-number').forEach(counter => {
            const target = +counter.getAttribute('data-target');
            const duration = 2000; // ms
            const increment = target / (duration / 16); // 60fps

            let current = 0;
            const updateCounter = () => {
                current += increment;
                if (current < target) {
                    counter.innerText = Math.ceil(current);
                    requestAnimationFrame(updateCounter);
                } else {
                    counter.innerText = target;
                }
            };
            updateCounter();
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
                    behavior: "smooth"
                });
            }
        });
    });

    /* --- Validator Logic --- */
    const dropZone = document.getElementById('drop-zone');
    const fileInput = document.getElementById('file-input');
    const uploadState = document.getElementById('upload-state');
    const processingState = document.getElementById('processing-state');
    const resultState = document.getElementById('result-state');
    const resetBtn = document.getElementById('reset-btn');
    const urlInput = document.getElementById('url-input');
    const urlAnalyzeBtn = document.getElementById('url-analyze-btn');
    const urlInputContainer = document.getElementById('url-input-container');

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
                handleFile(e.dataTransfer.files[0]);
            }
        });

        // Click to browse
        uploadState.addEventListener('click', () => fileInput.click());

        fileInput.addEventListener('change', (e) => {
            if (e.target.files.length) {
                handleFile(e.target.files[0]);
            }
        });

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

        // Reset
        if (resetBtn) {
            resetBtn.addEventListener('click', () => {
                resultState.classList.add('hidden');
                uploadState.classList.remove('hidden');
                urlInputContainer.classList.remove('hidden');
                fileInput.value = '';
                urlInput.value = '';
                lastValidationResult = null;
            });
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

        lastFileName = file.name.replace('.zip', '');

        // Show processing
        const errorContainer = document.getElementById('error-container');
        if (errorContainer) errorContainer.classList.add('hidden');

        uploadState.classList.add('hidden');
        urlInputContainer.classList.add('hidden');
        processingState.classList.remove('hidden');

        try {
            const arrayBuffer = await file.arrayBuffer();
            await runValidation(arrayBuffer);
        } catch (err) {
            console.error("File reading error:", err);
            processingState.classList.add('hidden');
            uploadState.classList.remove('hidden');
            urlInputContainer.classList.remove('hidden');
        }
    }

    async function handleUrl(url) {
        // Show processing
        const errorContainer = document.getElementById('error-container');
        if (errorContainer) errorContainer.classList.add('hidden');

        uploadState.classList.add('hidden');
        urlInputContainer.classList.add('hidden');
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
                console.warn("Direct fetch failed, trying local CORS proxy...", err);
                // Try via local proxy on the same server
                // This requires Nginx to be configured with the /cors-proxy/ location
                const proxyUrl = '/cors-proxy/' + url;
                arrayBuffer = await tryFetch(proxyUrl);
            }

            if (arrayBuffer.byteLength > MAX_FILE_SIZE_BYTES) {
                showTooLarge(arrayBuffer.byteLength);
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
If the server blocks cross-origin requests (CORS), we tried to use a local proxy but that failed too.
Please ensure the Nginx container is configured with the proxy settings.`);
            processingState.classList.add('hidden');
            uploadState.classList.remove('hidden');
            urlInputContainer.classList.remove('hidden');
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
        uploadState.classList.remove('hidden');
        urlInputContainer.classList.remove('hidden');
    }

    function showTooLarge(sizeBytes) {
        const sizeMb = (sizeBytes / (1024 * 1024)).toFixed(1);
        showValidationError(
            `This feed is ${sizeMb} MB. In-browser validation is capped at 100 MB. ` +
            `For larger feeds use the free desktop app or CLI (see the Download section) — they handle any size.`
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

    function showResults(result) {
        processingState.classList.add('hidden');
        resultState.classList.remove('hidden');

        const errors = result.error_count;
        const warnings = result.warning_count;

        errorCountEl.innerText = errors;
        warningCountEl.innerText = warnings;

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
            // Render each group
            if (groups.error.length > 0) {
                html += renderNoticeGroup('Errors', 'error', groups.error);
            }
            if (groups.warning.length > 0) {
                html += renderNoticeGroup('Warnings', 'warning', groups.warning);
            }
            if (groups.info.length > 0) {
                html += renderNoticeGroup('Info', 'info', groups.info);
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

        // Map pins: open the stop location map
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

    /* --- Stop location map (Leaflet, lazy-loaded) --- */
    let leafletLoading = null;
    let stopMap = null;
    let stopMapMarkers = null;

    function ensureLeaflet() {
        if (window.L) return Promise.resolve();
        if (leafletLoading) return leafletLoading;
        leafletLoading = new Promise((resolve, reject) => {
            const css = document.createElement('link');
            css.rel = 'stylesheet';
            css.href = 'https://unpkg.com/leaflet@1.9.4/dist/leaflet.css';
            document.head.appendChild(css);
            const script = document.createElement('script');
            script.src = 'https://unpkg.com/leaflet@1.9.4/dist/leaflet.js';
            script.onload = resolve;
            script.onerror = () => { leafletLoading = null; reject(new Error('Failed to load the map library')); };
            document.head.appendChild(script);
        });
        return leafletLoading;
    }

    async function openStopMap(stopId, code) {
        const info = stopsIndex && stopsIndex.get(stopId);
        if (!info) return;
        try {
            await ensureLeaflet();
        } catch (err) {
            console.error(err);
            return;
        }

        const mapModal = document.getElementById('map-modal');
        const mapTitle = document.getElementById('map-modal-title');
        if (!mapModal) return;

        if (mapTitle) {
            mapTitle.textContent = info.name ? `${info.name} (${stopId})` : stopId;
        }
        mapModal.classList.remove('hidden');

        if (!stopMap) {
            stopMap = L.map('stop-map');
            L.tileLayer('https://tile.openstreetmap.org/{z}/{x}/{y}.png', {
                maxZoom: 19,
                attribution: '&copy; <a href="https://www.openstreetmap.org/copyright">OpenStreetMap</a> contributors'
            }).addTo(stopMap);
            stopMapMarkers = L.layerGroup().addTo(stopMap);
        }

        stopMapMarkers.clearLayers();
        const marker = L.marker([info.lat, info.lon]).addTo(stopMapMarkers);
        const popupHtml =
            `<b>${escapeHtml(info.name || stopId)}</b><br>` +
            `<code>${escapeHtml(stopId)}</code>` +
            (code ? `<br><code>${escapeHtml(code)}</code>` : '');
        marker.bindPopup(popupHtml);

        stopMap.setView([info.lat, info.lon], 16);
        // The map container was hidden when Leaflet measured it.
        setTimeout(() => {
            stopMap.invalidateSize();
            stopMap.setView([info.lat, info.lon], 16);
            marker.openPopup();
        }, 50);
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

    function renderNoticeGroup(title, severity, notices) {
        // First, group notices by CODE
        const noticesByCode = {};
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
            const count = codeNotices.length;
            const sample = codeNotices[0];

            // Prepare flattened data for display
            // We'll process only the first 50 displayed
            const displayNotices = codeNotices.slice(0, 50).map(n => {
                const flat = { ...n };
                // Flatten context if present (and handle [object Object] issue)
                if (flat.context && typeof flat.context === 'object') {
                    Object.assign(flat, flat.context);
                    delete flat.context;
                }
                return flat;
            });

            // Extract dynamic keys for table headers (exclude standard and internal ones)
            const excludeKeys = ['message', 'code', 'severity', 'totalNotices', 'field_order', 'context'];
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
                return `<tr>${tdHtml}</tr>`;
            }).join('');

            const moreCount = count > 50 ? count - 50 : 0;
            const moreNote = moreCount > 0 ?
                `<div style="text-align: center; padding: 0.5rem; color: var(--text-secondary); font-size: 0.85rem; border-top: 1px solid var(--border);">
                    + ${moreCount} more records (download full report to see all)
                 </div>` : '';

            sectionsHtml += `
                <div class="notice-group">
                    <div class="notice-group-header ${severity}" data-type="details">
                        <div class="notice-group-title">
                            <i data-lucide="chevron-right" class="toggle-icon"></i>
                            <span style="font-family: 'Fira Code', monospace; font-size: 0.95rem;">${escapeHtml(code)}</span>
                        </div>
                        <span class="notice-group-count">${count}</span>
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
                    ${title} <span style="background: rgba(255,255,255,0.1); padding: 0.1rem 0.6rem; border-radius: 20px; font-size: 0.8rem;">${notices.length}</span>
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
        if (!lastValidationResult) return;

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

        let headerTotals = [];
        if (groups.error.length > 0) headerTotals.push(`${groups.error.length} Errors`);
        if (groups.warning.length > 0) headerTotals.push(`${groups.warning.length} Warnings`);
        if (groups.info.length > 0) headerTotals.push(`${groups.info.length} Info`);

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
            if (groups.error.length > 0) {
                reportContent += renderNoticeGroup('Errors', 'error', groups.error);
            }
            if (groups.warning.length > 0) {
                reportContent += renderNoticeGroup('Warnings', 'warning', groups.warning);
            }
            if (groups.info.length > 0) {
                reportContent += renderNoticeGroup('Info', 'info', groups.info);
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
            <div class="file-name">File: ${lastFileName}.zip</div>
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

