// Which view is open, plus cross-view shortcuts ("call this number", "text this number").

export type View = "phone" | "dialer" | "calls" | "contacts" | "history" | "messages" | "devices" | "settings";

class UiStore {
  view = $state<View>("phone");
  /** Number pre-filled in the dialer. */
  dialNumber = $state("");
  /** Conversation to open in Messages. */
  smsPeer = $state<string | null>(null);

  go(view: View) {
    this.view = view;
  }

  openDialer(number = "") {
    this.dialNumber = number;
    this.view = "dialer";
  }

  openConversation(number: string) {
    this.smsPeer = number;
    this.view = "messages";
  }
}

export const ui = new UiStore();
