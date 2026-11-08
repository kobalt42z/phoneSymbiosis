<!-- Shown on a PairingConfirm event: the user checks the code matches the phone. -->
<script lang="ts">
  import * as api from "../api";
  import { phone } from "../stores/phone.svelte";

  let { request }: { request: { address: string; passkey: number } } = $props();
  const code = $derived(String(request.passkey).padStart(6, "0"));

  async function reply(accept: boolean) {
    phone.pairing = null;
    await phone.run(api.confirmPairing(request.address, accept));
  }
</script>

<div class="backdrop">
  <div class="card box">
    <h2>Confirm pairing</h2>
    <p class="muted">Make sure this code is shown on your phone ({request.address}), then tap Pair there too.</p>
    <div class="code">{code}</div>
    <div class="row actions">
      <button onclick={() => reply(false)}>Cancel</button>
      <button class="primary" onclick={() => reply(true)}>Codes match</button>
    </div>
  </div>
</div>

<style>
  .backdrop { position: fixed; inset: 0; background: rgba(0, 0, 0, 0.45); display: grid; place-items: center; z-index: 40; }
  .box { width: 380px; box-shadow: var(--shadow); }
  .code { font-size: 2.4rem; letter-spacing: 0.2em; font-weight: 700; text-align: center; margin: 18px 0; font-variant-numeric: tabular-nums; }
  .actions { justify-content: flex-end; }
</style>
