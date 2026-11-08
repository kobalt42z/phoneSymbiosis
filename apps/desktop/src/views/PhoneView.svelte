<!-- Overview: which phone is connected, battery / signal, audio route. -->
<script lang="ts">
  import Icon from "../lib/components/Icon.svelte";
  import { phone } from "../lib/stores/phone.svelte";
  import { ui } from "../lib/stores/ui.svelte";

  const d = $derived(phone.connected);
  const s = $derived(phone.status);
  const ready = $derived(d ? phone.callsAvailable.includes(d.address) || (d.capabilities.calls && d.connected) : false);
</script>

<h1>Phone</h1>

{#if !d}
  <div class="card">
    <h2>No phone connected</h2>
    <p class="muted">Pair your iPhone once in Devices. After that it reconnects automatically when it is nearby.</p>
    <button class="primary" onclick={() => ui.go("devices")}>Open Devices</button>
  </div>
{:else}
  <div class="card main">
    <div class="name">{d.name}</div>
    <div class="muted">{d.address}</div>
    <div class="row tags">
      <span class="badge good">Connected</span>
      <span class="badge" class:good={d.capabilities.calls}>Calls</span>
      <span class="badge" class:good={d.capabilities.contacts}>Contacts</span>
      <span class="badge" class:good={d.capabilities.messages}>Messages</span>
    </div>
  </div>

  <div class="stats">
    <div class="card stat">
      <Icon name="battery" /><span class="muted">Battery</span>
      <b>{s?.battery != null ? `${s.battery}%` : "—"}</b>
    </div>
    <div class="card stat">
      <Icon name="signal" /><span class="muted">Signal</span>
      <b>{s?.signal != null ? `${s.signal}/5` : "—"}</b>
    </div>
    <div class="card stat">
      <span class="muted">Network</span>
      <b>{s?.operator ?? "—"}{s?.roaming ? " (roaming)" : ""}</b>
    </div>
    <div class="card stat">
      <span class="muted">Call audio</span>
      <b>{phone.audioActive ? "On this PC" : "Idle"}</b>
    </div>
  </div>

  <div class="row">
    <button class="primary" onclick={() => ui.go("dialer")} disabled={!ready}>Make a call</button>
    <button onclick={() => ui.go("messages")}>Messages</button>
    <button onclick={() => phone.refresh()}><Icon name="refresh" size={14} /> Refresh</button>
  </div>
{/if}

<style>
  .main { margin-bottom: 16px; }
  .name { font-size: 1.3rem; font-weight: 600; }
  .tags { margin-top: 10px; flex-wrap: wrap; }
  .stats { display: grid; grid-template-columns: repeat(auto-fill, minmax(160px, 1fr)); gap: 12px; margin-bottom: 18px; }
  .stat { display: flex; flex-direction: column; gap: 4px; }
  .stat b { font-size: 1.2rem; }
  button { display: inline-flex; align-items: center; gap: 6px; }
</style>
