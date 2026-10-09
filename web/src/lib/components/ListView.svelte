<script lang="ts">
  import type { ProjectStore } from '../project.svelte';
  import Section from './Section.svelte';

  let { store }: { store: ProjectStore } = $props();
</script>

<div class="list">
  {#each store.statuses as status (status.id)}
    <Section {status} {store} />
  {/each}
</div>

<style>
  /* The list sits on a panel so a custom canvas never sits behind its text. */
  .list {
    margin: 16px 24px 96px;
    padding: 12px 16px 4px;
    max-width: 1100px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius);
  }

  @media (max-width: 760px) {
    .list {
      margin: 8px 6px 96px;
      padding: 8px 8px 2px;
    }
  }
</style>
