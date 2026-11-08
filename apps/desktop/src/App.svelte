<!-- Root layout: sidebar + current view, plus overlays (incoming call, pairing, toasts). -->
<script lang="ts">
  import { onMount } from "svelte";
  import { phone } from "./lib/stores/phone.svelte";
  import { ui } from "./lib/stores/ui.svelte";
  import Sidebar from "./lib/components/Sidebar.svelte";
  import IncomingCall from "./lib/components/IncomingCall.svelte";
  import PairingDialog from "./lib/components/PairingDialog.svelte";
  import Toasts from "./lib/components/Toasts.svelte";
  import SetupScreen from "./lib/components/SetupScreen.svelte";
  import PhoneView from "./views/PhoneView.svelte";
  import DialerView from "./views/DialerView.svelte";
  import CallsView from "./views/CallsView.svelte";
  import ContactsView from "./views/ContactsView.svelte";
  import HistoryView from "./views/HistoryView.svelte";
  import MessagesView from "./views/MessagesView.svelte";
  import DevicesView from "./views/DevicesView.svelte";
  import SettingsView from "./views/SettingsView.svelte";

  onMount(() => {
    phone.start().catch((e) => phone.toast(String(e), "error"));
  });

  // Jump to the calls panel when a call starts.
  let hadCall = false;
  $effect(() => {
    if (phone.inCall && !hadCall) ui.go("calls");
    hadCall = phone.inCall;
  });
</script>

<div class="layout">
  <Sidebar />
  <main>
    {#if !phone.app.ready && ui.view !== "settings"}
      <SetupScreen />
    {:else if ui.view === "phone"}
      <PhoneView />
    {:else if ui.view === "dialer"}
      <DialerView />
    {:else if ui.view === "calls"}
      <CallsView />
    {:else if ui.view === "contacts"}
      <ContactsView />
    {:else if ui.view === "history"}
      <HistoryView />
    {:else if ui.view === "messages"}
      <MessagesView />
    {:else if ui.view === "devices"}
      <DevicesView />
    {:else if ui.view === "settings"}
      <SettingsView />
    {/if}
  </main>
</div>

{#if phone.ringing}
  <IncomingCall call={phone.ringing} />
{/if}
{#if phone.pairing}
  <PairingDialog request={phone.pairing} />
{/if}
<Toasts />

<style>
  .layout { display: flex; height: 100%; }
  main { flex: 1; overflow: auto; padding: 24px 28px; min-width: 0; }
</style>
