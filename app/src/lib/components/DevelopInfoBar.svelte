<script>
  /**
   * @type {{ imagePath: string }}
   */
  let { imagePath } = $props();

  let filename = $derived(imagePath.split("/").pop() ?? imagePath);
  let dirPath = $derived(
    imagePath.slice(0, Math.max(0, imagePath.length - filename.length)).replace(/[/\\]$/, ""),
  );
  let copied = $state(false);

  async function copyPath() {
    try {
      await navigator.clipboard.writeText(imagePath);
      copied = true;
      setTimeout(() => (copied = false), 1500);
    } catch {
      // ignore
    }
  }
</script>

<div class="info-bar">
  <div class="path-display" title={imagePath}>
    {#if dirPath}
      <span class="dir-path">{dirPath}/</span>
    {/if}
    <span class="filename">{filename}</span>
  </div>
  {#if imagePath}
    <button class="copy-btn" type="button" title="Copy full path to clipboard" onclick={copyPath}>
      {copied ? "Copied ✓" : "Copy Path"}
    </button>
  {/if}
</div>

<style>
  .info-bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    height: 28px;
    flex: none;
    padding: 0 14px;
    background: var(--bg-app);
    border-top: 1px solid var(--border-subtle);
    border-bottom: 1px solid var(--border-subtle);
    gap: 12px;
  }
  .path-display {
    display: flex;
    align-items: center;
    font-size: 11px;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
    min-width: 0;
  }
  .dir-path {
    color: var(--text-tertiary);
    font-family: var(--font-mono);
    font-size: 10px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .filename {
    font-weight: 500;
    color: var(--text-primary);
    font-family: var(--font-mono);
    font-size: 11px;
    flex-shrink: 0;
  }
  .copy-btn {
    all: unset;
    font-size: 10px;
    font-family: var(--font-mono);
    color: var(--text-tertiary);
    background: var(--bg-panel-raised);
    padding: 2px 7px;
    border-radius: var(--radius-s);
    cursor: pointer;
    flex-shrink: 0;
    transition: all 0.12s ease;
  }
  .copy-btn:hover {
    color: var(--text-primary);
    background: var(--bg-control-hover);
  }
</style>
