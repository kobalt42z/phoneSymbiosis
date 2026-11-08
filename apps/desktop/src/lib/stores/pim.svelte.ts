// Cached contacts, call history and messages, downloaded from the phone on demand.
// Downloads can be slow (PBAP/MAP), so views share this cache instead of re-fetching.

import * as api from "../api";
import { normalizeNumber } from "../format";
import type { CallHistoryEntry, Contact, Message } from "../types";

/** One loadable list with its loading / error state. */
class Loadable<T> {
  items = $state<T[]>([]);
  loading = $state(false);
  error = $state<string | null>(null);
  loaded = $state(false);

  constructor(private fetch: () => Promise<T[]>) {}

  async load(force = false) {
    if (this.loading || (this.loaded && !force)) return;
    this.loading = true;
    this.error = null;
    try {
      this.items = await this.fetch();
      this.loaded = true;
    } catch (e) {
      this.error = api.errorText(e);
    } finally {
      this.loading = false;
    }
  }
}

async function allMessages(): Promise<Message[]> {
  // Inbox is required; sent is optional (some phones don't expose it).
  const [inbox, sent] = await Promise.allSettled([api.listMessages("inbox"), api.listMessages("sent")]);
  if (inbox.status === "rejected") throw inbox.reason;
  return [...inbox.value, ...(sent.status === "fulfilled" ? sent.value : [])];
}

class PimStore {
  contacts = new Loadable<Contact>(() => api.contacts());
  history = new Loadable<CallHistoryEntry>(() => api.callHistory());
  messages = new Loadable<Message>(allMessages);

  /** number (normalised) → contact name, for showing names in history/messages. */
  names = $derived.by(() => {
    const m = new Map<string, string>();
    for (const c of this.contacts.items)
      for (const n of c.numbers) m.set(normalizeNumber(n.number), c.name);
    return m;
  });

  nameFor(number: string): string | undefined {
    return this.names.get(normalizeNumber(number));
  }
}

export const pim = new PimStore();
