<!-- Contacts downloaded from the phone (read-only): search, call, text. -->
<script lang="ts">
  import * as api from "../lib/api";
  import EmptyState from "../lib/components/EmptyState.svelte";
  import { initials } from "../lib/format";
  import { phone } from "../lib/stores/phone.svelte";
  import { pim } from "../lib/stores/pim.svelte";
  import { ui } from "../lib/stores/ui.svelte";

  const list = pim.contacts;
  let query = $state("");

  $effect(() => {
    list.load();
  });

  const filtered = $derived.by(() => {
    const q = query.trim().toLowerCase();
    const sorted = [...list.items].sort((a, b) => a.name.localeCompare(b.name));
    if (!q) return sorted;
    return sorted.filter(
      (c) => c.name.toLowerCase().includes(q) || c.numbers.some((n) => n.number.replace(/\s/g, "").includes(q.replace(/\s/g, ""))),
    );
  });
</script>

<div class="row head">
  <h1 class="grow">Contacts</h1>
  <input placeholder="Search" bind:value={query} />
  <button onclick={() => list.load(true)} disabled={list.loading}>{list.loading ? "Loading…" : "Reload"}</button>
</div>

{#if list.error}
  <EmptyState error title="Couldn't load contacts" detail={list.error} />
{:else if list.loading && !list.loaded}
  <EmptyState title="Downloading contacts…" detail="The first download can take a little while." />
{:else if filtered.length === 0}
  <EmptyState title={query ? "No matches" : "No contacts"} detail={query ? "" : "Allow contact sharing for this PC in the iPhone's Bluetooth settings."} />
{:else}
  <ul class="list card">
    {#each filtered as c (c.id)}
      <li>
        {#if c.photo_base64}
          <img class="avatar" src={`data:image/jpeg;base64,${c.photo_base64}`} alt="" />
        {:else}
          <div class="avatar">{initials(c.name)}</div>
        {/if}
        <div class="grow">
          <div class="name">{c.name}</div>
          {#if c.organization}<div class="muted small">{c.organization}</div>{/if}
          {#each c.numbers as n (n.number)}
            <div class="row num">
              <span class="muted">{n.kind ?? "phone"}</span>
              <span class="grow">{n.number}</span>
              <button class="small" onclick={() => phone.run(api.dial(n.number))}>Call</button>
              <button class="small" onclick={() => ui.openConversation(n.number)}>SMS</button>
            </div>
          {/each}
        </div>
      </li>
    {/each}
  </ul>
{/if}

<style>
  .head { margin-bottom: 16px; }
  .head h1 { margin: 0; }
  .list { padding: 0; }
  .list li { align-items: flex-start; }
  .avatar { width: 38px; height: 38px; border-radius: 50%; background: var(--panel-2); display: grid; place-items: center; font-weight: 600; color: var(--muted); object-fit: cover; flex-shrink: 0; }
  .name { font-weight: 600; margin-bottom: 4px; }
  .small { font-size: 0.85em; }
  .num { margin-top: 4px; }
  .num .muted { width: 60px; font-size: 0.85em; }
</style>
