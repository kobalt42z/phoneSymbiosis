<!-- SMS: conversations grouped by number, thread view, send box. -->
<script lang="ts">
  import * as api from "../lib/api";
  import EmptyState from "../lib/components/EmptyState.svelte";
  import { formatTime, normalizeNumber } from "../lib/format";
  import { phone } from "../lib/stores/phone.svelte";
  import { pim } from "../lib/stores/pim.svelte";
  import { ui } from "../lib/stores/ui.svelte";
  import type { Message } from "../lib/types";

  const list = pim.messages;
  /** Messages we sent from this app in this session (shown immediately). */
  let sentLocally = $state<Message[]>([]);
  let draft = $state("");
  let newNumber = $state("");
  let sending = $state(false);

  $effect(() => {
    list.load();
    pim.contacts.load();
  });

  interface Conversation { key: string; number: string; name: string; messages: Message[]; last: Message }

  // Merge downloaded, live-received and locally-sent messages, grouped by number.
  const conversations = $derived.by(() => {
    const seen = new Set<string>();
    const groups = new Map<string, Message[]>();
    for (const m of [...list.items, ...phone.incomingMessages, ...sentLocally]) {
      const id = `${m.folder}:${m.id}`;
      if (m.id && seen.has(id)) continue;
      seen.add(id);
      const key = normalizeNumber(m.peer_number);
      groups.set(key, [...(groups.get(key) ?? []), m]);
    }
    const out: Conversation[] = [];
    for (const [key, msgs] of groups) {
      msgs.sort((a, b) => (a.timestamp ?? "").localeCompare(b.timestamp ?? ""));
      const last = msgs[msgs.length - 1];
      const name = msgs.find((m) => m.peer_name)?.peer_name ?? pim.nameFor(last.peer_number) ?? last.peer_number;
      out.push({ key, number: last.peer_number, name, messages: msgs, last });
    }
    return out.sort((a, b) => (b.last.timestamp ?? "").localeCompare(a.last.timestamp ?? ""));
  });

  // The open conversation: chosen in the list, or set from Contacts ("SMS" button).
  const openKey = $derived(ui.smsPeer ? normalizeNumber(ui.smsPeer) : null);
  const open = $derived(conversations.find((c) => c.key === openKey) ?? null);

  function timestampNow(): string {
    const d = new Date();
    const p = (n: number) => String(n).padStart(2, "0");
    return `${d.getFullYear()}${p(d.getMonth() + 1)}${p(d.getDate())}T${p(d.getHours())}${p(d.getMinutes())}${p(d.getSeconds())}`;
  }

  async function send() {
    const to = ui.smsPeer;
    const text = draft.trim();
    if (!to || !text) return;
    sending = true;
    try {
      await api.sendMessage(to, text);
      sentLocally = [...sentLocally, {
        id: "", folder: "sent", peer_number: to, peer_name: null, text, timestamp: timestampNow(), read: true,
      }];
      draft = "";
    } catch (e) {
      phone.toast(api.errorText(e), "error");
    } finally {
      sending = false;
    }
  }

  function startNew() {
    if (newNumber.trim()) ui.smsPeer = newNumber.trim();
    newNumber = "";
  }

  function onkeydown(e: KeyboardEvent) {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      send();
    }
  }
</script>

<div class="row head">
  <h1 class="grow">Messages</h1>
  <button onclick={() => list.load(true)} disabled={list.loading}>{list.loading ? "Loading…" : "Reload"}</button>
</div>

<div class="split">
  <aside class="card convs">
    <form class="row new" onsubmit={(e) => { e.preventDefault(); startNew(); }}>
      <input class="grow" placeholder="New message to…" bind:value={newNumber} inputmode="tel" />
      <button class="small" type="submit" disabled={!newNumber.trim()}>Start</button>
    </form>
    {#if list.error}
      <EmptyState error title="Couldn't load messages" detail={list.error} />
    {:else if list.loading && !list.loaded}
      <EmptyState title="Downloading messages…" />
    {:else if conversations.length === 0}
      <EmptyState title="No messages" detail="Allow notifications sharing for this PC in the iPhone's Bluetooth settings." />
    {/if}
    <ul class="list">
      {#each conversations as c (c.key)}
        <li class:active={c.key === openKey}>
          <button class="conv" onclick={() => (ui.smsPeer = c.number)}>
            <div class="row"><b class="grow clip">{c.name}</b><span class="muted small">{formatTime(c.last.timestamp)}</span></div>
            <div class="muted small clip">{c.last.text}</div>
          </button>
        </li>
      {/each}
    </ul>
  </aside>

  <section class="card thread">
    {#if !ui.smsPeer}
      <EmptyState title="Pick a conversation" />
    {:else}
      <div class="row thread-head">
        <b class="grow">{open?.name ?? pim.nameFor(ui.smsPeer) ?? ui.smsPeer}</b>
        <button class="small" onclick={() => ui.smsPeer && phone.run(api.dial(ui.smsPeer))}>Call</button>
      </div>
      <div class="bubbles">
        {#each open?.messages ?? [] as m, i (i)}
          <div class="bubble" class:mine={m.folder !== "inbox"}>
            <div>{m.text}</div>
            <div class="time">{formatTime(m.timestamp)}</div>
          </div>
        {/each}
      </div>
      <div class="row compose">
        <textarea class="grow" rows="2" placeholder="Text message" bind:value={draft} {onkeydown}></textarea>
        <button class="primary" onclick={send} disabled={!draft.trim() || sending}>{sending ? "Sending…" : "Send"}</button>
      </div>
    {/if}
  </section>
</div>

<style>
  .head { margin-bottom: 16px; }
  .head h1 { margin: 0; }
  .split { display: grid; grid-template-columns: 280px 1fr; gap: 14px; height: calc(100vh - 120px); min-height: 360px; }
  .convs { padding: 0; overflow: auto; }
  .new { padding: 10px; border-bottom: 1px solid var(--border); }
  .convs li { padding: 0; }
  .conv { width: 100%; min-width: 0; text-align: left; border: none; background: none; border-radius: 0; padding: 10px 12px; }
  .convs li.active { background: color-mix(in srgb, var(--accent) 12%, transparent); }
  .clip { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; min-width: 0; }
  .small { font-size: 0.85em; }
  .thread { display: flex; flex-direction: column; padding: 0; min-width: 0; }
  .thread-head { padding: 12px 14px; border-bottom: 1px solid var(--border); }
  .bubbles { flex: 1; overflow: auto; padding: 14px; display: flex; flex-direction: column; gap: 8px; }
  .bubble { max-width: 70%; background: var(--panel-2); padding: 8px 12px; border-radius: 14px; white-space: pre-wrap; align-self: flex-start; }
  .bubble.mine { align-self: flex-end; background: var(--accent); color: var(--accent-text); }
  .time { font-size: 0.7em; opacity: 0.7; margin-top: 2px; }
  .compose { padding: 10px; border-top: 1px solid var(--border); }
  textarea { resize: none; }
</style>
