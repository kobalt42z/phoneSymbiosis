<!-- App settings and a health check ("doctor"). -->
<script lang="ts">
  import { onMount } from "svelte";
  import * as api from "../lib/api";
  import { phone } from "../lib/stores/phone.svelte";
  import type { Check, Settings } from "../lib/types";

  let settings = $state<Settings | null>(null);
  let savedAudio = $state<boolean | null>(null);
  let checks = $state<Check[] | null>(null);
  let checking = $state(false);

  // Load once when the view opens. (An $effect here re-ran on every
  // loading/error change and retried a failing download in a loop.)
  onMount(() => {
    api.getSettings().then((s) => {
      settings = s;
      savedAudio = s.auto_audio;
    });
  });

  async function save() {
    if (settings) await phone.run(api.setSettings($state.snapshot(settings)));
  }

  async function runChecks() {
    checking = true;
    checks = (await phone.run(api.runChecks())) ?? null;
    checking = false;
  }
</script>

<h1>Settings</h1>

{#if settings}
  <div class="card group">
    <label class="row">
      <input type="checkbox" bind:checked={settings.auto_reconnect} onchange={save} />
      <span class="grow">Reconnect to my phone automatically</span>
    </label>
    <label class="row">
      <input type="checkbox" bind:checked={settings.auto_audio} onchange={save} />
      <span class="grow">Use this PC's speakers and microphone for calls automatically</span>
    </label>
    {#if savedAudio !== null && savedAudio !== settings.auto_audio}
      <p class="muted small">The audio setting applies after restarting the app.</p>
    {/if}
    <p class="muted small">Last phone: {settings.last_phone ?? "none"}</p>
  </div>
{/if}

<div class="card group">
  <div class="row">
    <div class="grow">
      <b>Backend</b> <span class="muted">{phone.app.backend}</span>
      <div class="muted small">{phone.app.ready ? "Running" : (phone.app.error ?? "Starting…")}</div>
    </div>
    <button onclick={runChecks} disabled={checking}>{checking ? "Checking…" : "Run checks"}</button>
  </div>
  {#if checks}
    <ul class="list checks">
      {#each checks as c (c.name)}
        <li>
          <span class="badge" class:good={c.ok} class:bad={!c.ok}>{c.ok ? "OK" : "Problem"}</span>
          <b>{c.name}</b>
          <span class="muted grow">{c.detail}</span>
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .group { margin-bottom: 14px; display: flex; flex-direction: column; gap: 10px; }
  label { cursor: pointer; }
  .small { font-size: 0.85em; margin: 0; }
  .checks { margin-top: 6px; }
  .checks li { padding: 8px 0; }
</style>
