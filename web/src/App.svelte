<script lang="ts">
  import { api, setUnauthorizedHandler } from './lib/api';
  import Login from './lib/components/Login.svelte';
  import Shell from './lib/components/Shell.svelte';
  import Toasts from './lib/components/Toasts.svelte';
  import { session } from './lib/state.svelte';

  setUnauthorizedHandler(() => (session.me = null));

  api
    .me()
    .then((me) => (session.me = me))
    .catch(() => (session.me = null))
    .finally(() => (session.checked = true));
</script>

{#if session.checked}
  {#if session.me}
    <Shell />
  {:else}
    <Login />
  {/if}
{/if}
<Toasts />
