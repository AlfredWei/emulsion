<script>
  /**
   * A thin info bar between the Develop canvas and the Filmstrip, showing
   * the currently-open image's filename -- the one piece of "which photo
   * am I looking at" context Develop otherwise has no persistent readout
   * for (Library has this via MetadataPanel; Develop doesn't). Just the
   * filename for now, matching this codebase's "smallest real instance
   * first" practice -- no dimensions/file size/EXIF here, those already
   * live in MetadataPanel for when the user wants them.
   * @type {{ imagePath: string }}
   */
  let { imagePath } = $props();

  // imagePath is always a real OS path (forward-slash-separated even on
  // Windows, per this app's own established convention elsewhere -- see
  // e.g. develop.js's own path handling), so a plain split-on-"/" is
  // sufficient; no need for a path-parsing library for just a basename.
  let filename = $derived(imagePath.split("/").pop() ?? imagePath);
</script>

<div class="info-bar">
  <span class="filename" title={imagePath}>{filename}</span>
</div>

<style>
  .info-bar {
    display: flex;
    align-items: center;
    height: 28px;
    flex: none;
    padding: 0 14px;
    background: var(--bg-app);
    border-top: 1px solid var(--border-subtle);
    border-bottom: 1px solid var(--border-subtle);
  }
  .filename {
    font-size: 11px;
    color: var(--text-secondary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
