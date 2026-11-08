<!-- Placeholder for empty lists and failed loads. -->
<script lang="ts">
  import type { Snippet } from "svelte";

  let { title, detail = "", error = false, children }: {
    title: string;
    detail?: string;
    error?: boolean;
    children?: Snippet;
  } = $props();
</script>

<div class="empty" class:error>
  <div class="title">{title}</div>
  {#if detail}<div class="muted">{detail}</div>{/if}
  {#if children}<div class="extra">{@render children()}</div>{/if}
</div>

<style>
  .empty { text-align: center; padding: 48px 20px; }
  .title { font-weight: 600; margin-bottom: 6px; }
  .error .title { color: var(--bad); }
  .extra { margin-top: 14px; }
</style>
