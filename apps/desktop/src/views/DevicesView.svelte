<!-- Bluetooth adapter + devices: scan, pair, connect, disconnect, forget. -->
<script lang="ts">
  import { onMount } from "svelte";
  import * as api from "../lib/api";
  import EmptyState from "../lib/components/EmptyState.svelte";
  import { phone } from "../lib/stores/phone.svelte";
  import type { AdapterInfo, Device } from "../lib/types";

  let adapter = $state<AdapterInfo | null>(null);
  let adapterError = $state<string | null>(null);
  let scanning = $state(false);
  /** Address of the device with an action in progress. */
  let busy = $state<string | null>(null);

  async function loadAdapter() {
    try {
      adapter = await api.adapter();
      adapterError = null;
    } catch (e) {
      adapterError = api.errorText(e);
    }
  }

  // Load once when the view opens. (An $effect here re-ran on every
  // loading/error change and retried a failing download in a loop.)
  onMount(() => {
    loadAdapter();
    phone.run(api.devices()).then((d) => d && (phone.devices = d));
  });

  async function scan() {
    scanning = true;
    const found = await phone.run(api.scan(8));
    if (found) phone.devices = found;
    scanning = false;
  }

  async function act(d: Device, fn: (a: string) => Promise<void>) {
    busy = d.address;
    await phone.run(fn(d.address));
    const list = await phone.run(api.devices());
    if (list) phone.devices = list;
    busy = null;
  }

  async function togglePower() {
    if (!adapter) return;
    await phone.run(api.setPowered(!adapter.powered));
    await loadAdapter();
  }

  // Paired first, then phones (devices with call support), then by signal.
  const sorted = $derived(
    [...phone.devices].sort(
      (a, b) => +b.paired - +a.paired || +b.capabilities.calls - +a.capabilities.calls || (b.rssi ?? -999) - (a.rssi ?? -999),
    ),
  );
</script>

<div class="row head">
  <h1 class="grow">Devices</h1>
  <button class="primary" onclick={scan} disabled={scanning}>{scanning ? "Scanning…" : "Scan"}</button>
</div>

<div class="card adapter row">
  {#if adapter}
    <div class="grow">
      <b>{adapter.name}</b> <span class="muted">{adapter.address}</span>
      <div class="muted small">Bluetooth is {adapter.powered ? "on" : "off"}</div>
    </div>
    <button onclick={togglePower}>{adapter.powered ? "Turn off" : "Turn on"}</button>
  {:else}
    <div class="grow muted">{adapterError ?? "Loading adapter…"}</div>
  {/if}
</div>

{#if sorted.length === 0}
  <EmptyState title="No devices yet" detail="On your iPhone open Settings → Bluetooth, keep it open, then press Scan." />
{:else}
  <ul class="list card">
    {#each sorted as d (d.address)}
      <li>
        <div class="grow">
          <div><b>{d.name || d.address}</b></div>
          <div class="muted small">
            {d.address}{d.rssi != null ? ` · ${d.rssi} dBm` : ""}
          </div>
        </div>
        {#if d.connected}<span class="badge good">Connected</span>{:else if d.paired}<span class="badge">Paired</span>{/if}
        {#if !d.paired}
          <button class="small primary" disabled={busy === d.address} onclick={() => act(d, api.pair)}>Pair</button>
        {:else if d.connected}
          <button class="small" disabled={busy === d.address} onclick={() => act(d, api.disconnect)}>Disconnect</button>
        {:else}
          <button class="small primary" disabled={busy === d.address} onclick={() => act(d, api.connect)}>Connect</button>
        {/if}
        {#if d.paired}
          <button class="small link" disabled={busy === d.address} onclick={() => confirm(`Forget ${d.name}?`) && act(d, api.forget)}>Forget</button>
        {/if}
      </li>
    {/each}
  </ul>
{/if}

<style>
  .head { margin-bottom: 16px; }
  .head h1 { margin: 0; }
  .adapter { margin-bottom: 14px; }
  .list { padding: 0; }
  .small { font-size: 0.85em; }
</style>
