import { history } from "@/lib/history";
import type { Device } from "@/lib/history";

export interface Session {
  approved: boolean;
  live: number;
  peek: number | null;
  running: boolean;
}

export type Chip = "approve" | "approved" | null;

export interface View {
  busy: boolean;
  chip: Chip;
  device: Device;
  meta: string;
  replay: boolean;
  time: string;
  title: string;
}

const at = (id: string) => history.findIndex((event) => event.id === id);

export const start = at("handoff");
export const gate = at("allow");
export const end = at("suite");

export const initial: Session = {
  approved: false,
  live: start,
  peek: null,
  running: false,
};

export const advance = (state: Session, id: string): Session => {
  const index = at(id);
  if (index === -1) {
    return state;
  }
  const limit = state.approved && !state.running ? end : gate;
  const live = Math.max(state.live, Math.min(index, limit));
  return live === state.live ? state : { ...state, live };
};

export const approve = (state: Session): Session =>
  state.live === gate && !state.approved
    ? { ...state, approved: true, running: true }
    : state;

export const finish = (state: Session): Session =>
  state.running ? { ...state, live: end, running: false } : state;

export const peek = (state: Session, index: number | null): Session =>
  state.peek === index ? state : { ...state, peek: index };

const chip = (state: Session): Chip => {
  if (state.live < gate) {
    return null;
  }
  return state.approved ? "approved" : "approve";
};

export const view = (state: Session): View => {
  const replay = state.peek !== null;
  const event = history[state.peek ?? state.live] ?? history[start];
  const base: View = {
    busy: false,
    chip: null,
    device: event?.device ?? "phone",
    meta: event?.meta ?? "",
    replay,
    time: event?.time ?? "",
    title: event?.title ?? "",
  };
  if (replay) {
    return base;
  }
  if (state.running) {
    return { ...base, busy: true, chip: "approved", title: "Running bun test" };
  }
  if (state.live === gate && !state.approved) {
    return { ...base, chip: "approve", title: "Wants to run bun test" };
  }
  return { ...base, chip: chip(state) };
};

let current = initial;
const listeners = new Set<() => void>();

const set = (next: Session) => {
  if (next === current) {
    return;
  }
  current = next;
  for (const listener of listeners) {
    listener();
  }
};

export const session = {
  advance: (id: string) => set(advance(current, id)),
  approve: () => set(approve(current)),
  finish: () => set(finish(current)),
  peek: (index: number | null) => set(peek(current, index)),
  read: () => current,
  subscribe: (listener: () => void) => {
    listeners.add(listener);
    return () => {
      listeners.delete(listener);
    };
  },
};
