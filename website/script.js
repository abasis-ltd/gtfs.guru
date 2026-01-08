import init, { validate_gtfs } from './pkg/gtfs_guru_wasm.js';

// Initialize WASM
init().catch(console.error);

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
        if (downloadJsonModalBtn) {
            downloadJsonModalBtn.addEventListener('click', downloadValidationJSON);
        }

        // Open in new window
        if (openWindowBtn) {
            openWindowBtn.addEventListener('click', () => {
                if (lastValidationResult) {
                    openReportWindow(lastValidationResult);
                }
            });
        }

        // Close modal on Escape
        document.addEventListener('keydown', (e) => {
            if (e.key === 'Escape' && reportModal && !reportModal.classList.contains('hidden')) {
                closeReportModal();
            }
        });
    }

    async function handleFile(file) {
        if (!file.name.endsWith('.zip')) {
            alert('Please upload a ZIP file.');
            return;
        }

        lastFileName = file.name.replace('.zip', '');

        // Show processing
        uploadState.classList.add('hidden');
        urlInputContainer.classList.add('hidden');
        processingState.classList.remove('hidden');

        try {
            const arrayBuffer = await file.arrayBuffer();
            const bytes = new Uint8Array(arrayBuffer);
            await runValidation(bytes);
        } catch (err) {
            console.error("File reading error:", err);
            processingState.classList.add('hidden');
            uploadState.classList.remove('hidden');
            urlInputContainer.classList.remove('hidden');
        }
    }

    async function handleUrl(url) {
        // Show processing
        uploadState.classList.add('hidden');
        urlInputContainer.classList.add('hidden');
        processingState.classList.remove('hidden');

        try {
            const response = await fetch(url);
            if (!response.ok) {
                throw new Error(`Failed to fetch: ${response.statusText}`);
            }

            const arrayBuffer = await response.arrayBuffer();
            const bytes = new Uint8Array(arrayBuffer);

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

            await runValidation(bytes);
        } catch (err) {
            console.error("URL fetch error:", err);
            alert(`Error loading from URL: ${err.message}\nNote: Remote servers must allow CORS.`);
            processingState.classList.add('hidden');
            uploadState.classList.remove('hidden');
            urlInputContainer.classList.remove('hidden');
        }
    }

    async function runValidation(bytes) {
        // Run WASM validation (give UI a moment to update)
        return new Promise((resolve) => {
            setTimeout(() => {
                try {
                    const result = validate_gtfs(bytes, null);
                    lastValidationResult = result;
                    showResults(result);
                    resolve();
                } catch (err) {
                    console.error("Validation error:", err);
                    alert("Error processing file. See console for details.");
                    processingState.classList.add('hidden');
                    uploadState.classList.remove('hidden');
                    urlInputContainer.classList.remove('hidden');
                    resolve();
                }
            }, 100);
        });
    }

    function showResults(result) {
        processingState.classList.add('hidden');
        resultState.classList.remove('hidden');

        const errors = result.error_count;
        const warnings = result.warning_count;

        errorCountEl.innerText = errors;
        warningCountEl.innerText = warnings;

        // Simple scoring
        let score = Math.max(0, 100 - (errors * 20));
        if (errors > 0 && score === 100) score = 90;

        scoreNumberEl.innerText = score;

        // Color ring
        if (errors > 0) {
            scoreRingEl.style.borderColor = 'var(--error)';
        } else if (warnings > 0) {
            scoreRingEl.style.borderColor = 'var(--warning)';
        } else {
            scoreRingEl.style.borderColor = 'var(--success)';
        }

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
            error: notices.filter(n => n.severity === 'error'),
            warning: notices.filter(n => n.severity === 'warning'),
            info: notices.filter(n => n.severity === 'info')
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

        // Initialize lucide icons in modal
        if (typeof lucide !== 'undefined') {
            lucide.createIcons();
        }
    }

    function renderNoticeGroup(title, severity, notices) {
        const items = notices.slice(0, 50).map(notice => `
            <div class="notice-item ${severity}">
                <div class="notice-code">${escapeHtml(notice.code)}</div>
                <div class="notice-message">${escapeHtml(notice.message)}</div>
                <div class="notice-location">
                    ${notice.file ? `<span>File: <code>${escapeHtml(notice.file)}</code></span>` : ''}
                    ${notice.row ? `<span>Row: <code>${notice.row}</code></span>` : ''}
                    ${notice.field ? `<span>Field: <code>${escapeHtml(notice.field)}</code></span>` : ''}
                </div>
            </div>
        `).join('');

        const moreCount = notices.length > 50 ? notices.length - 50 : 0;
        const moreNote = moreCount > 0 ? `<p style="color: var(--text-secondary); font-size: 0.85rem; text-align: center;">+ ${moreCount} more ${severity}s (download JSON for full report)</p>` : '';

        return `
            <div class="notice-group">
                <div class="notice-group-header ${severity}">
                    <span>${title}</span>
                    <span class="notice-group-count">${notices.length}</span>
                </div>
                ${items}
                ${moreNote}
            </div>
        `;
    }

    function escapeHtml(text) {
        if (!text) return '';
        const div = document.createElement('div');
        div.textContent = text;
        return div.innerHTML;
    }

    function closeReportModal() {
        if (reportModal) {
            reportModal.classList.add('hidden');
        }
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
            error: notices.filter(n => n.severity === 'error'),
            warning: notices.filter(n => n.severity === 'warning'),
            info: notices.filter(n => n.severity === 'info')
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

