<!-- Left navigation with the connected phone summary at the top. -->
<script lang="ts">
  import Icon from "./Icon.svelte";
  import { phone } from "../stores/phone.svelte";
  import { ui, type View } from "../stores/ui.svelte";

  const items: { view: View; label: string; icon: string }[] = [
    { view: "phone", label: "Phone", icon: "phone" },
    { view: "dialer", label: "Dialer", icon: "grid" },
    { view: "calls", label: "Calls", icon: "call" },
    { view: "contacts", label: "Contacts", icon: "users" },
    { view: "history", label: "History", icon: "clock" },
    { view: "messages", label: "Messages", icon: "message" },
    { view: "devices", label: "Devices", icon: "bluetooth" },
    { view: "settings", label: "Settings", icon: "settings" },
  ];
</script>

<nav>
  <div class="brand">PhoneSymbiosis</div>
  <div class="device">
    <span class="dot" class:on={!!phone.connected}></span>
    <span class="name">{phone.connected?.name ?? "No phone connected"}</span>
  </div>
  {#each items as it (it.view)}
    <button class="item" class:active={ui.view === it.view} onclick={() => ui.go(it.view)}>
      <Icon name={it.icon} />
      <span>{it.label}</span>
      {#if it.view === "calls" && phone.calls.length > 0}
        <span class="count">{phone.calls.length}</span>
      {/if}
    </button>
  {/each}
</nav>

<style>
  nav {
    width: 210px;
    flex-shrink: 0;
    background: var(--panel);
    border-right: 1px solid var(--border);
    padding: 16px 10px;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .brand { font-weight: 700; padding: 0 10px 12px; }
  .device { display: flex; align-items: center; gap: 8px; padding: 0 10px 16px; color: var(--muted); font-size: 0.9em; }
  .name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .dot { width: 8px; height: 8px; border-radius: 50%; background: var(--muted); flex-shrink: 0; }
  .dot.on { background: var(--good); }
  .item { display: flex; align-items: center; gap: 10px; border: none; background: none; text-align: left; padding: 9px 10px; }
  .item.active { background: color-mix(in srgb, var(--accent) 14%, transparent); color: var(--accent); }
  .count { margin-left: auto; background: var(--good); color: #fff; border-radius: 999px; font-size: 0.75em; padding: 1px 7px; }
</style>
