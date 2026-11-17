import type { Agent, Chat, Entry, Folder } from "./api";
import type { Scope } from "./patch";

export type Group =
  | { kind: "work"; key: number; steps: (Entry & { kind: "step" })[] }
  | { kind: "single"; key: number; entry: Entry };

export const group = (entries: Entry[]): Group[] => {
  const groups: Group[] = [];
  for (const entry of entries) {
    const last = groups.at(-1);
    if (entry.kind !== "step") {
      groups.push({ entry, key: entry.id, kind: "single" });
    } else if (last?.kind === "work") {
      last.steps.push(entry);
    } else {
      groups.push({ key: entry.id, kind: "work", steps: [entry] });
    }
  }
  return groups;
};

export const recent = (entries: Entry[]): Scope | null => {
  const found = entries.findLast((entry) => entry.kind === "changes");
  return found?.kind === "changes"
    ? { from: found.from, kind: "turn", to: found.to }
    : null;
};

export const rounds = (entries: Entry[]) =>
  entries.filter((entry) => entry.kind === "changes" || entry.kind === "done")
    .length;

const kinds: Record<string, [string, string, string]> = {
  Edit: ["edited", "file", "files"],
  Find: ["searched", "time", "times"],
  List: ["listed", "folder", "folders"],
  Read: ["read", "file", "files"],
  Run: ["ran", "command", "commands"],
  Test: ["ran", "check", "checks"],
  View: ["viewed", "image", "images"],
  Web: ["looked up", "page", "pages"],
};

export const summary = (steps: { label: string }[]) => {
  const counts = new Map<string, number>();
  let other = 0;
  for (const step of steps) {
    if (step.label in kinds) {
      counts.set(step.label, (counts.get(step.label) ?? 0) + 1);
    } else {
      other += 1;
    }
  }
  const parts = [...counts].map(([label, count]) => {
    const [verb, one, many] = kinds[label] ?? ["did", "step", "steps"];
    return `${verb} ${count} ${count === 1 ? one : many}`;
  });
  if (other > 0) {
    parts.push(`${other} other ${other === 1 ? "step" : "steps"}`);
  }
  return parts.join(", ");
};

export const matches = (chat: Chat, query: string) => {
  const words = query.toLowerCase().split(/\s+/u).filter(Boolean);
  const haystack =
    `${chat.title} ${chat.project} ${chat.place} ${chat.agent}`.toLowerCase();
  return words.every((word) => haystack.includes(word));
};

export interface Target {
  key: string;
  agent: Agent;
  id: string | null;
  title: string;
  place: string;
  folder: Folder | null;
  updated: number;
}

export const target = (chat: Chat, key: string): Target => ({
  agent: chat.agent,
  folder: { path: chat.folder, place: chat.place, project: chat.project },
  id: chat.id,
  key,
  place: chat.place,
  title: chat.title,
  updated: chat.updated,
});

export const fresh = (
  agent: Agent,
  folder: Folder | null,
  stamp: number
): Target => ({
  agent,
  folder,
  id: null,
  key: `fresh:${stamp}`,
  place: folder?.place ?? "",
  title: "new chat",
  updated: 0,
});
