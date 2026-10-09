<script lang="ts">
  import { api, setUnauthorizedHandler } from './lib/api';
  import Login from './lib/components/Login.svelte';
  import Shell from './lib/components/Shell.svelte';
  import Toasts from './lib/components/Toasts.svelte';
  import { session } from './lib/state.svelte';
  import { applyPrefs, watchSystemTheme } from './lib/theme';

  setUnauthorizedHandler(() => (session.me = null));
  watchSystemTheme(() => session.me?.prefs);

  // Any change to the prefs (Settings, or another device via live sync) restyles the page.
  $effect(() => {
    if (session.me) applyPrefs(session.me.prefs);
  });

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
