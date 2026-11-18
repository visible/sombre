import { Channel } from "@tauri-apps/api/core";

import type {
  Agent,
  Change,
  Changed,
  Choice,
  Mode,
  Opening,
  Out,
  Question,
  Task,
  Update,
} from "./bindings";
import { commands } from "./bindings";

export type {
  Agent,
  Aim,
  Change,
  Changed,
  Chat,
  Checked,
  Choice,
  Defaults,
  Diff,
  Editor,
  Folder,
  Installed,
  Mode,
  Model,
  Offer,
  Opening,
  Out,
  Paired,
  Phone,
  Picture,
  Preset,
  Provider,
  Question,
  Repo,
  Server,
  Settings,
  Skill,
  Task,
  Update,
} from "./bindings";

export type Item =
  | { kind: "user"; text: string; steered?: boolean; images?: string[] }
  | { kind: "text"; text: string }
  | {
      kind: "step";
      step: string;
      label: string;
      detail: string;
      output: string;
      failed: boolean;
      patch: string[];
    }
  | { kind: "plan"; tasks: Task[] }
  | {
      kind: "ask";
      ask: string;
      tool: string;
      detail: string;
      reason: string;
      change: string[];
      always: string | null;
      answer: Choice | null;
    }
  | {
      kind: "questions";
      ask: string;
      questions: Question[];
      answers: string[] | null;
      settled: boolean;
    }
  | { kind: "note"; text: string }
  | { kind: "failed"; text: string }
  | { kind: "changes"; from: string; to: string; files: Changed[] }
  | { kind: "done"; millis: number | null };

export type Entry = Item & { id: number };

export const names: Record<Agent, string> = {
  claude: "claude code",
  codex: "codex",
  sombre: "sombre",
};

export const agents: Agent[] = ["claude", "codex", "sombre"];

export const levels: Record<Agent, string[]> = {
  claude: ["low", "medium", "high", "xhigh", "max"],
  codex: ["minimal", "low", "medium", "high", "xhigh"],
  sombre: ["minimal", "low", "medium", "high", "xhigh", "max"],
};

export const modes: { mode: Mode; about: string }[] = [
  { about: "ask before edits and commands", mode: "ask" },
  { about: "edit files freely, ask before commands", mode: "auto" },
  { about: "never ask. full access to your computer", mode: "yolo" },
];

export const rotate = (mode: Mode): Mode => {
  const order = modes.map((entry) => entry.mode);
  return order[(order.indexOf(mode) + 1) % order.length] ?? "ask";
};

export const chats = () => commands.chats();

export const chat = (
  agent: Agent,
  id: string,
  whole: boolean
): Promise<{ entries: Entry[]; whole: boolean }> =>
  commands.chat(agent, id, whole);

export const defaults = () => commands.defaults();

export const folder = (path: string) => commands.folder(path);

export const models = (agent: Agent) => commands.models(agent);

export const start = (opening: Opening, listen: (update: Update) => void) =>
  commands.start(opening, new Channel<Update>(listen));

export const send = (key: string, text: string, images: string[]) =>
  commands.send(key, text, images);

export const steer = (key: string, text: string) => commands.steer(key, text);

export const answer = (key: string, id: string, choice: Choice) =>
  commands.answer(key, id, choice);

export const pair = (key: string, home?: boolean) =>
  commands.pair(key, home ?? null);

export const phones = () => commands.phones();

export const forget = (id: string) => commands.forget(id);

export const unpair = () => commands.unpair();

export const take = (key: string) => commands.take(key);

export const check = () => commands.check();

export const ready = () => commands.ready();

export const restart = () => commands.restart();

export const followed = () => commands.followed();

export const tell = (key: string, id: string, answers: string[] | null) =>
  commands.reply(key, id, answers);

export const stop = (key: string) => commands.stop(key);

export const tune = (
  key: string,
  change: { model?: string; mode?: Mode; effort?: string }
) =>
  commands.tune(
    key,
    change.model ?? null,
    change.mode ?? null,
    change.effort ?? null
  );

export const rename = (key: string, name: string) => commands.rename(key, name);

export const compact = (key: string) => commands.compact(key);

export const keep = (data: string) => commands.keep(data);

export const picture = (path: string) => commands.picture(path);

export const settings = (place: string | null) => commands.settings(place);

export const configure = (change: Change, place: string | null) =>
  commands.configure(change, place);

export const connect = (
  provider: { name: string; url: string; key: string },
  place: string | null
) => commands.connect(provider.name, provider.url, provider.key, place);

export const rekey = (name: string, key: string, place: string | null) =>
  commands.rekey(name, key, place);

export const disconnect = (name: string, place: string | null) =>
  commands.disconnect(name, place);

export const plug = (name: string, run: string, place: string | null) =>
  commands.plug(name, run, place);

export const unplug = (name: string, place: string | null) =>
  commands.unplug(name, place);

export const bring = (name: string, place: string | null) =>
  commands.bring(name, place);

export const record = (
  agent: Agent,
  id: string,
  change: { turn: number; from: string; to: string; files: Changed[] }
) => commands.record(agent, id, change);

export const purge = (path: string) => commands.purge(path);

export const goal = (key: string, text: string) => commands.goal(key, text);

export const carry = (move: {
  key: string;
  from: Agent;
  id: string;
  to: Agent;
  folder: string;
}) => commands.carry(move.key, move.from, move.id, move.to, move.folder);

export const changes = (
  place: string,
  from: string | null,
  to: string | null
) => commands.changes(place, from, to);

export const editors = () => commands.editors();

export const launch = (path: string, editor: string | null) =>
  commands.launch(path, editor);

export const installs = () => commands.agents();

export const files = (place: string) => commands.files(place);

export const repo = (place: string) => commands.repo(place);

export const commit = (place: string, message: string) =>
  commands.commit(place, message);

export const push = (place: string) => commands.push(place);

export const branch = (place: string, name: string) =>
  commands.branch(place, name);

export const request = (place: string) => commands.request(place);

export const pty = (
  shell: { key: string; folder: string; cols: number; rows: number },
  listen: (out: Out) => void
) =>
  commands.pty(
    shell.key,
    shell.folder,
    shell.cols,
    shell.rows,
    new Channel<Out>(listen)
  );

export const input = (key: string, data: string) => commands.input(key, data);

export const resize = (key: string, cols: number, rows: number) =>
  commands.resize(key, cols, rows);

export const hangup = (key: string) => commands.hangup(key);

export const failure = (error: unknown) =>
  typeof error === "string" ? error : "something went wrong talking to sombre";
