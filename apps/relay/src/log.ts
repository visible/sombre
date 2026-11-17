export interface Device {
  id: string;
  label: string;
  since: number;
}

export interface Log {
  base: number;
  items: string[];
}

export const KEPT = 4000;
export const ROW = 512 * 1024;
export const LARGEST = 7_500_000;

const parsed = (text: string | null | undefined): unknown => {
  try {
    return JSON.parse(text ?? "null");
  } catch {
    return null;
  }
};

export const empty = (): Log => ({ base: 0, items: [] });

export const total = (log: Log) => log.base + log.items.length;

export const since = (log: Log, after: number) => {
  const end = total(log);
  const start = Math.max(0, Math.min(after, end) - log.base);
  return { items: log.items.slice(start), next: end };
};

export const append = (log: Log, fresh: string[], kept = KEPT): Log => {
  const items = [...log.items, ...fresh];
  const extra = Math.max(0, items.length - kept);
  return { base: log.base + extra, items: items.slice(extra) };
};

export const pack = (items: string[], cap = ROW): string[][] => {
  const rows: string[][] = [];
  let row: string[] = [];
  let size = 0;
  for (const whole of items) {
    const item = whole.length > LARGEST ? "" : whole;
    if (row.length > 0 && size + item.length > cap) {
      rows.push(row);
      row = [];
      size = 0;
    }
    row.push(item);
    size += item.length;
  }
  if (row.length > 0) {
    rows.push(row);
  }
  return rows;
};

export const rebuild = (
  rows: { first: number; items: string[] }[],
  start: number,
  kept = KEPT
): Log => {
  const ordered = rows.toSorted((one, two) => one.first - two.first);
  const items = ordered.flatMap((row) => row.items);
  return append({ base: ordered[0]?.first ?? start, items: [] }, items, kept);
};

export const room = (path: string) => {
  const groups = /^\/r\/(?<id>[0-9a-f]{24})(?:\/(?<action>[a-z]+))?\/?$/u.exec(
    path
  )?.groups;
  return groups?.id ? { action: groups.action ?? "", id: groups.id } : null;
};

export interface Frame {
  fresh: boolean;
  out: string[];
  read: number | null;
}

export const cursor = (value: unknown, fallback: number) => {
  const number =
    typeof value === "string" && /^\d+$/u.test(value) ? Number(value) : value;
  return typeof number === "number" &&
    Number.isSafeInteger(number) &&
    number >= 0
    ? number
    : fallback;
};

export const frame = (text: string): Frame => {
  const value = parsed(text);
  if (typeof value !== "object" || value === null) {
    return { fresh: false, out: [], read: null };
  }
  const out: unknown = Reflect.get(value, "out");
  const read = cursor(Reflect.get(value, "read"), -1);
  return {
    fresh: Reflect.get(value, "fresh") === true,
    out: (Array.isArray(out) ? out : [out]).filter(
      (item): item is string => typeof item === "string" && item.length > 0
    ),
    read: read === -1 ? null : read,
  };
};

export const hex = (bytes: number) =>
  [...crypto.getRandomValues(new Uint8Array(bytes))]
    .map((byte) => byte.toString(16).padStart(2, "0"))
    .join("");

export const strings = (text: string | null | undefined): string[] => {
  const value = parsed(text);
  return Array.isArray(value)
    ? value.filter((item): item is string => typeof item === "string")
    : [];
};

const machine = (value: unknown): value is Device =>
  typeof value === "object" &&
  value !== null &&
  typeof Reflect.get(value, "id") === "string" &&
  typeof Reflect.get(value, "label") === "string" &&
  typeof Reflect.get(value, "since") === "number";

export const machines = (text: string | null | undefined): Device[] => {
  const value = parsed(text);
  return Array.isArray(value) ? value.filter(machine) : [];
};

const kinds = [
  ["iPhone", "iPhone"],
  ["iPad", "iPad"],
  ["Android", "Android"],
  ["Macintosh", "Mac"],
  ["Windows", "Windows"],
] as const;

const browsers = [
  ["CriOS", "Chrome"],
  ["FxiOS", "Firefox"],
  ["Edg", "Edge"],
  ["Chrome", "Chrome"],
  ["Firefox", "Firefox"],
  ["Safari", "Safari"],
] as const;

export const label = (agent: string) => {
  const kind =
    kinds.find(([needle]) => agent.includes(needle))?.[1] ?? "browser";
  const browser = browsers.find(([needle]) => agent.includes(needle))?.[1];
  return browser ? `${kind}, ${browser}` : kind;
};

export const MINUTES = 10 * 60 * 1000;
export const DAY = 24 * 60 * 60 * 1000;
export const BURST = 10;
export const DAILY = 50;

export const allowed = (stamps: number[], now: number) =>
  stamps.filter((at) => now - at < MINUTES).length < BURST &&
  stamps.filter((at) => now - at < DAY).length < DAILY;

export const network = (address: string) =>
  address.includes(":")
    ? address.split(":").slice(0, 4).join(":").toLowerCase()
    : address;
