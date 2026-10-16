<script>
  // "Connected photos" block of the Library info panel (RFC-0029 §3.2): for an AI enlargement, where it came from;
  // for an original, its enlargements. Renders nothing for an ordinary photo, and does not even ask the backend.
  import { getUpscaleLinks } from "$lib/api/catalog.js";
  import { library } from "$lib/state/library.svelte.js";
  import { shell } from "$lib/state/shell.svelte.js";
  import { goToOriginal, showEnlargements } from "$lib/actions/upscaleActions.js";
  import { connectedLines } from "$lib/upscaleLinks.js";

  /** @type {{ image: import('$lib/api/catalog.js').ImageSummary }} */
  let { image } = $props();

  let connected = $derived(image.enlarged_scale != null || (image.enlargement_count ?? 0) > 0);
  let links = $state(/** @type {import('$lib/api/catalog.js').UpscaleLinks | null} */ (null));
  let failed = $state(false);

  // Re-read when the photo changes, when its link data changes, and when the user comes back from Develop (the
  // original may have been edited there: "edited since" is computed on read).
  $effect(() => {
    const id = image.image_id;
    void image.enlarged_from;
    void image.enlargement_count;
    void shell.activeModule;
    if (!connected) {
      links = null;
      return;
    }
    let stale = false;
    failed = false;
    getUpscaleLinks(id)
      .then((l) => {
        if (!stale) links = l;
      })
      .catch(() => {
        if (!stale) {
          links = null;
          failed = true;
        }
      });
    return () => {
      stale = true;
    };
  });

  const nameOf = (/** @type {number} */ imageId) => {
    const found = library.images.find((i) => i.image_id === imageId);
    return found ? (found.path.split(/[/\\]/).pop() ?? found.path) : null;
  };
  let lines = $derived(links ? connectedLines(links, nameOf) : []);

  function go(/** @type {import('$lib/upscaleLinks.js').ConnectedLine} */ line) {
    if (line.kind === "origin") goToOriginal(image.image_id);
    else showEnlargements(image.image_id);
  }
</script>

{#if connected}
  <div class="section-label">Connected photos</div>
  {#each lines as line (line.kind)}
    <div class="connected-line">
      <span class="connected-text">{line.text}</span>
      {#if line.note}<span class="connected-note">{line.note}</span>{/if}
      {#if line.imageId !== null && line.action}
        <button type="button" class="connected-link" onclick={() => go(line)}>{line.action} ↗</button>
      {/if}
    </div>
  {/each}
  {#if failed}
    <p class="connected-note">Could not read the connection.</p>
  {/if}
{/if}

<style>
  .section-label {
    font-family: var(--font-mono);
    font-size: 10px;
    letter-spacing: 0.09em;
    text-transform: uppercase;
    color: var(--text-tertiary);
    padding: 10px 4px 6px;
    font-weight: 600;
  }
  .connected-line {
    display: flex;
    flex-direction: column;
    gap: 3px;
    padding: 2px 4px 6px;
    font-size: 12px;
    color: var(--text-secondary);
  }
  .connected-note {
    font-size: 11px;
    color: var(--text-tertiary);
    padding: 0 4px;
  }
  .connected-line .connected-note {
    padding: 0;
  }
  .connected-link {
    all: unset;
    align-self: flex-start;
    cursor: pointer;
    font-size: 11px;
    color: var(--accent-strong);
  }
  .connected-link:hover {
    text-decoration: underline;
  }
  .connected-link:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
</style>
