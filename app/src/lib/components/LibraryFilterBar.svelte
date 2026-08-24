<script>
  /**
   * @type {{
   *   searchQuery: string,
   *   flagFilter: "all" | "pick" | "unflagged" | "reject",
   *   minRating: number,
   *   ratingOp: ">=" | "=",
   *   colorLabelFilter: string,
   *   fileTypeFilter: "all" | "raw" | "jpeg",
   *   cameraFilter: string,
   *   lensFilter: string,
   *   dateFrom: string,
   *   dateTo: string,
   *   cameraOptions: string[],
   *   lensOptions: string[],
   *   totalCount: number,
   *   matchedCount: number,
   *   onSearchChange: (val: string) => void,
   *   onFlagChange: (flag: "all" | "pick" | "unflagged" | "reject") => void,
   *   onRatingChange: (rating: number, op: ">=" | "=") => void,
   *   onColorLabelChange: (label: string) => void,
   *   onFileTypeChange: (type: "all" | "raw" | "jpeg") => void,
   *   onCameraChange: (camera: string) => void,
   *   onLensChange: (lens: string) => void,
   *   onDateRangeChange: (from: string, to: string) => void,
   *   onReset: () => void,
   * }}
   */
  let {
    searchQuery,
    flagFilter,
    minRating,
    ratingOp,
    colorLabelFilter,
    fileTypeFilter,
    cameraFilter,
    lensFilter,
    dateFrom,
    dateTo,
    cameraOptions,
    lensOptions,
    totalCount,
    matchedCount,
    onSearchChange,
    onFlagChange,
    onRatingChange,
    onColorLabelChange,
    onFileTypeChange,
    onCameraChange,
    onLensChange,
    onDateRangeChange,
    onReset,
  } = $props();

  const COLOR_OPTIONS = [
    { id: "all", label: "All Colors", color: "" },
    { id: "red", label: "Red", color: "var(--label-red)" },
    { id: "yellow", label: "Yellow", color: "var(--label-yellow)" },
    { id: "green", label: "Green", color: "var(--label-green)" },
    { id: "blue", label: "Blue", color: "var(--label-blue)" },
    { id: "purple", label: "Purple", color: "var(--label-purple)" },
    { id: "none", label: "None", color: "rgba(255,255,255,0.2)" },
  ];

  let hasActiveFilters = $derived(
    Boolean(searchQuery.trim()) ||
      flagFilter !== "all" ||
      minRating > 0 ||
      colorLabelFilter !== "all" ||
      fileTypeFilter !== "all" ||
      cameraFilter !== "all" ||
      lensFilter !== "all" ||
      Boolean(dateFrom) ||
      Boolean(dateTo),
  );

  function toggleStar(/** @type {number} */ n) {
    if (minRating === n) {
      onRatingChange(0, ratingOp);
    } else {
      onRatingChange(n, ratingOp);
    }
  }

  function toggleRatingOp() {
    onRatingChange(minRating, ratingOp === ">=" ? "=" : ">=");
  }
</script>

<div class="filter-bar">
  <!-- Search Input -->
  <div class="search-box">
    <span class="search-icon">🔍</span>
    <input
      type="text"
      placeholder="Search photos (name, camera, lens, caption, tags)…"
      value={searchQuery}
      oninput={(e) => onSearchChange(e.currentTarget.value)}
      aria-label="Search photos in library"
    />
    {#if searchQuery}
      <button
        type="button"
        class="clear-search-btn"
        aria-label="Clear search query"
        onclick={() => onSearchChange("")}
      >×</button>
    {/if}
  </div>

  <div class="divider"></div>

  <!-- Flag Filters -->
  <div class="btn-group flag-group" role="radiogroup" aria-label="Flag filter">
    <button
      type="button"
      class="filter-btn"
      class:active={flagFilter === "all"}
      title="All Flags"
      onclick={() => onFlagChange("all")}
    >
      All
    </button>
    <button
      type="button"
      class="filter-btn pick-btn"
      class:active={flagFilter === "pick"}
      title="Picks only (P)"
      aria-label="Picks only"
      onclick={() => onFlagChange(flagFilter === "pick" ? "all" : "pick")}
    >
      <svg viewBox="0 0 16 16" width="12" height="12" fill="currentColor" aria-hidden="true">
        <path d="M3 2v12h1.5V9h7l-1.5-3.5L11.5 2H3z" />
      </svg>
    </button>
    <button
      type="button"
      class="filter-btn unflagged-btn"
      class:active={flagFilter === "unflagged"}
      title="Unflagged only (U)"
      aria-label="Unflagged only"
      onclick={() => onFlagChange(flagFilter === "unflagged" ? "all" : "unflagged")}
    >
      <svg viewBox="0 0 16 16" width="12" height="12" fill="none" stroke="currentColor" stroke-width="1.4" aria-hidden="true">
        <path d="M3 2v12h1.5V9h6.5l-1.2-3.5L11 2H3z" stroke-linejoin="round" />
      </svg>
    </button>
    <button
      type="button"
      class="filter-btn reject-btn"
      class:active={flagFilter === "reject"}
      title="Rejects only (X)"
      aria-label="Rejects only"
      onclick={() => onFlagChange(flagFilter === "reject" ? "all" : "reject")}
    >
      <svg viewBox="0 0 16 16" width="12" height="12" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" aria-hidden="true">
        <path d="M4 4l8 8M12 4l-8 8" />
      </svg>
    </button>
  </div>

  <div class="divider"></div>

  <!-- Star Rating Filter -->
  <div class="rating-filter">
    <button
      type="button"
      class="op-toggle-btn"
      title="Toggle {ratingOp === '>=' ? 'Greater than or equal to (>=)' : 'Exact match (=)'}"
      onclick={toggleRatingOp}
    >
      {ratingOp}
    </button>
    <div class="stars-row">
      {#each [1, 2, 3, 4, 5] as n (n)}
        <button
          type="button"
          class="star-btn"
          class:active={n <= minRating}
          title="Filter by {ratingOp} {n} star{n === 1 ? '' : 's'}"
          onclick={() => toggleStar(n)}
        >
          ★
        </button>
      {/each}
    </div>
  </div>

  <div class="divider"></div>

  <!-- Color Label Filter -->
  <div class="color-filter">
    {#each COLOR_OPTIONS as opt (opt.id)}
      {#if opt.id === "all"}
        <button
          type="button"
          class="color-btn all-color-btn"
          class:active={colorLabelFilter === "all"}
          title="All color labels"
          onclick={() => onColorLabelChange("all")}
        >
          All
        </button>
      {:else}
        <button
          type="button"
          class="color-dot-btn"
          class:active={colorLabelFilter === opt.id}
          style={opt.color ? `background: ${opt.color}` : ""}
          title="{opt.label} color label"
          onclick={() => onColorLabelChange(colorLabelFilter === opt.id ? "all" : opt.id)}
        ></button>
      {/if}
    {/each}
  </div>

  <div class="divider"></div>

  <!-- File Type Filter -->
  <div class="btn-group type-group" role="radiogroup" aria-label="File format filter">
    <button
      type="button"
      class="filter-btn"
      class:active={fileTypeFilter === "all"}
      onclick={() => onFileTypeChange("all")}
    >
      All
    </button>
    <button
      type="button"
      class="filter-btn"
      class:active={fileTypeFilter === "raw"}
      title="RAW files (.CR2, .NEF, .ARW, .DNG, etc.)"
      onclick={() => onFileTypeChange("raw")}
    >
      RAW
    </button>
    <button
      type="button"
      class="filter-btn"
      class:active={fileTypeFilter === "jpeg"}
      title="JPEG files (.jpg, .jpeg)"
      onclick={() => onFileTypeChange("jpeg")}
    >
      JPEG
    </button>
  </div>

  <div class="divider"></div>

  <!-- Camera / Lens Filters -->
  <select
    class="dropdown-filter"
    aria-label="Filter by camera"
    value={cameraFilter}
    onchange={(e) => onCameraChange(e.currentTarget.value)}
  >
    <option value="all">All Cameras</option>
    {#each cameraOptions as camera (camera)}
      <option value={camera}>{camera}</option>
    {/each}
  </select>

  <select
    class="dropdown-filter"
    aria-label="Filter by lens"
    value={lensFilter}
    onchange={(e) => onLensChange(e.currentTarget.value)}
  >
    <option value="all">All Lenses</option>
    {#each lensOptions as lens (lens)}
      <option value={lens}>{lens}</option>
    {/each}
  </select>

  <div class="divider"></div>

  <!-- Date Taken Range Filter -->
  <div class="date-range-filter">
    <input
      type="date"
      class="date-input"
      aria-label="Date taken from"
      value={dateFrom}
      onchange={(e) => onDateRangeChange(e.currentTarget.value, dateTo)}
    />
    <span class="date-range-sep">–</span>
    <input
      type="date"
      class="date-input"
      aria-label="Date taken to"
      value={dateTo}
      onchange={(e) => onDateRangeChange(dateFrom, e.currentTarget.value)}
    />
  </div>

  <!-- Spacer & Counter -->
  <div class="spacer"></div>

  <div class="counts-badge">
    {#if hasActiveFilters}
      <span class="filtered-highlight">{matchedCount}</span>
      <span class="total-text">of {totalCount} photos</span>
      <button class="reset-filters-btn" type="button" onclick={onReset} title="Clear all filters">
        Clear Filters
      </button>
    {:else}
      <span class="total-text">{totalCount} photo{totalCount === 1 ? "" : "s"}</span>
    {/if}
  </div>
</div>

<style>
  .filter-bar {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 14px;
    background: var(--bg-panel);
    border-bottom: 1px solid var(--border-subtle);
    flex: none;
    font-size: 11.5px;
    min-height: 36px;
    box-sizing: border-box;
    overflow-x: auto;
  }
  .search-box {
    position: relative;
    display: flex;
    align-items: center;
    width: 220px;
    flex-shrink: 0;
  }
  .search-icon {
    position: absolute;
    left: 7px;
    font-size: 11px;
    color: var(--text-tertiary);
    pointer-events: none;
  }
  .search-box input {
    width: 100%;
    box-sizing: border-box;
    padding: 4px 22px 4px 24px;
    background: var(--bg-app);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-s);
    color: var(--text-primary);
    font-size: 11px;
    font-family: inherit;
  }
  .search-box input:focus {
    outline: 2px solid var(--accent);
    outline-offset: -1px;
  }
  .clear-search-btn {
    all: unset;
    position: absolute;
    right: 6px;
    cursor: pointer;
    font-size: 12px;
    color: var(--text-tertiary);
    line-height: 1;
  }
  .clear-search-btn:hover {
    color: var(--text-primary);
  }
  .divider {
    width: 1px;
    height: 16px;
    background: var(--border-subtle);
    flex-shrink: 0;
  }
  .btn-group {
    display: flex;
    align-items: center;
    background: var(--bg-app);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-s);
    padding: 1px;
    flex-shrink: 0;
  }
  .filter-btn {
    all: unset;
    padding: 3px 8px;
    font-size: 11px;
    font-family: var(--font-mono);
    color: var(--text-tertiary);
    cursor: pointer;
    border-radius: 3px;
    transition: all 0.1s ease;
  }
  .filter-btn:hover {
    color: var(--text-primary);
  }
  .filter-btn.active {
    background: var(--bg-panel-raised);
    color: var(--text-primary);
    font-weight: 500;
  }
  .filter-btn.pick-btn.active {
    color: var(--label-green);
    background: var(--bg-panel-raised);
  }
  .filter-btn.reject-btn.active {
    color: var(--label-red);
    background: var(--bg-panel-raised);
  }
  .rating-filter {
    display: flex;
    align-items: center;
    gap: 4px;
    flex-shrink: 0;
    background: var(--bg-app);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-s);
    padding: 2px 6px;
  }
  .op-toggle-btn {
    all: unset;
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--text-secondary);
    cursor: pointer;
    padding: 0 4px;
    border-radius: 2px;
  }
  .op-toggle-btn:hover {
    color: var(--accent);
  }
  .stars-row {
    display: flex;
    gap: 1px;
  }
  .star-btn {
    all: unset;
    font-size: 13px;
    line-height: 1;
    color: rgba(255, 255, 255, 0.2);
    cursor: pointer;
    transition: color 0.1s ease;
  }
  .star-btn.active {
    color: var(--accent-strong);
  }
  .color-filter {
    display: flex;
    align-items: center;
    gap: 5px;
    flex-shrink: 0;
  }
  .all-color-btn {
    all: unset;
    font-size: 10.5px;
    font-family: var(--font-mono);
    color: var(--text-tertiary);
    cursor: pointer;
    padding: 2px 5px;
    border-radius: var(--radius-s);
  }
  .all-color-btn.active {
    color: var(--text-primary);
    background: var(--bg-panel-raised);
    font-weight: 500;
  }
  .color-dot-btn {
    all: unset;
    width: 10px;
    height: 10px;
    border-radius: 50%;
    cursor: pointer;
    opacity: 0.5;
    box-shadow: 0 0 0 1px rgba(0, 0, 0, 0.5);
    transition: all 0.1s ease;
  }
  .color-dot-btn:hover {
    opacity: 0.85;
    transform: scale(1.15);
  }
  .color-dot-btn.active {
    opacity: 1;
    box-shadow: 0 0 0 2px var(--text-primary);
    transform: scale(1.2);
  }
  .dropdown-filter {
    flex-shrink: 0;
    max-width: 130px;
    background: var(--bg-app);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-s);
    color: var(--text-secondary);
    font-size: 11px;
    font-family: inherit;
    padding: 3px 4px;
  }
  .dropdown-filter:hover {
    color: var(--text-primary);
  }
  .date-range-filter {
    display: flex;
    align-items: center;
    gap: 4px;
    flex-shrink: 0;
  }
  .date-input {
    background: var(--bg-app);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-s);
    color: var(--text-secondary);
    font-size: 11px;
    font-family: inherit;
    padding: 2px 4px;
    color-scheme: dark;
  }
  .date-input:hover {
    color: var(--text-primary);
  }
  .date-range-sep {
    color: var(--text-tertiary);
    font-size: 11px;
  }
  .spacer {
    flex: 1;
    min-width: 8px;
  }
  .counts-badge {
    display: flex;
    align-items: center;
    gap: 6px;
    font-family: var(--font-mono);
    font-size: 11px;
    flex-shrink: 0;
  }
  .filtered-highlight {
    color: var(--accent-strong);
    font-weight: 600;
  }
  .total-text {
    color: var(--text-tertiary);
  }
  .reset-filters-btn {
    all: unset;
    font-size: 10.5px;
    color: var(--accent);
    background: var(--bg-app);
    border: 1px solid var(--border-subtle);
    padding: 2px 7px;
    border-radius: var(--radius-s);
    cursor: pointer;
    margin-left: 4px;
    transition: all 0.12s ease;
  }
  .reset-filters-btn:hover {
    background: var(--accent-soft);
    color: var(--accent-strong);
    border-color: var(--accent);
  }
</style>
