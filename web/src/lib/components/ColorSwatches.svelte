<script lang="ts">
  import { COLORS, solid } from '../colors';

  let { value, onpick }: { value: string; onpick: (color: string) => void } = $props();
</script>

<div class="grid" role="listbox" aria-label="Color">
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
</div>

<style>
  .grid {
    display: grid;
    grid-template-columns: repeat(6, 24px);
    gap: 6px;
    padding: 6px;
  }

  .swatch {
    width: 24px;
    height: 24px;
    border-radius: 50%;
    border: 2px solid transparent;
  }

  .swatch.on {
    outline: 2px solid var(--text);
    outline-offset: 1px;
  }
</style>
