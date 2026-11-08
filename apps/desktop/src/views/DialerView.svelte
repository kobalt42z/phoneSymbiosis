<!-- Number entry + keypad. During a call, key presses are sent as DTMF tones. -->
<script lang="ts">
  import * as api from "../lib/api";
  import Icon from "../lib/components/Icon.svelte";
  import Keypad from "../lib/components/Keypad.svelte";
  import { phone } from "../lib/stores/phone.svelte";
  import { ui } from "../lib/stores/ui.svelte";

  let number = $state(ui.dialNumber);
  let busy = $state(false);

  function press(key: string) {
    if (phone.inCall) phone.run(api.sendTones(key));
    number += key;
  }

  async function call() {
    busy = true;
    await phone.run(api.dial(number));
    busy = false;
  }

  function onkeydown(e: KeyboardEvent) {
    if (e.key === "Enter" && number && !phone.inCall) call();
  }
</script>

<h1>Dialer</h1>
<div class="dialer">
  <input class="number" bind:value={number} placeholder="Enter number" {onkeydown} inputmode="tel" />
  {#if phone.inCall}<p class="muted hint">In a call: keys are sent as tones.</p>{/if}
  <Keypad onkey={press} />
  <div class="row actions">
    <button onclick={() => (number = number.slice(0, -1))} disabled={!number}>⌫</button>
    {#if phone.inCall}
      <button class="bad round" onclick={() => phone.run(api.hangUpAll())} title="Hang up"><Icon name="hangup" size={24} /></button>
    {:else}
      <button class="good round" onclick={call} disabled={!number || busy} title="Call"><Icon name="phone" size={24} /></button>
    {/if}
    <button onclick={() => (number = "")} disabled={!number}>Clear</button>
  </div>
</div>

<style>
  .dialer { max-width: 320px; margin: 0 auto; display: flex; flex-direction: column; gap: 18px; }
  .number { font-size: 1.6rem; text-align: center; padding: 10px; }
  .hint { text-align: center; margin: -8px 0 0; }
  .actions { justify-content: center; gap: 18px; }
  .round { width: 64px; height: 64px; border-radius: 50%; display: grid; place-items: center; }
</style>
