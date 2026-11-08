<!-- Call history from the phone: filter by kind, click to call back. -->
<script lang="ts">
  import * as api from "../lib/api";
  import EmptyState from "../lib/components/EmptyState.svelte";
  import { formatTime } from "../lib/format";
  import { phone } from "../lib/stores/phone.svelte";
  import { pim } from "../lib/stores/pim.svelte";
  import type { CallHistoryKind } from "../lib/types";

  const list = pim.history;
  let filter = $state<CallHistoryKind | "all">("all");
  const filters: (CallHistoryKind | "all")[] = ["all", "incoming", "outgoing", "missed"];
  const arrows: Record<CallHistoryKind, string> = { incoming: "↙", outgoing: "↗", missed: "✕" };

  $effect(() => {
    list.load();
    pim.contacts.load(); // for names
  });

  const shown = $derived(
    list.items
      .filter((e) => filter === "all" || e.kind === filter)
      .sort((a, b) => (b.timestamp ?? "").localeCompare(a.timestamp ?? "")),
  );
</script>

<div class="row head">
  <h1 class="grow">History</h1>
  <div class="tabs">
    {#each filters as f (f)}
      <button class="small" class:on={filter === f} onclick={() => (filter = f)}>{f}</button>
    {/each}
  </div>
  <button onclick={() => list.load(true)} disabled={list.loading}>{list.loading ? "Loading…" : "Reload"}</button>
</div>

{#if list.error}
  <EmptyState error title="Couldn't load call history" detail={list.error} />
{:else if list.loading && !list.loaded}
  <EmptyState title="Downloading call history…" />
{:else if shown.length === 0}
  <EmptyState title="No calls" />
{:else}
  <ul class="list card">
    {#each shown as e, i (i)}
      <li>
        <span class="kind {e.kind}" title={e.kind}>{arrows[e.kind]}</span>
        <div class="grow">
          <div class:missed={e.kind === "missed"}>{e.name ?? pim.nameFor(e.number) ?? (e.number || "Unknown")}</div>
          <div class="muted small">{e.number}</div>
        </div>
        <span class="muted small">{formatTime(e.timestamp)}</span>
        <button class="small" onclick={() => phone.run(api.dial(e.number))} disabled={!e.number}>Call</button>
      </li>
    {/each}
  </ul>
{/if}

<style>
  .head { margin-bottom: 16px; }
  .head h1 { margin: 0; }
  .tabs { display: flex; gap: 4px; }
  .tabs button { text-transform: capitalize; }
  .tabs .on { background: var(--accent); color: var(--accent-text); border-color: transparent; }
  .list { padding: 0; }
  .kind { width: 20px; text-align: center; color: var(--muted); }
  .kind.missed, .missed { color: var(--bad); }
  .small { font-size: 0.85em; }
</style>
