<!-- Full-window overlay for a ringing call with big Answer / Decline buttons. -->
<script lang="ts">
  import * as api from "../api";
  import { phone } from "../stores/phone.svelte";
  import type { Call } from "../types";
  import Icon from "./Icon.svelte";

  let { call }: { call: Call } = $props();
  // If another call is active, answering puts it on hold.
  const otherActive = $derived(phone.calls.some((c) => c.id !== call.id && c.state === "active"));

  const answer = () => phone.run(otherActive ? api.holdAndAnswer() : api.answer(call.id));
  const decline = () => phone.run(api.hangUp(call.id));
</script>

<div class="backdrop">
  <div class="box">
    <div class="label muted">Incoming call</div>
    <div class="who">{call.name ?? (call.number || "Unknown")}</div>
    {#if call.name}<div class="muted">{call.number}</div>{/if}
    <div class="actions">
      <button class="bad big" onclick={decline}><Icon name="hangup" size={28} /><span>Decline</span></button>
      <button class="good big" onclick={answer}><Icon name="phone" size={28} /><span>Answer</span></button>
    </div>
  </div>
</div>

<style>
  .backdrop { position: fixed; inset: 0; background: rgba(0, 0, 0, 0.55); display: grid; place-items: center; z-index: 50; }
  .box { background: var(--panel); border-radius: 18px; padding: 32px 40px; text-align: center; box-shadow: var(--shadow); min-width: 340px; }
  .label { text-transform: uppercase; letter-spacing: 0.08em; font-size: 0.8em; }
  .who { font-size: 1.8rem; font-weight: 600; margin: 10px 0 4px; }
  .actions { display: flex; gap: 32px; justify-content: center; margin-top: 28px; }
  .big { display: flex; flex-direction: column; align-items: center; gap: 6px; width: 110px; padding: 16px; border-radius: 16px; font-weight: 600; }
</style>
