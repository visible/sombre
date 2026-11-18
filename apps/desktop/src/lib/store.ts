import { UserAttentionType, getCurrentWindow } from "@tauri-apps/api/window";
import { useSyncExternalStore } from "react";

import type {
  Agent,
  Choice,
  Entry,
  Mode,
  Opening,
  Picture,
  Update,
} from "./api";
import {
  answer,
  carry,
  compact,
  failure,
  goal,
  record,
  names,
  rename,
  send,
  start,
  steer,
  stop,
  tell,
  tune,
} from "./api";
import { numbers } from "./inbox";
import type { Live } from "./live";
import {
  answered,
  apply,
  initial,
  mark,
  marked,
  replied,
  restored,
  said,
  steering,
  told,
  waiting,
} from "./live";

const EMPTY = initial();
const lives = new Map<string, Live>();
const drafts = new Map<string, string>();
const aliases = new Map<string, string>();
const opened = new Set<string>();
const starting = new Map<string, Promise<null>>();
const stories = new Map<
  string,
  { entries: Entry[]; updated: number; whole: boolean }
>();
const prompts = new Map<string, number>();
const KEPT = 6;
const unread = new Set<string>();
let viewing: string | null = null;
let shown = "";

const SEEN = "sombre.seen";
const SINCE = "sombre.since";
const clock = () => Math.floor(Date.now() / 1000);

const recalled = () => {
  try {
    return numbers(JSON.parse(localStorage.getItem(SEEN) ?? "{}"));
  } catch {
    return {};
  }
};

const started = () => {
  const saved = Number(localStorage.getItem(SINCE));
  if (saved > 0) {
    return saved;
  }
  const now = clock();
  localStorage.setItem(SINCE, String(now));
  return now;
};

export interface Seen {
  at: Record<string, number>;
  since: number;
}

let glance: Seen = { at: recalled(), since: started() };

const look = (key: string) => {
  glance = { ...glance, at: { ...glance.at, [key]: clock() } };
  localStorage.setItem(SEEN, JSON.stringify(glance.at));
};

export const seenAt = (seen: Seen, key: string) =>
  Math.max(seen.at[key] ?? 0, seen.since);

export type Event = "done" | "ask" | "failed";

let alerting: (key: string, event: Event) => void = () => null;
const listeners = new Set<() => void>();
let busy = "";
let changed: () => void = () => null;

const subscribe = (listener: () => void) => {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
};

let frame = 0;

const notify = () => {
  frame = 0;
  busy = [...lives]
    .filter(([, live]) => live.busy)
    .map(([key]) => key)
    .join("\n");
  shown = [...lives]
    .map(([key, live]) => {
      const found = mark(live, unread.has(key));
      return found ? `${key}=${found}` : "";
    })
    .filter(Boolean)
    .join("\n");
  for (const listener of listeners) {
    listener();
  }
};

const emit = () => {
  if (frame === 0) {
    frame = requestAnimationFrame(notify);
  }
};

const read = (key: string) => lives.get(key) ?? EMPTY;

const write = (key: string, change: (live: Live) => Live) => {
  lives.set(key, change(read(key)));
  emit();
};

const fail = (key: string, error: unknown) =>
  write(key, (live) =>
    apply(
      { ...live, busy: false, since: null },
      { kind: "failed", text: failure(error) },
      Date.now()
    )
  );

const nudge = () => {
  if (!document.hasFocus()) {
    getCurrentWindow().requestUserAttention(UserAttentionType.Informational);
  }
};

const dispatch = async (key: string, text: string, images: Picture[] = []) => {
  write(key, (live) => ({
    ...said(
      live,
      text,
      false,
      images.map((image) => image.src)
    ),
    busy: true,
    since: Date.now(),
  }));
  try {
    await starting.get(key);
    await send(
      key,
      marked(text, images.length),
      images.map((image) => image.path)
    );
  } catch (error) {
    fail(key, error);
  }
};

const settle = (key: string, live: Live) => {
  if (live.stopping) {
    const back = restored(live, drafts.get(key) ?? "");
    drafts.set(key, back.draft);
    lives.set(key, back.live);
    return;
  }
  const [next, ...rest] = live.queue;
  if (next !== undefined) {
    lives.set(key, { ...live, queue: rest });
    dispatch(key, next.text, next.images);
  }
};

const save = (
  key: string,
  agent: Agent,
  live: Live,
  update: Extract<Update, { kind: "changes" }>
) => {
  if (!live.session || stories.get(key)?.whole === false) {
    return;
  }
  const asked =
    (prompts.get(key) ?? 0) +
    live.entries.filter((entry) => entry.kind === "user").length;
  record(agent, live.session, {
    files: update.files,
    from: update.from,
    to: update.to,
    turn: asked,
  }).catch(() => null);
};

const away = (key: string) => viewing !== key || !document.hasFocus();

const listen = (key: string, agent: Agent) => (update: Update) => {
  const before = read(key);
  const after = apply(before, update, Date.now());
  lives.set(key, after);
  if (update.kind === "changes") {
    save(key, agent, after, update);
  }
  const asking = update.kind === "ask" || update.kind === "questions";
  if (asking && !waiting(before) && away(key)) {
    alerting(key, "ask");
  }
  if (before.busy && !after.busy && !after.stopping && away(key)) {
    unread.add(key);
    alerting(key, after.entries.at(-1)?.kind === "failed" ? "failed" : "done");
  }
  if (update.kind === "state" && update.session) {
    aliases.set(`${agent}:${update.session}`, key);
  }
  if (asking || update.kind === "done") {
    nudge();
  }
  if (before.busy && !after.busy) {
    settle(key, after);
    changed();
  } else if (update.kind === "state" && update.session !== before.session) {
    changed();
  }
  emit();
};

export const keyOf = (agent: Agent, id: string) =>
  aliases.get(`${agent}:${id}`) ?? `${agent}:${id}`;

export const watch = (refresh: () => void) => {
  changed = refresh;
};

export const useLive = (key: string) =>
  useSyncExternalStore(subscribe, () => read(key));

export const useBusy = () => useSyncExternalStore(subscribe, () => busy);

export const useMarks = () => useSyncExternalStore(subscribe, () => shown);

export const useSeen = () => useSyncExternalStore(subscribe, () => glance);

export const alerts = (handler: (key: string, event: Event) => void) => {
  alerting = handler;
};

export const view = (key: string | null) => {
  if (viewing && viewing !== key) {
    look(viewing);
  }
  viewing = key;
  if (key) {
    look(key);
  }
  if (key && document.hasFocus()) {
    unread.delete(key);
  }
  emit();
};

window.addEventListener("focus", () => {
  if (viewing) {
    look(viewing);
  }
  if (viewing && unread.delete(viewing)) {
    emit();
  }
});

window.addEventListener("blur", () => {
  if (viewing) {
    look(viewing);
    emit();
  }
});

window.addEventListener("beforeunload", () => {
  if (viewing) {
    look(viewing);
  }
});

export const draft = (key: string) => drafts.get(key) ?? "";

export const keep = (key: string, text: string) => {
  drafts.set(key, text);
};

export const begin = async (opening: Opening) => {
  if (opened.has(opening.key)) {
    return;
  }
  opened.add(opening.key);
  if (opening.mode) {
    write(opening.key, (live) => ({
      ...live,
      mode: opening.mode ?? live.mode,
    }));
  }
  const launched = start(opening, listen(opening.key, opening.agent));
  starting.set(
    opening.key,
    launched.then(
      () => null,
      () => null
    )
  );
  try {
    await launched;
  } catch (error) {
    opened.delete(opening.key);
    fail(opening.key, error);
  } finally {
    starting.delete(opening.key);
  }
};

export const submit = async (
  key: string,
  text: string,
  images: Picture[] = []
) => {
  drafts.set(key, "");
  if (read(key).busy) {
    write(key, (live) => ({
      ...live,
      queue: [...live.queue, { images, text }],
    }));
    return;
  }
  await dispatch(key, text, images);
};

export const unqueue = (key: string) => {
  const live = read(key);
  const last = live.queue.at(-1);
  if (last === undefined) {
    return null;
  }
  write(key, (current) => ({ ...current, queue: current.queue.slice(0, -1) }));
  return last;
};

export const hurry = async (key: string, agent: Agent, text: string) => {
  const live = read(key);
  const flushing = text.trim() === "";
  const { message, sent } = flushing
    ? steering(live.queue)
    : { message: text, sent: [{ images: [], text }] };
  if (message.trim() === "" || !live.busy) {
    return false;
  }
  try {
    if (await steer(key, message)) {
      write(key, (current) => ({
        ...told(
          current,
          sent.map((queued) => queued.text)
        ),
        queue: flushing ? steering(current.queue).kept : current.queue,
      }));
      return true;
    }
  } catch (error) {
    fail(key, error);
    return false;
  }
  write(key, (current) =>
    apply(
      {
        ...current,
        queue:
          text.trim() === ""
            ? current.queue
            : [...current.queue, { images: [], text }],
      },
      {
        kind: "note",
        text: `${names[agent]} can't take a message mid-turn here, so it's queued for after`,
      },
      Date.now()
    )
  );
  return true;
};

export const respond = async (key: string, ask: string, choice: Choice) => {
  write(key, (live) => answered(live, ask, choice));
  try {
    await answer(key, ask, choice);
  } catch (error) {
    fail(key, error);
  }
};

export const resolve = async (
  key: string,
  ask: string,
  answers: string[] | null
) => {
  write(key, (live) => replied(live, ask, answers));
  try {
    await tell(key, ask, answers);
  } catch (error) {
    fail(key, error);
  }
};

export const halt = async (key: string) => {
  if (!read(key).busy) {
    return;
  }
  write(key, (live) => ({ ...live, stopping: true }));
  try {
    await stop(key);
  } catch (error) {
    fail(key, error);
  }
};

const noted = (key: string, text: string) =>
  write(key, (live) => apply(live, { kind: "note", text }, Date.now()));

export const retune = async (
  key: string,
  change: { model?: string; mode?: Mode; effort?: string },
  quiet = false
) => {
  try {
    const reply = await tune(key, change);
    if (!quiet) {
      noted(key, reply);
    }
  } catch (error) {
    fail(key, error);
  }
};

export const retitle = async (key: string, name: string) => {
  try {
    noted(key, await rename(key, name));
    changed();
    return true;
  } catch (error) {
    fail(key, error);
    return false;
  }
};

export const squash = async (key: string) => {
  try {
    await compact(key);
  } catch (error) {
    fail(key, error);
  }
};

export const note = noted;

export const aim = async (key: string, text: string) => {
  try {
    await goal(key, text);
  } catch (error) {
    fail(key, error);
  }
};

export const move = async (
  chat: { key: string; agent: Agent; folder: string },
  id: string,
  to: Agent
) => {
  if (read(chat.key).busy) {
    noted(chat.key, "wait for this turn to finish, or stop it first");
    return null;
  }
  try {
    return await carry({ ...chat, from: chat.agent, id, to });
  } catch (error) {
    opened.delete(chat.key);
    await begin({
      agent: chat.agent,
      effort: null,
      folder: null,
      id,
      key: chat.key,
      mode: null,
      model: null,
    });
    fail(chat.key, error);
    return null;
  }
};

export const isOpen = (key: string) => opened.has(key);

export const remembered = (key: string) => stories.get(key)?.entries;

export const unchanged = (key: string, updated: number) => {
  const found = stories.get(key);
  return found !== undefined && found.whole && found.updated === updated;
};

export const remember = (
  key: string,
  entries: Entry[],
  updated: number,
  whole: boolean
) => {
  stories.delete(key);
  stories.set(key, { entries, updated, whole });
  if (whole) {
    prompts.set(key, entries.filter((entry) => entry.kind === "user").length);
  }
  for (const old of stories.keys()) {
    if (stories.size <= KEPT) {
      break;
    }
    stories.delete(old);
  }
};

export const forget = (key: string) => {
  opened.delete(key);
  lives.delete(key);
  stories.delete(key);
  prompts.delete(key);
  unread.delete(key);
};
