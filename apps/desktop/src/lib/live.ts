import type {
  Aim,
  Choice,
  Entry,
  Item,
  Mode,
  Picture,
  Task,
  Update,
} from "./api";

export interface Queued {
  text: string;
  images: Picture[];
}

export interface Live {
  entries: Entry[];
  busy: boolean;
  model: string | null;
  mode: Mode;
  session: string | null;
  effort: string | null;
  levels: string[];
  cost: number | null;
  left: number | null;
  folder: string | null;
  goal: Aim | null;
  open: boolean;
  since: number | null;
  queue: Queued[];
  stopping: boolean;
  restore: number;
  next: number;
}

export type Asked = Entry & { kind: "ask" };

export type Quiz = Entry & { kind: "questions" };

export const initial = (mode: Mode = "ask"): Live => ({
  busy: false,
  cost: null,
  effort: null,
  entries: [],
  folder: null,
  goal: null,
  left: null,
  levels: [],
  mode,
  model: null,
  next: 1_000_000,
  open: false,
  queue: [],
  restore: 0,
  session: null,
  since: null,
  stopping: false,
});

const push = (live: Live, item: Item, open = false): Live => ({
  ...live,
  entries: [...live.entries, { ...item, id: live.next }],
  next: live.next + 1,
  open,
});

const swap = (
  live: Live,
  index: number,
  entry: Entry,
  open: boolean
): Live => ({
  ...live,
  entries: live.entries.map((current, at) => (at === index ? entry : current)),
  open,
});

const touch = (
  live: Live,
  step: string,
  change: (entry: Entry & { kind: "step" }) => Entry
): Live => {
  const index = live.entries.findLastIndex(
    (entry) => entry.kind === "step" && entry.step === step
  );
  const found = live.entries[index];
  if (found?.kind !== "step") {
    return live;
  }
  return swap(live, index, change(found), live.open);
};

const stream = (live: Live, text: string, more: boolean): Live => {
  const index = live.entries.length - 1;
  const last = live.entries[index];
  if (live.open && last?.kind === "text") {
    return swap(
      live,
      index,
      { ...last, text: more ? last.text + text : text },
      more
    );
  }
  return push(live, { kind: "text", text }, more);
};

export const answered = (live: Live, ask: string, choice: Choice): Live => ({
  ...live,
  entries: live.entries.map((entry) =>
    entry.kind === "ask" && entry.ask === ask
      ? { ...entry, answer: choice }
      : entry
  ),
});

export const replied = (
  live: Live,
  ask: string,
  answers: string[] | null
): Live => ({
  ...live,
  entries: live.entries.map((entry) =>
    entry.kind === "questions" && entry.ask === ask
      ? { ...entry, answers, settled: true }
      : entry
  ),
});

const stated = (
  live: Live,
  update: Update & { kind: "state" },
  now: number
): Live => ({
  ...live,
  busy: update.busy,
  cost: update.cost,
  effort: update.effort,
  folder: update.folder,
  goal: update.goal,
  left: update.left ?? live.left,
  levels: update.levels,
  mode: update.mode,
  model: update.model,
  session: update.session,
  since: update.busy ? (live.since ?? now) : null,
});

const planned = (live: Live, tasks: Task[]): Live => {
  const index = live.entries.findLastIndex((entry) => entry.kind === "plan");
  const turn = live.entries.findLastIndex((entry) => entry.kind === "user");
  const found = live.entries[index];
  if (found && index > turn) {
    return swap(live, index, { id: found.id, kind: "plan", tasks }, false);
  }
  return push(live, { kind: "plan", tasks });
};

export const apply = (live: Live, update: Update, now: number): Live => {
  switch (update.kind) {
    case "state": {
      return stated(live, update, now);
    }
    case "delta": {
      return stream(live, update.text, true);
    }
    case "text": {
      return stream(live, update.text, false);
    }
    case "step": {
      return push(live, {
        detail: update.detail,
        failed: false,
        kind: "step",
        label: update.label,
        output: "",
        patch: [],
        step: update.step,
      });
    }
    case "output": {
      return touch(live, update.step, (entry) => ({
        ...entry,
        failed: update.failed,
        output: update.text,
      }));
    }
    case "patch": {
      return touch(live, update.step, (entry) => ({
        ...entry,
        patch: update.lines,
      }));
    }
    case "plan": {
      return planned(live, update.tasks);
    }
    case "ask": {
      return push(live, {
        always: update.always,
        answer: null,
        ask: update.id,
        change: update.change,
        detail: update.detail,
        kind: "ask",
        reason: update.reason,
        tool: update.tool,
      });
    }
    case "questions": {
      return push(live, {
        answers: null,
        ask: update.id,
        kind: "questions",
        questions: update.questions,
        settled: false,
      });
    }
    case "granted": {
      return answered(live, update.id, "yes");
    }
    case "decided": {
      return answered(live, update.id, update.yes ? "yes" : "no");
    }
    case "replied": {
      return replied(live, update.id, update.answers);
    }
    case "phone": {
      return push(live, { kind: "user", text: update.text });
    }
    case "changes": {
      return push(live, {
        files: update.files,
        from: update.from,
        kind: "changes",
        to: update.to,
      });
    }
    case "done": {
      return push(live, {
        cost: update.cost,
        kind: "done",
        millis: update.millis,
      });
    }
    default: {
      return push(live, { kind: update.kind, text: update.text });
    }
  }
};

export const said = (
  live: Live,
  text: string,
  steered = false,
  images: string[] = []
): Live => push(live, { images, kind: "user", steered, text });

export const told = (live: Live, texts: string[]) => {
  let next = live;
  for (const text of texts) {
    next = said(next, text, true);
  }
  return next;
};

export const steering = (queue: Queued[]) => {
  const sent = queue.filter((queued) => queued.images.length === 0);
  return {
    kept: queue.filter((queued) => queued.images.length > 0),
    message: sent.map((queued) => queued.text).join("\n\n"),
    sent,
  };
};

export const marked = (text: string, count: number) =>
  [
    text,
    Array.from({ length: count }, (_, index) => `[Image #${index + 1}]`).join(
      " "
    ),
  ]
    .filter((part) => part !== "")
    .join(" ");

export const waiting = (live: Live): Asked | Quiz | undefined =>
  live.entries.find(
    (entry): entry is Asked | Quiz =>
      (entry.kind === "ask" && entry.answer === null) ||
      (entry.kind === "questions" && !entry.settled)
  );

export type Flag = "busy" | "ask" | "new" | "failed";

export const mark = (live: Live, unread: boolean): Flag | null => {
  if (live.busy) {
    return waiting(live) ? "ask" : "busy";
  }
  if (!unread) {
    return null;
  }
  return live.entries.at(-1)?.kind === "failed" ? "failed" : "new";
};

export const marks = (text: string) =>
  new Map(
    text
      .split("\n")
      .filter(Boolean)
      .map((line): [string, Flag] => {
        const at = line.lastIndexOf("=");
        const value = line.slice(at + 1);
        const known: Flag[] = ["busy", "ask", "new", "failed"];
        return [
          line.slice(0, at),
          known.find((entry) => entry === value) ?? "new",
        ];
      })
  );

export const restored = (live: Live, draft: string) => ({
  draft: [...live.queue.map((queued) => queued.text), draft]
    .filter((part) => part.trim() !== "")
    .join("\n"),
  live: { ...live, queue: [], restore: live.restore + 1, stopping: false },
});

export const asked = (tool: string) => {
  if (tool === "Bash") {
    return "run this command?";
  }
  if (["Edit", "Write", "MultiEdit", "NotebookEdit"].includes(tool)) {
    return "make this edit?";
  }
  if (tool === "WebFetch" || tool === "WebSearch") {
    return "go on the web?";
  }
  return `use ${tool}?`;
};

export const seconds = (since: number | null, now: number) =>
  since === null ? 0 : Math.max(0, Math.floor((now - since) / 1000));
