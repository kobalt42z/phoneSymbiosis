<!-- Short-lived messages in the bottom-right corner (errors and warnings). -->
<script lang="ts">
  import { phone } from "../stores/phone.svelte";
</script>

<div class="toasts">
  {#each phone.toasts as t (t.id)}
    <div class="toast" class:error={t.kind === "error"}>{t.text}</div>
  {/each}
</div>

<style>
  .toasts { position: fixed; right: 16px; bottom: 16px; display: flex; flex-direction: column; gap: 8px; z-index: 60; max-width: 420px; }
  .toast { background: var(--panel); border: 1px solid var(--border); border-left: 4px solid var(--accent); border-radius: 8px; padding: 10px 14px; box-shadow: var(--shadow); }
  .toast.error { border-left-color: var(--bad); }
</style>
