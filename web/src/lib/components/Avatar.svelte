<script lang="ts">
  import type { User } from '../api';
  import { solid } from '../colors';
  import { onColor } from '../theme';

  let { user, size = 22 }: { user: User | undefined; size?: number } = $props();
</script>

{#if user}
  <span
    class="avatar"
    style:width="{size}px"
    style:height="{size}px"
    style:font-size="{size * 0.45}px"
    style:background={solid(user.avatar_color)}
    style:color={onColor(user.avatar_color)}
    title={user.display_name}>{user.display_name.slice(0, 1).toUpperCase()}</span
  >
{:else}
  <span class="avatar empty" style:width="{size}px" style:height="{size}px" title="Unassigned"></span>
{/if}

<style>
  .avatar {
    display: inline-grid;
    place-items: center;
    flex: none;
    border-radius: 50%;
    font-weight: 600;
  }

  .empty {
    border: 1.5px dashed var(--border);
  }
</style>
