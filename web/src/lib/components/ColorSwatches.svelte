<!-- Palette swatches, a "Custom…" color, and optionally a "none" choice (e.g. Default background). -->
<script lang="ts">
  import { COLORS, solid } from '../colors';

  let {
    value,
    onpick,
    none,
  }: {
    value: string | null;
    onpick: (color: string | null) => void;
    /** Label for a "no color" option; omitted means there isn't one. */
    none?: string;
  } = $props();

  const custom = $derived(value?.startsWith('#') ? value : null);
</script>

<div class="grid" role="listbox" aria-label="Color">
  {#if none}
    <button class="swatch none" class:on={value === null} role="option" aria-selected={value === null} title={none} onclick={() => onpick(null)}
    ></button>
  {/if}
  {#each COLORS as c (c)}
    <button
      class="swatch"
      class:on={c === value}
      style:background={solid(c)}
      role="option"
      aria-selected={c === value}
      title={c}
      onclick={() => onpick(c)}
    ></button>
  {/each}
  <label class="swatch custom" class:on={custom !== null} title="Custom…" style:--custom={custom ?? 'transparent'}>
    <span class="sr-only">Custom color</span>
    <input type="color" value={custom ?? '#e8451f'} onchange={(e) => onpick(e.currentTarget.value)} />
  </label>
</div>

<style>
  .grid {
    display: grid;
    grid-template-columns: repeat(7, 24px);
    gap: 6px;
    padding: 6px;
  }

  .swatch {
    position: relative;
    width: 24px;
    height: 24px;
    padding: 0;
    border-radius: 50%;
    border: 2px solid transparent;
    cursor: pointer;
  }

  .swatch.on {
    outline: 2px solid var(--text);
    outline-offset: 1px;
  }

  .none {
    background: var(--surface);
    border: 1.5px dashed var(--text-muted);
  }

  .custom {
    background:
      radial-gradient(circle, var(--custom) 0 42%, transparent 44%),
      conic-gradient(#e8451f, #ffe800, #2fbf71, #1fb5d6, #1b27e8, #e5195a, #e8451f);
  }

  .custom input {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    opacity: 0;
    cursor: pointer;
  }
</style>
