import { Channel, invoke } from "@tauri-apps/api/core";

export type Agent = "claude" | "codex" | "sombre";

export type Mode = "ask" | "auto" | "yolo";

export type Choice = "yes" | "always" | "no";

export interface Chat {
  agent: Agent;
  id: string;
  title: string;
  project: string;
  place: string;
  folder: string;
  ago: string;
  updated: number;
}

export interface Changed {
  path: string;
  added: number | null;
  removed: number | null;
}

export interface Diff {
  from: string;
  to: string;
  text: string;
  cut: boolean;
}

export interface Editor {
  id: string;
  name: string;
}

export interface Repo {
  branch: string | null;
  upstream: string | null;
  ahead: number;
  behind: number;
  changed: number;
  files: string[];
  remote: string | null;
}

export interface Installed {
  agent: Agent;
  version: string | null;
  install: string;
  docs: string;
}

export interface Picture {
  path: string;
  src: string;
}

export interface Provider {
  name: string;
  url: string;
  key: string | null;
  builtin: boolean;
}

export interface Preset {
  id: string;
  label: string;
  url: string;
  local: boolean;
}

export interface Offer {
  from: string;
  name: string;
  run: string;
}

export interface Aim {
  objective: string;
  status: string;
  active: boolean;
  summary: string;
}

export interface Server {
  name: string;
  run: string;
}

export interface Skill {
  name: string;
  about: string;
}

export interface Settings {
  agent: Agent;
  mode: Mode;
  effort: string | null;
  publish: boolean;
  steer: boolean;
  awake: boolean;
  notify: boolean;
  model: string;
  editor: string | null;
  editors: Editor[];
  providers: Provider[];
  presets: Preset[];
  servers: Server[];
  offered: Offer[];
  skills: Skill[];
  home: string;
}

export type Change = Partial<
  Pick<
    Settings,
    "agent" | "mode" | "publish" | "steer" | "awake" | "notify" | "model"
  >
> & { effort?: string; editor?: string };

export interface Task {
  text: string;
  status: string;
}

export interface Folder {
  path: string;
  place: string;
  project: string;
}

export interface Defaults {
  agent: Agent;
  mode: Mode;
  folders: Folder[];
}

export interface Model {
  id: string;
  about: string;
}

export interface Question {
  header: string;
  text: string;
  choices: { label: string; about: string }[];
  multiple: boolean;
}

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
  | { kind: "done"; millis: number | null; cost: number | null };

export type Entry = Item & { id: number };

export type Update =
  | {
      kind: "state";
      busy: boolean;
      model: string | null;
      mode: Mode;
      session: string | null;
      effort: string | null;
      levels: string[];
      cost: number | null;
      left: number | null;
      folder: string;
      goal: Aim | null;
    }
  | { kind: "delta"; text: string }
  | { kind: "text"; text: string }
  | { kind: "step"; step: string; label: string; detail: string }
  | { kind: "output"; step: string; text: string; failed: boolean }
  | { kind: "patch"; step: string; lines: string[] }
  | { kind: "plan"; tasks: Task[] }
  | {
      kind: "ask";
      id: string;
      tool: string;
      detail: string;
      reason: string;
      change: string[];
      always: string | null;
    }
  | { kind: "questions"; id: string; questions: Question[] }
  | { kind: "granted"; id: string }
  | { kind: "note"; text: string }
  | { kind: "failed"; text: string }
  | { kind: "changes"; from: string; to: string; files: Changed[] }
  | {
      kind: "done";
      millis: number | null;
      cost: number | null;
      left: number | null;
    };

export interface Opening {
  key: string;
  agent: Agent;
  id: string | null;
  folder: string | null;
  model: string | null;
  mode: Mode | null;
  effort: string | null;
}

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

export const chats = () => invoke<Chat[]>("chats");

export const chat = (agent: Agent, id: string) =>
  invoke<Entry[]>("chat", { agent, id });

export const defaults = () => invoke<Defaults>("defaults");

export const folder = (path: string) => invoke<Folder>("folder", { path });

export const models = (agent: Agent) => invoke<Model[]>("models", { agent });

export const start = (opening: Opening, listen: (update: Update) => void) => {
  const updates = new Channel<Update>(listen);
  return invoke<null>("start", { chat: opening, updates });
};

export const send = (key: string, text: string, images: string[]) =>
  invoke<null>("send", { images, key, text });

export const steer = (key: string, text: string) =>
  invoke<boolean>("steer", { key, text });

export const answer = (key: string, id: string, choice: Choice) =>
  invoke<null>("answer", { choice, id, key });

export const tell = (key: string, id: string, answers: string[] | null) =>
  invoke<null>("reply", { answers, id, key });

export const stop = (key: string) => invoke<null>("stop", { key });

export const tune = (
  key: string,
  change: { model?: string; mode?: Mode; effort?: string }
) =>
  invoke<string>("tune", {
    effort: change.effort ?? null,
    key,
    mode: change.mode ?? null,
    model: change.model ?? null,
  });

export const rename = (key: string, name: string) =>
  invoke<string>("rename", { key, name });

export const compact = (key: string) => invoke<null>("compact", { key });

export const keep = (data: string) => invoke<Picture>("keep", { data });

export const picture = (path: string) => invoke<Picture>("picture", { path });

export const settings = (place: string | null) =>
  invoke<Settings>("settings", { folder: place });

export const configure = (change: Change, place: string | null) =>
  invoke<Settings>("configure", { change, folder: place });

export const connect = (
  provider: { name: string; url: string; key: string },
  place: string | null
) => invoke<Settings>("connect", { ...provider, folder: place });

export const rekey = (name: string, key: string, place: string | null) =>
  invoke<Settings>("rekey", { folder: place, key, name });

export const disconnect = (name: string, place: string | null) =>
  invoke<Settings>("disconnect", { folder: place, name });

export const plug = (name: string, run: string, place: string | null) =>
  invoke<Settings>("plug", { folder: place, name, run });

export const unplug = (name: string, place: string | null) =>
  invoke<Settings>("unplug", { folder: place, name });

export const bring = (name: string, place: string | null) =>
  invoke<Settings>("bring", { folder: place, name });

export const record = (
  agent: Agent,
  id: string,
  change: { turn: number; from: string; to: string; files: Changed[] }
) => invoke<null>("record", { agent, change, id });

export const purge = (path: string) =>
  invoke<number>("purge", { folder: path });

export const goal = (key: string, text: string) =>
  invoke<null>("goal", { key, text });

export const carry = (move: {
  key: string;
  from: Agent;
  id: string;
  to: Agent;
  folder: string;
}) => invoke<{ session: string; note: string }>("carry", move);

export const changes = (
  place: string,
  from: string | null,
  to: string | null
) => invoke<Diff>("changes", { folder: place, from, to });

export const editors = () => invoke<Editor[]>("editors");

export const launch = (path: string, editor: string | null) =>
  invoke<null>("launch", { editor, path });

export const installs = () => invoke<Installed[]>("agents");

export const files = (place: string) =>
  invoke<string[]>("files", { folder: place });

export const repo = (place: string) =>
  invoke<Repo | null>("repo", { folder: place });

export const commit = (place: string, message: string) =>
  invoke<string>("commit", { folder: place, message });

export const push = (place: string) =>
  invoke<string>("push", { folder: place });

export const branch = (place: string, name: string) =>
  invoke<string>("branch", { folder: place, name });

export const request = (place: string) =>
  invoke<string>("request", { folder: place });

export type Out =
  | { kind: "data"; text: string }
  | { kind: "exit"; code: number | null };

export const pty = (
  shell: { key: string; folder: string; cols: number; rows: number },
  listen: (out: Out) => void
) => {
  const output = new Channel<Out>(listen);
  return invoke<null>("pty", { ...shell, output });
};

export const input = (key: string, data: string) =>
  invoke<null>("input", { data, key });

export const resize = (key: string, cols: number, rows: number) =>
  invoke<null>("resize", { cols, key, rows });

export const hangup = (key: string) => invoke<null>("hangup", { key });

export const failure = (error: unknown) =>
  typeof error === "string" ? error : "something went wrong talking to sombre";
