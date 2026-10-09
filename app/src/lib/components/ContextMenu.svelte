<script>
  // The one context menu (RFC-0028 §3.1). Renders whatever `contextMenu` holds -- plain item data from a
  // builder -- and knows nothing about photos. Navigation rules are the pure reducer in
  // lib/contextMenus/navigation.js; placement and activation rules are in model.js.
  //
  // Focus: DOM focus stays on the root menu element and `aria-activedescendant` names the highlighted row,
  // so one keydown handler serves every level. Closes on an outside pointer-down, window blur, resize and
  // scroll; Esc and the end of an action return focus to the element that opened it.

  import { contextMenu } from "$lib/state/contextMenu.svelte.js";
  import { INITIAL, hover, highlighted, levelsOf, navigate } from "$lib/contextMenus/navigation.js";
  import { activation, firstEnabled, isItem, isSeparator, placeAtPoint, placeSubmenu } from "$lib/contextMenus/model.js";
  import ConfirmDialog from "$lib/components/ConfirmDialog.svelte";

  let nav = $state(INITIAL);

  let levels = $derived(levelsOf(contextMenu.items, nav));
  let activeId = $derived(nav.path.length > 0 ? `cm-${nav.path.length - 1}-${nav.path[nav.path.length - 1]}` : undefined);

  // A newly opened menu starts with nothing highlighted (mouse) or the first row (keyboard), and takes focus.
  $effect(() => {
    if (!contextMenu.isOpen) return;
    const first = firstEnabled(contextMenu.items);
    nav = contextMenu.fromKeyboard && first >= 0 ? { path: [first], sub: false } : INITIAL;
  });

  /** Places a level once it is in the DOM and its size is known: the root at the pointer, a submenu beside
   * its parent row. Re-runs when the parameters change.
   * @param {HTMLElement} node @param {{ depth: number }} params */
  function place(node, params) {
    /** @param {{ depth: number }} p */
    function apply(p) {
      const size = { width: node.offsetWidth, height: node.offsetHeight };
      const viewport = { width: window.innerWidth, height: window.innerHeight };
      let pos;
      if (p.depth === 0) {
        pos = placeAtPoint(contextMenu.x, contextMenu.y, size, viewport);
      } else {
        const row = document.getElementById(`cm-${p.depth - 1}-${nav.path[p.depth - 1]}`);
        if (!row) return;
        pos = placeSubmenu(row.getBoundingClientRect(), size, viewport);
      }
      node.style.left = `${pos.left}px`;
      node.style.top = `${pos.top}px`;
    }
    apply(params);
    if (params.depth === 0) node.focus({ preventScroll: true });
    return { update: apply };
  }

  /** @param {number} depth @param {number} index */
  function onHover(depth, index) {
    nav = hover(contextMenu.items, nav, depth, index);
  }

  /** @param {import('$lib/contextMenus/model.js').MenuItem} item */
  function runItem(item) {
    const kind = activation(item);
    if (kind === "none") return;
    if (kind === "submenu") {
      const state = { path: nav.path, sub: true };
      const first = firstEnabled(item.children ?? []);
      nav = first >= 0 ? { path: [...state.path, first], sub: false } : state;
      return;
    }
    contextMenu.close(true);
    if (kind === "confirm" && item.confirm && item.run) contextMenu.askConfirm(item.confirm, item.run);
    else item.run?.();
  }

  /** @param {number} depth @param {number} index @param {import('$lib/contextMenus/model.js').MenuItem} item */
  function onClickRow(depth, index, item) {
    nav = hover(contextMenu.items, nav, depth, index);
    runItem(item);
  }

  /** @param {KeyboardEvent} e */
  function onKeydown(e) {
    if (!contextMenu.isOpen) return;
    if (e.key === "Tab") {
      contextMenu.close();
      return;
    }
    if (e.ctrlKey || e.metaKey || e.altKey) return;
    // Every plain key belongs to the open menu: a stray letter must not reach the page's shortcuts (P = Pick).
    e.preventDefault();
    e.stopPropagation();
    const step = navigate(contextMenu.items, nav, e.key);
    if (step.close) {
      contextMenu.close(true);
      return;
    }
    nav = step.state;
    if (step.activate) {
      const item = highlighted(contextMenu.items, nav);
      if (item) runItem(item);
    }
  }

  /** @param {PointerEvent} e */
  function onWindowPointerDown(e) {
    if (!contextMenu.isOpen) return;
    const t = e.target;
    if (t instanceof Element && t.closest(".context-menu")) return;
    contextMenu.close();
  }
</script>

<svelte:window
  onkeydowncapture={onKeydown}
  onpointerdowncapture={onWindowPointerDown}
  onblur={() => contextMenu.close()}
  onresize={() => contextMenu.close()}
  onscrollcapture={() => contextMenu.isOpen && contextMenu.close()}
/>

{#if contextMenu.isOpen}
  {#each levels as entries, depth (depth)}
    <div
      class="context-menu"
      class:root={depth === 0}
      role="menu"
      tabindex="-1"
      aria-activedescendant={depth === 0 ? activeId : undefined}
      data-testid={depth === 0 ? "context-menu" : "context-submenu"}
      use:place={{ depth }}
    >
      {#each entries as entry, index (index)}
        {#if isSeparator(entry)}
          <div class="sep" role="separator"></div>
        {:else if !isItem(entry)}
          <div class="header">{entry.header}</div>
        {:else}
          <div
            id="cm-{depth}-{index}"
            class="row"
            class:hl={nav.path[depth] === index}
            class:danger={entry.danger}
            role={entry.checked !== undefined ? "menuitemradio" : "menuitem"}
            aria-checked={entry.checked !== undefined ? entry.checked : undefined}
            aria-disabled={entry.disabled ? true : undefined}
            aria-haspopup={entry.children ? "menu" : undefined}
            title={entry.disabled ? entry.reason : undefined}
            data-menu-id={entry.id}
            tabindex="-1"
            onpointerenter={() => onHover(depth, index)}
            onclick={() => onClickRow(depth, index, entry)}
            onkeydown={() => {}}
          >
            <span class="check" aria-hidden="true">{entry.checked ? "✓" : ""}</span>
            <span class="label">{entry.label}</span>
            {#if entry.shortcut}<span class="shortcut">{entry.shortcut}</span>{/if}
            {#if entry.children}<span class="arrow" aria-hidden="true">▸</span>{/if}
          </div>
        {/if}
      {/each}
    </div>
  {/each}
{/if}

<ConfirmDialog
  open={contextMenu.pendingConfirm !== null}
  title={contextMenu.pendingConfirm?.title ?? ""}
  message={contextMenu.pendingConfirm?.message ?? ""}
  confirmLabel={contextMenu.pendingConfirm?.confirmLabel ?? "Confirm"}
  onConfirm={() => contextMenu.confirm()}
  onCancel={() => contextMenu.cancelConfirm()}
/>

<style>
  .context-menu {
    position: fixed;
    z-index: 1000;
    min-width: 210px;
    padding: 4px;
    background: var(--bg-panel-raised);
    border: 1px solid var(--border-strong);
    border-radius: 6px;
    box-shadow: var(--shadow-soft);
    font-size: 12px;
    outline: none;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 5px 8px;
    border-radius: var(--radius-s);
    color: var(--text-primary);
    white-space: nowrap;
    cursor: default;
    user-select: none;
  }
  .row.hl {
    background: var(--accent-soft);
    color: var(--accent-strong);
  }
  .row[aria-disabled="true"] {
    color: var(--text-tertiary);
  }
  .row.danger {
    color: var(--label-red);
  }
  .row.danger.hl {
    background: rgba(225, 88, 91, 0.16);
  }
  .check {
    width: 12px;
    flex: none;
    text-align: center;
    color: var(--accent-strong);
  }
  .label {
    flex: 1;
  }
  .shortcut,
  .arrow {
    color: var(--text-tertiary);
    font-family: var(--font-mono);
    font-size: 10.5px;
  }
  .sep {
    height: 1px;
    margin: 4px 2px;
    background: var(--border-subtle);
  }
  .header {
    padding: 4px 8px 5px;
    font-size: 10px;
    letter-spacing: 0.05em;
    text-transform: uppercase;
    color: var(--text-tertiary);
  }
</style>
