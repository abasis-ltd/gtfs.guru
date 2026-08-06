document.addEventListener("DOMContentLoaded", () => {
  const copyButton = document.querySelector("[data-copy]");
  if (copyButton) {
    copyButton.addEventListener("click", async () => {
      try {
        await navigator.clipboard.writeText(copyButton.dataset.copy);
        copyButton.textContent = "Copied";
        window.setTimeout(() => { copyButton.textContent = "Copy code"; }, 1400);
      } catch (_) {
        copyButton.textContent = "Select code above";
      }
    });
  }

  const search = document.querySelector("#notice-search");
  const rows = [...document.querySelectorAll(".notice-row")];
  const filters = [...document.querySelectorAll("[data-filter]")];
  const noResults = document.querySelector("#no-results");
  let activeSeverity = "all";

  const applyFilters = () => {
    const query = (search?.value || "").trim().toLowerCase();
    let visible = 0;
    rows.forEach((row) => {
      const matchesSearch = !query || row.dataset.search.toLowerCase().includes(query);
      const matchesSeverity = activeSeverity === "all" || row.dataset.severity === activeSeverity;
      const show = matchesSearch && matchesSeverity;
      row.hidden = !show;
      if (show) visible += 1;
    });
    if (noResults) noResults.hidden = visible !== 0;
  };

  search?.addEventListener("input", applyFilters);
  filters.forEach((button) => {
    button.addEventListener("click", () => {
      activeSeverity = button.dataset.filter;
      filters.forEach((candidate) => candidate.classList.toggle("active", candidate === button));
      applyFilters();
    });
  });
});
