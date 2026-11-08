<!-- Shown while the backend starts, or when it failed to start ("doctor" screen). -->
<script lang="ts">
  import * as api from "../api";
  import { phone } from "../stores/phone.svelte";
  import { ui } from "../stores/ui.svelte";

  let retrying = $state(false);

  async function retry() {
    retrying = true;
    const s = await phone.run(api.retryInit());
    if (s) phone.app = s;
    retrying = false;
  }
</script>

<div class="wrap">
  {#if phone.app.starting}
    <h1>Starting phone services…</h1>
    <p class="muted">Connecting to Bluetooth and audio.</p>
  {:else}
    <h1>Phone services could not start</h1>
    <div class="card err">{phone.app.error}</div>
    <h2>Things to check</h2>
    <ul class="muted">
      <li>Bluetooth is turned on and an adapter is present.</li>
      <li>The <code>bluez</code>, <code>bluez-obexd</code>, <code>pipewire</code> and <code>wireplumber</code> services are installed and running.</li>
      <li>You can also run <code>phonesym doctor</code> in a terminal for details.</li>
    </ul>
    <div class="row">
      <button class="primary" onclick={retry} disabled={retrying}>{retrying ? "Retrying…" : "Retry"}</button>
      <button onclick={() => ui.go("settings")}>Settings</button>
    </div>
  {/if}
  <p class="muted small">Backend: {phone.app.backend || "unknown"}</p>
</div>

<style>
  .wrap { max-width: 620px; }
  .err { border-left: 4px solid var(--bad); margin-bottom: 20px; white-space: pre-wrap; }
  li { margin: 4px 0; }
  .small { font-size: 0.85em; margin-top: 24px; }
</style>
