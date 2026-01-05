import init, { validate_gtfs } from './pkg/gtfs_validator_wasm.js';

// Initialize WASM
init().catch(console.error);

document.addEventListener('DOMContentLoaded', () => {

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

    // UI Elements for results
    const errorCountEl = document.getElementById('error-count');
    const warningCountEl = document.getElementById('warning-count');
    const scoreNumberEl = document.getElementById('score-number');
    const scoreRingEl = document.getElementById('score-ring');

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

        // Reset
        if (resetBtn) {
            resetBtn.addEventListener('click', () => {
                resultState.classList.add('hidden');
                uploadState.classList.remove('hidden');
                fileInput.value = ''; // Reset input
            });
        }
    }

    async function handleFile(file) {
        if (!file.name.endsWith('.zip')) {
            alert('Please upload a ZIP file.');
            return;
        }

        // Show processing
        uploadState.classList.add('hidden');
        processingState.classList.remove('hidden');

        try {
            const arrayBuffer = await file.arrayBuffer();
            const bytes = new Uint8Array(arrayBuffer);

            // Run WASM validation (give UI a moment to update)
            setTimeout(() => {
                try {
                    const result = validate_gtfs(bytes, null);
                    showResults(result);
                } catch (err) {
                    console.error("Validation error:", err);
                    alert("Error processing file. See console for details.");
                    processingState.classList.add('hidden');
                    uploadState.classList.remove('hidden');
                }
            }, 100);
        } catch (err) {
            console.error("File reading error:", err);
            processingState.classList.add('hidden');
            uploadState.classList.remove('hidden');
        }
    }

    function showResults(result) {
        processingState.classList.add('hidden');
        resultState.classList.remove('hidden');

        const errors = result.error_count;
        const warnings = result.warning_count;

        errorCountEl.innerText = errors;
        warningCountEl.innerText = warnings;

        // Simple scoring: 100 if no errors, else 0. 
        // Or maybe 100 - (errors * 5). Min 0.
        let score = Math.max(0, 100 - (errors * 20));
        if (errors > 0 && score === 100) score = 90; // Penalize at least a bit

        scoreNumberEl.innerText = score;

        // Color ring
        if (errors > 0) {
            scoreRingEl.style.borderColor = 'var(--error)';
        } else if (warnings > 0) {
            scoreRingEl.style.borderColor = 'var(--warning)';
        } else {
            scoreRingEl.style.borderColor = 'var(--success)';
        }

        console.log("Validation Result:", result);
    }
});
