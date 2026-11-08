<!-- Active calls: answer / hang up / hold-swap / merge, mic mute, volume, DTMF. -->
<script lang="ts">
  import * as api from "../lib/api";
  import EmptyState from "../lib/components/EmptyState.svelte";
  import Icon from "../lib/components/Icon.svelte";
  import Keypad from "../lib/components/Keypad.svelte";
  import { phone } from "../lib/stores/phone.svelte";
  import { ui } from "../lib/stores/ui.svelte";
  import type { CallState } from "../lib/types";

  const labels: Record<CallState, string> = {
    incoming: "Ringing", waiting: "Waiting", dialing: "Dialing…", alerting: "Ringing…",
    active: "Active", held: "On hold", disconnected: "Ended",
  };

  let showPad = $state(false);
  const held = $derived(phone.calls.some((c) => c.state === "held"));
  const active = $derived(phone.calls.some((c) => c.state === "active"));
  const waiting = $derived(phone.calls.some((c) => c.state === "waiting"));

  // Local copy of the volume for the sliders; sent on change.
  let speaker = $state(phone.volume?.speaker ?? 10);
  let mic = $state(phone.volume?.microphone ?? 10);
  $effect(() => {
    if (phone.volume) ({ speaker, microphone: mic } = phone.volume);
  });
  const sendVolume = () => phone.run(api.setVolume({ speaker, microphone: mic }));

  async function toggleMute() {
    const next = !phone.micMuted;
    try {
      await api.setMicMuted(next);
      phone.micMuted = next;
    } catch (e) {
      phone.toast(api.errorText(e), "error");
    }
  }
</script>

<h1>Calls</h1>

{#if phone.calls.length === 0}
  <EmptyState title="No active calls" detail="Calls you make or receive show up here.">
    <button class="primary" onclick={() => ui.go("dialer")}>Open dialer</button>
  </EmptyState>
{:else}
  <ul class="list card calls">
    {#each phone.calls as c (c.id)}
      <li>
        <div class="grow">
          <div class="who">{c.name ?? (c.number || "Unknown")}</div>
          <div class="muted">{c.name ? c.number + " · " : ""}{labels[c.state]}{c.multiparty ? " · conference" : ""}</div>
        </div>
        {#if c.state === "incoming"}
          <button class="good" onclick={() => phone.run(api.answer(c.id))}>Answer</button>
        {/if}
        <button class="bad" onclick={() => phone.run(api.hangUp(c.id))}>Hang up</button>
      </li>
    {/each}
  </ul>

  <div class="row controls">
    {#if waiting}
      <button onclick={() => phone.run(api.holdAndAnswer())}>Hold & answer</button>
      <button onclick={() => phone.run(api.releaseAndAnswer())}>End & answer</button>
    {/if}
    <button onclick={() => phone.run(api.swapCalls())} disabled={!(held || active)}>Hold / swap</button>
    <button onclick={() => phone.run(api.mergeCalls())} disabled={!(held && active)}>Merge</button>
    <button onclick={toggleMute} class:bad={phone.micMuted}>
      <Icon name={phone.micMuted ? "micOff" : "mic"} size={16} /> {phone.micMuted ? "Unmute" : "Mute"}
    </button>
    <button onclick={() => (showPad = !showPad)}>Keypad</button>
    <button onclick={() => phone.run(phone.audioActive ? api.audioStop() : api.audioStart())}>
      {phone.audioActive ? "Audio to phone" : "Audio to PC"}
    </button>
    <button class="bad" onclick={() => phone.run(api.hangUpAll())}>End all</button>
  </div>

  <div class="card volume">
    <label>Speaker <input type="range" min="0" max="15" bind:value={speaker} onchange={sendVolume} /></label>
    <label>Microphone <input type="range" min="0" max="15" bind:value={mic} onchange={sendVolume} /></label>
  </div>

  {#if showPad}
    <div class="pad"><Keypad onkey={(k) => phone.run(api.sendTones(k))} /></div>
  {/if}
{/if}

<style>
  .calls { padding: 0; margin-bottom: 14px; }
  .who { font-weight: 600; }
  .controls { flex-wrap: wrap; margin-bottom: 14px; }
  .controls button { display: inline-flex; align-items: center; gap: 6px; }
  .volume { display: flex; gap: 24px; flex-wrap: wrap; margin-bottom: 14px; }
  .volume label { display: flex; align-items: center; gap: 10px; }
  .pad { margin-top: 10px; }
</style>
