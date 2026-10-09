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
  .list {
    padding: 16px 24px 96px;
    max-width: 1100px;
  }

  @media (max-width: 760px) {
    .list {
      padding: 12px 8px 96px;
    }
  }
</style>
