import type { Chat } from "./api";
import { matches } from "./feed";
import type { Shelf } from "./inbox";
import { numbers, shelf } from "./inbox";
import type { Flag } from "./live";

export interface Prefs {
  pinned: string[];
  archived: string[];
  folded: string[];
  opened: string[];
  hidden: boolean;
  width: number;
  settled: Record<string, number>;
  snoozed: Record<string, number>;
  shelves: string[];
  only: string[];
  flat: boolean;
}

export interface Project {
  folder: string;
  name: string;
  place: string;
  chats: Chat[];
  more: number;
}

export interface Sections {
  pinned: Chat[];
  projects: Project[];
  total: number;
}

export type Shelves = Record<Shelf | "pinned", Chat[]> & { total: number };

export interface Place {
  folder: string;
  name: string;
  place: string;
  count: number;
}

export const LIMIT = 5;

export const WIDTH = 272;

export const widen = (value: number) =>
  Math.round(Math.min(420, Math.max(220, value)));

export const blank: Prefs = {
  archived: [],
  flat: false,
  folded: [],
  hidden: false,
  only: [],
  opened: [],
  pinned: [],
  settled: {},
  shelves: [],
  snoozed: {},
  width: WIDTH,
};

const strings = (value: unknown) =>
  Array.isArray(value)
    ? value.filter((item): item is string => typeof item === "string")
    : [];

export const parse = (raw: string | null): Prefs => {
  try {
    const value: unknown = JSON.parse(raw ?? "{}");
    if (typeof value !== "object" || value === null) {
      return blank;
    }
    const field = (name: string): unknown => Reflect.get(value, name);
    return {
      archived: strings(field("archived")),
      flat: field("flat") === true,
      folded: strings(field("folded")),
      hidden: field("hidden") === true,
      only: strings(field("only")),
      opened: strings(field("opened")),
      pinned: strings(field("pinned")),
      settled: numbers(field("settled")),
      shelves: strings(field("shelves")),
      snoozed: numbers(field("snoozed")),
      width:
        typeof field("width") === "number"
          ? widen(Number(field("width")))
          : WIDTH,
    };
  } catch {
    return blank;
  }
};

export const toggle = (list: string[], item: string) =>
  list.includes(item)
    ? list.filter((entry) => entry !== item)
    : [...list, item];

export const keyed = (chat: Chat) => `${chat.agent}:${chat.id}`;

export const short = (ago: string) => ago.replace(/ ago$/u, "");

const visible = (chats: Chat[], prefs: Prefs, query: string) =>
  chats.filter(
    (chat) =>
      !prefs.archived.includes(keyed(chat)) &&
      matches(chat, query) &&
      (prefs.only.length === 0 || prefs.only.includes(chat.folder))
  );

export const shelves = (
  chats: Chat[],
  prefs: Prefs,
  query: string,
  flag: (chat: Chat) => Flag | null,
  now: number
): Shelves => {
  const kept = visible(chats, prefs, query);
  const found: Shelves = {
    inbox: [],
    pinned: [],
    settled: [],
    snoozed: [],
    total: kept.length,
  };
  for (const chat of kept) {
    const key = keyed(chat);
    const place = prefs.pinned.includes(key)
      ? "pinned"
      : shelf(chat, key, prefs, flag(chat), now);
    found[place].push(chat);
  }
  found.snoozed.sort(
    (one, two) =>
      (prefs.snoozed[keyed(one)] ?? 0) - (prefs.snoozed[keyed(two)] ?? 0)
  );
  return found;
};

export const places = (chats: Chat[], prefs: Prefs): Place[] => {
  const found = new Map<string, Place>();
  for (const chat of chats) {
    if (prefs.archived.includes(keyed(chat))) {
      continue;
    }
    const known = found.get(chat.folder);
    found.set(chat.folder, {
      count: (known?.count ?? 0) + 1,
      folder: chat.folder,
      name: known?.name ?? chat.project,
      place: known?.place ?? chat.place,
    });
  }
  return [...found.values()];
};

export const sections = (
  chats: Chat[],
  prefs: Prefs,
  query: string,
  current: string | null
): Sections => {
  const kept = visible(chats, prefs, query);
  const pinned = kept.filter((chat) => prefs.pinned.includes(keyed(chat)));
  const rest = kept.filter((chat) => !prefs.pinned.includes(keyed(chat)));
  const folders = new Map<string, Chat[]>();
  for (const chat of rest) {
    folders.set(chat.folder, [...(folders.get(chat.folder) ?? []), chat]);
  }
  const searching = query.trim() !== "";
  const projects = [...folders.entries()].map(([folder, list]) => {
    const all = searching || prefs.opened.includes(folder);
    const shown = all
      ? list
      : list.filter((chat, index) => index < LIMIT || keyed(chat) === current);
    const [first] = list;
    return {
      chats: shown,
      folder,
      more: list.length - shown.length,
      name: first?.project ?? folder,
      place: first?.place ?? folder,
    };
  });
  return { pinned, projects, total: kept.length };
};
