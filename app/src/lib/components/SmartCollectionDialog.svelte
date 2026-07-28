<script>
  import { onMount } from "svelte";
  import { listKeywords } from "$lib/api/catalog.js";

  /**
   * Smart Collection create/edit (M2 Slice 5): a name plus an AND-only
   * rule-row builder. Rules are ANDed together, no OR/nesting for V1 --
   * see collectionRules.js for the evaluator these rules feed.
   * @type {{
   *   open: boolean,
   *   title: string,
   *   initialName?: string,
   *   initialRules?: import('$lib/api/catalog.js').CollectionRule[] | null,
   *   confirmLabel?: string,
   *   onConfirm: (name: string, rules: import('$lib/api/catalog.js').CollectionRule[]) => void,
   *   onCancel: () => void,
   * }}
   */
  let {
    open,
    title,
    initialName = "",
    initialRules = null,
    confirmLabel = "Save",
    onConfirm,
    onCancel,
  } = $props();

  const FLAG_OPTIONS = ["pick", "reject", "none"];
  const COLOR_OPTIONS = ["red", "yellow", "green", "blue", "purple", "none"];

  let name = $state("");
  let rules = $state(/** @type {import('$lib/api/catalog.js').CollectionRule[]} */ ([]));
  let allKeywords = $state(/** @type {import('$lib/api/catalog.js').KeywordNode[]} */ ([]));

  onMount(async () => {
    allKeywords = await listKeywords();
  });

  // "Has Keyword" needs full paths (not just leaf names) to disambiguate
  // same-named keywords under different parents -- same client-side
  // parent_id walk MetadataPanel already uses for its suggestions.
  let keywordOptions = $derived.by(() => {
    const byId = new Map(allKeywords.map((k) => [k.id, k]));
    return allKeywords.map((node) => {
      const segments = [node.name];
      let current = node;
      while (current.parent_id !== null) {
        const parent = byId.get(current.parent_id);
        if (!parent) break;
        segments.unshift(parent.name);
        current = parent;
      }
      return { id: node.id, path: segments.join(" / ") };
    });
  });

  // Reset to the initial (or blank) state each time the dialog opens.
  $effect(() => {
    if (!open) return;
    name = initialName;
    rules = initialRules && initialRules.length > 0 ? [...initialRules] : [{ field: "rating", op: ">=", value: 4 }];
  });

  /** @param {import('$lib/api/catalog.js').CollectionRule} rule */
  function kindOf(rule) {
    if (rule.field === "keyword") return rule.op === "empty" ? "keyword_empty" : "keyword_has";
    return rule.field;
  }

  /** @returns {import('$lib/api/catalog.js').CollectionRule} */
  function defaultRuleForKind(/** @type {string} */ kind) {
    switch (kind) {
      case "rating":
        return { field: "rating", op: ">=", value: 4 };
      case "flag":
        return { field: "flag", op: "==", value: "pick" };
      case "color_label":
        return { field: "color_label", op: "==", value: "red" };
      case "keyword_has":
        return { field: "keyword", op: "has", value: keywordOptions[0]?.id ?? 0 };
      default:
        return { field: "keyword", op: "empty" };
    }
  }

  function changeKind(/** @type {number} */ index, /** @type {string} */ kind) {
    rules = rules.map((rule, i) => (i === index ? defaultRuleForKind(kind) : rule));
  }

  function updateRule(/** @type {number} */ index, /** @type {Partial<import('$lib/api/catalog.js').CollectionRule>} */ patch) {
    rules = rules.map((rule, i) => (i === index ? { ...rule, ...patch } : rule));
  }

  function addRule() {
    rules = [...rules, defaultRuleForKind("rating")];
  }

  function removeRule(/** @type {number} */ index) {
    rules = rules.filter((_, i) => i !== index);
  }

  function submit() {
    const trimmed = name.trim();
    if (!trimmed || rules.length === 0) return;
    onConfirm(trimmed, rules);
  }
</script>

<svelte:window onkeydown={(e) => open && e.key === "Escape" && onCancel()} />

{#if open}
  <div class="overlay">
    <div class="dialog" role="dialog" aria-modal="true" aria-label={title}>
      <h2>{title}</h2>

      <div class="field">
        <label for="smart-name">Name</label>
        <input id="smart-name" type="text" placeholder="e.g. Best Shots" bind:value={name} />
      </div>

      <div class="field">
        <span class="field-label">Rules (all must match)</span>
        <div class="rules">
          {#each rules as rule, index (index)}
            <div class="rule-row">
              <select value={kindOf(rule)} onchange={(e) => changeKind(index, e.currentTarget.value)}>
                <option value="rating">Rating</option>
                <option value="flag">Flag</option>
                <option value="color_label">Color Label</option>
                <option value="keyword_has">Has Keyword</option>
                <option value="keyword_empty">Untagged (no keywords)</option>
              </select>

              {#if rule.field === "rating"}
                <select
                  value={rule.op}
                  onchange={(e) => updateRule(index, { op: /** @type {">=" | "<=" | "=="} */ (e.currentTarget.value) })}
                >
                  <option value=">=">≥</option>
                  <option value="<=">≤</option>
                  <option value="==">=</option>
                </select>
                <input
                  type="number"
                  min="0"
                  max="5"
                  value={rule.value}
                  onchange={(e) => updateRule(index, { value: Number(e.currentTarget.value) })}
                />
              {:else if rule.field === "flag"}
                <select value={rule.value} onchange={(e) => updateRule(index, { value: e.currentTarget.value })}>
                  {#each FLAG_OPTIONS as option (option)}
                    <option value={option}>{option}</option>
                  {/each}
                </select>
              {:else if rule.field === "color_label"}
                <select value={rule.value} onchange={(e) => updateRule(index, { value: e.currentTarget.value })}>
                  {#each COLOR_OPTIONS as option (option)}
                    <option value={option}>{option}</option>
                  {/each}
                </select>
              {:else if rule.field === "keyword" && rule.op === "has"}
                <select
                  value={rule.value}
                  onchange={(e) => updateRule(index, { value: Number(e.currentTarget.value) })}
                >
                  {#each keywordOptions as option (option.id)}
                    <option value={option.id}>{option.path}</option>
                  {:else}
                    <option value={0} disabled>No keywords yet</option>
                  {/each}
                </select>
              {/if}

              <button
                type="button"
                class="remove-rule"
                aria-label="Remove rule"
                disabled={rules.length === 1}
                onclick={() => removeRule(index)}
              >×</button>
            </div>
          {/each}
        </div>
        <button type="button" class="add-rule" onclick={addRule}>+ Add condition</button>
      </div>

      <div class="actions">
        <button class="secondary" type="button" onclick={onCancel}>Cancel</button>
        <button class="primary" type="button" onclick={submit} disabled={!name.trim()}>{confirmLabel}</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.5);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 100;
  }
  .dialog {
    width: 380px;
    background: var(--bg-panel);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-m);
    box-shadow: var(--shadow-soft);
    padding: 16px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  h2 {
    margin: 0;
    font-size: 13px;
    font-weight: 600;
    color: var(--text-primary);
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .field label,
  .field-label {
    font-size: 10.5px;
    color: var(--text-tertiary);
  }
  .field input {
    box-sizing: border-box;
    width: 100%;
    padding: 6px 8px;
    font-size: 12px;
    font-family: inherit;
    color: var(--text-primary);
    background: var(--bg-panel-raised);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-s);
  }
  .field input:focus {
    outline: 2px solid var(--accent);
    outline-offset: -1px;
  }
  .rules {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .rule-row {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .rule-row select,
  .rule-row input[type="number"] {
    box-sizing: border-box;
    padding: 5px 6px;
    font-size: 11.5px;
    font-family: inherit;
    color: var(--text-primary);
    background: var(--bg-panel-raised);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-s);
  }
  .rule-row select:first-child {
    flex: 1 1 auto;
    min-width: 0;
  }
  .rule-row input[type="number"] {
    width: 44px;
  }
  .remove-rule {
    all: unset;
    cursor: pointer;
    flex: none;
    padding: 0 4px;
    color: var(--text-tertiary);
  }
  .remove-rule:hover {
    color: var(--label-red);
  }
  .remove-rule:disabled {
    opacity: 0.4;
    cursor: default;
  }
  .add-rule {
    all: unset;
    cursor: pointer;
    align-self: flex-start;
    padding: 4px 2px;
    font-size: 11px;
    color: var(--accent-strong);
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 6px;
  }
  .actions button {
    all: unset;
    cursor: pointer;
    padding: 6px 14px;
    font-size: 11.5px;
    font-weight: 600;
    border-radius: 6px;
  }
  .actions button:disabled {
    opacity: 0.6;
    cursor: default;
  }
  .secondary {
    color: var(--text-secondary);
    border: 1px solid var(--border-strong);
  }
  .primary {
    background: var(--accent-soft);
    color: var(--accent-strong);
    border: 1px solid var(--accent);
  }
</style>
