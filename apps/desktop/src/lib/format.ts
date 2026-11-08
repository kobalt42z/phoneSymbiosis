// Small display helpers.

/** Phone timestamps look like "20261108T110455" (PBAP/MAP). Turn them into a Date. */
export function parseTimestamp(ts: string | null): Date | null {
  if (!ts) return null;
  const m = /^(\d{4})(\d{2})(\d{2})T(\d{2})(\d{2})(\d{2})/.exec(ts);
  if (m) return new Date(+m[1], +m[2] - 1, +m[3], +m[4], +m[5], +m[6]);
  const d = new Date(ts);
  return isNaN(d.getTime()) ? null : d;
}

export function formatTime(ts: string | null): string {
  const d = parseTimestamp(ts);
  if (!d) return "";
  const today = new Date();
  return d.toDateString() === today.toDateString()
    ? d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })
    : d.toLocaleDateString([], { day: "numeric", month: "short" }) +
        " " +
        d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
}

/** Normalise a number for grouping conversations ("+1 (555) 123" → "+1555123"). */
export function normalizeNumber(n: string): string {
  return n.replace(/[^\d+]/g, "");
}

export function initials(name: string): string {
  const parts = name.trim().split(/\s+/).filter(Boolean);
  return ((parts[0]?.[0] ?? "") + (parts[1]?.[0] ?? "")).toUpperCase() || "#";
}
