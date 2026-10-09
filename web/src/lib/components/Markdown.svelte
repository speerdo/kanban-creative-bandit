<script lang="ts">
  import { renderMarkdown } from '../markdown';

  let { text }: { text: string } = $props();
  const html = $derived(renderMarkdown(text));
</script>

<!-- Safe: renderMarkdown sanitizes with DOMPurify. -->
<div class="md">{@html html}</div>

<style>
  .md {
    line-height: 1.55;
    overflow-wrap: anywhere;
  }

  .md :global(:first-child) {
    margin-top: 0;
  }

  .md :global(:last-child) {
    margin-bottom: 0;
  }

  .md :global(p),
  .md :global(ul),
  .md :global(ol) {
    margin: 0 0 0.6em;
  }

  .md :global(ul),
  .md :global(ol) {
    padding-left: 1.4em;
  }

  .md :global(h1),
  .md :global(h2),
  .md :global(h3) {
    margin: 0.8em 0 0.4em;
    font-size: 1em;
  }

  .md :global(a) {
    color: var(--accent);
  }

  .md :global(code) {
    font-family: var(--font-mono);
    font-size: 0.85em;
    padding: 1px 4px;
    border-radius: 4px;
    background: var(--surface-2);
  }

  .md :global(pre) {
    padding: 8px 10px;
    border-radius: var(--radius-sm);
    background: var(--surface-2);
    overflow-x: auto;
  }

  .md :global(pre code) {
    padding: 0;
    background: none;
  }

  .md :global(blockquote) {
    margin: 0 0 0.6em;
    padding-left: 10px;
    border-left: 3px solid var(--border);
    color: var(--text-muted);
  }

  .md :global(input[type='checkbox']) {
    margin: 0 6px 0 0;
    vertical-align: middle;
  }

  .md :global(li:has(> input[type='checkbox'])) {
    list-style: none;
    margin-left: -1.2em;
  }
</style>
