import type { ReactNode, RefObject } from "react";
import {
  Suspense,
  lazy,
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react";

import type {
  Agent,
  Chat,
  Entry as Item,
  Folder,
  Installed,
  Mode,
  Picture,
  Settings,
} from "../lib/api";
import {
  agents,
  chat as read,
  failure,
  followed,
  launch,
  levels,
  modes,
  names,
  rotate,
} from "../lib/api";
import type { Name } from "../lib/commands";
import { commands } from "../lib/commands";
import type { Target } from "../lib/feed";
import { group, recent, rounds } from "../lib/feed";
import { money } from "../lib/format";
import type { Scope } from "../lib/patch";
import { pressed, shortcut } from "../lib/platform";
import { lack } from "../lib/scale";
import {
  aim,
  begin,
  halt,
  note,
  remember,
  unchanged,
  remembered,
  resolve,
  respond,
  retitle,
  retune,
  squash,
  submit,
  useLive,
} from "../lib/store";
import { useWidth } from "../lib/width";
import { Ask } from "./ask";
import { Composer } from "./composer";
import { Edits } from "./edits";
import { Entry } from "./entry";
import { Find } from "./find";
import { Header } from "./header";
import { Home } from "./home";
import { Notice } from "./notice";
import { Phone } from "./phone";
import { Questions } from "./questions";
import { Review } from "./review";
import { Start } from "./start";
import { Work } from "./work";

const Drawer = lazy(async () => {
  const loaded = await import("./drawer");
  return { default: loaded.Drawer };
});

type Story =
  | { state: "loading" }
  | { state: "failed"; message: string }
  | { state: "ready"; entries: Item[] };

export interface Tasks {
  create: (folder?: string) => void;
  settings: (section?: string) => void;
  carry: (target: Target, id: string, to: Agent) => void;
  archive: (target: Target) => void;
  reveal: (folder: string) => void;
  copy: (text: string) => void;
}

interface Props {
  target: Target | null;
  folders: Folder[];
  recent: Chat[];
  installed: Installed[] | null;
  settings: Settings | null;
  hidden: boolean;
  tasks: Tasks;
  onSidebar: () => void;
  onTarget: (target: Target) => void;
  onPick: (chat: Chat) => void;
  onPalette: () => void;
}

const NEAR = 120;

const invite = (agent: Agent, fresh: boolean, missing: boolean) => {
  if (missing) {
    return `install ${names[agent]} first`;
  }
  return fresh
    ? `ask ${names[agent]} anything, or type / for commands`
    : undefined;
};

const usages = commands.map((command) => ({
  about: command.about,
  usage: `/${command.name}${command.takes ? ` <${command.takes}>` : ""}`,
}));
const widest = Math.max(...usages.map(({ usage }) => usage.length));

const help = [
  "commands",
  ...usages.map(({ about, usage }) => `  ${usage.padEnd(widest)}  ${about}`),
  "",
  "keys",
  `  ${shortcut("k")} palette   ${shortcut("n")} new chat   ${shortcut("f")} find   ${shortcut("f", true)} search chats   ${shortcut("d", true)} changes   ${shortcut("b")} sidebar   ${shortcut(",")} settings`,
  "  enter send   shift enter new line   tab steer or queue   esc stop   ↑ edit queued",
].join("\n");

const picked = (next: string | undefined, current: string | null) => {
  if (next === undefined) {
    return current;
  }
  return next === "default" ? null : next;
};

interface Context {
  key: string;
  target: Target;
  started: boolean;
  effort: string[];
  tasks: Tasks;
  status: () => string;
  tune: (change: { model?: string; mode?: Mode; effort?: string }) => void;
  pursue: (text: string) => void;
  move: (to: Agent) => void;
  say: (text: string) => void;
  changes: (() => void) | null;
  terminal: (() => void) | null;
}

const REVIEW =
  "Review the uncommitted changes in this repository. List bugs, risky edits, and missing tests, most important first, with the file and line for each. Don't change any files.";

const report = (fields: {
  agent: Target["agent"];
  model: string | null;
  effort: string | null;
  mode: Mode;
  folder: string;
  chat: string | null;
  spent: string;
  left: number | null;
}) =>
  [
    `agent: ${names[fields.agent]}`,
    `model: ${fields.model ?? "default"}`,
    `effort: ${fields.effort ?? "default"}`,
    `permissions: ${fields.mode}`,
    `folder: ${fields.folder}`,
    `chat: ${fields.chat ?? "not started yet"}`,
    `spent here: ${fields.spent || "$0.00"}`,
    fields.left === null ? "" : `context left: ${fields.left}%`,
  ]
    .filter(Boolean)
    .join("\n");

const perform = (name: Name, rest: string, context: Context) => {
  const { key, target, started, tasks, tune } = context;
  const table: Record<Name, () => void> = {
    agent: () => {
      const found = agents.find(
        (entry) => entry === rest || names[entry] === rest
      );
      if (found) {
        context.move(found);
      } else {
        note(key, "pick one: claude, codex, or sombre");
      }
    },
    archive: () => tasks.archive(target),
    changes: () =>
      context.changes
        ? context.changes()
        : note(key, "this chat has no folder to compare"),
    compact: () =>
      started ? squash(key) : note(key, "send a message first, then compact"),
    effort: () =>
      rest && ["default", ...context.effort].includes(rest)
        ? tune({ effort: rest })
        : note(
            key,
            `pick one: default, ${context.effort.join(", ")}. or use the model chip`
          ),
    goal: () => context.pursue(rest),
    help: () => note(key, help),
    mcp: () => tasks.settings("mcp servers"),
    model: () =>
      rest
        ? tune({ model: rest })
        : note(key, "type /model and a name, or use the model chip"),
    new: () => tasks.create(target.folder?.path),
    permissions: () => {
      const found = modes.find((entry) => entry.mode === rest);
      if (found) {
        tune({ mode: found.mode });
      } else {
        note(key, "pick one: ask, auto, or yolo");
      }
    },
    providers: () => tasks.settings("providers"),
    rename: () => {
      if (!rest) {
        note(key, "type /rename and the new name");
      } else if (started) {
        retitle(key, rest);
      } else {
        note(key, "send a message first, then rename the chat");
      }
    },
    reveal: () =>
      target.folder
        ? tasks.reveal(target.folder.path)
        : note(key, "pick a folder first"),
    review: () => context.say(REVIEW),
    settings: () => tasks.settings(),
    status: () => note(key, context.status()),
    stop: () => halt(key),
    terminal: () =>
      context.terminal
        ? context.terminal()
        : note(key, "this chat has no folder for a terminal"),
  };
  table[name]();
};

interface FeedProps {
  agent: Target["agent"];
  chatKey: string;
  busy: boolean;
  story: Story;
  groups: ReturnType<typeof group>;
  pinned: boolean;
  scroller: RefObject<HTMLDivElement | null>;
  onPinned: (pinned: boolean) => void;
  onRetry: () => void;
  onReview: (scope: Scope, focus: string | null) => void;
  onCycle: () => void;
  top: ReactNode;
}

const WINDOW = 120;

interface PartsProps {
  agent: Target["agent"];
  chatKey: string;
  busy: boolean;
  groups: ReturnType<typeof group>;
  onReview: (scope: Scope, focus: string | null) => void;
  onCycle: () => void;
}

const Parts = ({
  agent,
  chatKey,
  busy,
  groups,
  onReview,
  onCycle,
}: PartsProps) => {
  const [limit, setLimit] = useState(WINDOW);
  const kept = useRef<number | null>(null);
  const root = useRef<HTMLDivElement>(null);
  const hidden = Math.max(0, groups.length - limit);
  const shown = hidden > 0 ? groups.slice(hidden) : groups;
  const last = groups.at(-1)?.key;

  useLayoutEffect(() => {
    const element = root.current?.parentElement;
    if (element && kept.current !== null) {
      element.scrollTop = element.scrollHeight - kept.current;
      kept.current = null;
    }
  });

  return (
    <div
      className="mx-auto flex max-w-[780px] flex-col gap-4 px-8 py-8"
      ref={root}
    >
      {hidden > 0 ? (
        <button
          className="text-fog hover:text-body hover:bg-raise active:bg-well cursor-pointer self-center rounded-full px-3 py-1 text-[12.5px]"
          onClick={() => {
            const element = root.current?.parentElement;
            if (element) {
              kept.current = element.scrollHeight - element.scrollTop;
            }
            setLimit((current) => current + WINDOW * 2);
          }}
          type="button"
        >
          {`show earlier messages (${hidden} more)`}
        </button>
      ) : null}
      {shown.map((part) => {
        if (part.kind === "work") {
          return (
            <Work
              key={part.key}
              live={busy && part.key === last}
              steps={part.steps}
            />
          );
        }
        const { entry } = part;
        if (entry.kind === "changes") {
          return (
            <Edits
              files={entry.files}
              key={part.key}
              onOpen={(path) =>
                onReview({ from: entry.from, kind: "turn", to: entry.to }, path)
              }
            />
          );
        }
        if (entry.kind === "ask") {
          return (
            <Ask
              agent={names[agent]}
              entry={entry}
              key={part.key}
              onAnswer={(choice) => respond(chatKey, entry.ask, choice)}
              onCycle={onCycle}
            />
          );
        }
        if (entry.kind === "questions") {
          return (
            <Questions
              agent={names[agent]}
              entry={entry}
              key={part.key}
              onReply={(answers) => resolve(chatKey, entry.ask, answers)}
            />
          );
        }
        return <Entry entry={entry} key={part.key} />;
      })}
    </div>
  );
};

const Feed = ({
  agent,
  chatKey,
  busy,
  story,
  groups,
  pinned,
  scroller,
  onPinned,
  onRetry,
  onReview,
  onCycle,
  top,
}: FeedProps) => (
  <div className="relative min-h-0 flex-1">
    {top}
    <div
      className="h-full overflow-y-auto select-text"
      onScroll={(event) => {
        const element = event.currentTarget;
        onPinned(
          element.scrollHeight - element.scrollTop - element.clientHeight < NEAR
        );
      }}
      ref={scroller}
    >
      {story.state === "loading" ? (
        <Notice late title="reading the chat" />
      ) : null}
      {story.state === "failed" ? (
        <Notice onRetry={onRetry} title="couldn't read this chat">
          <p>{story.message}</p>
        </Notice>
      ) : null}
      {story.state === "ready" && groups.length === 0 ? (
        <Notice title="nothing to show">
          <p>this chat has no messages yet</p>
        </Notice>
      ) : null}
      {groups.length > 0 ? (
        <Parts
          agent={agent}
          busy={busy}
          chatKey={chatKey}
          groups={groups}
          onCycle={onCycle}
          onReview={onReview}
        />
      ) : null}
    </div>
    {pinned ? null : (
      <button
        className="border-rim bg-raise text-paper hover:border-fog active:bg-well absolute bottom-3 left-1/2 -translate-x-1/2 cursor-pointer rounded-full border px-3 py-1 text-[12.5px]"
        onClick={() => onPinned(true)}
        type="button"
      >
        ↓ latest
      </button>
    )}
  </div>
);

interface Reviewing {
  scope: Scope;
  focus: string | null;
}

const useDrawer = (folder: Folder | null) => {
  const [open, setOpen] = useState(false);
  const swap = useCallback(() => setOpen((current) => !current), []);
  const ready = folder !== null;
  useEffect(() => {
    const keys = (event: KeyboardEvent) => {
      if (event.ctrlKey && event.code === "Backquote" && ready) {
        event.preventDefault();
        swap();
      }
    };
    window.addEventListener("keydown", keys);
    return () => window.removeEventListener("keydown", keys);
  }, [ready, swap]);
  return {
    docked: open ? folder : null,
    stow: () => setOpen(false),
    swap: ready ? swap : null,
  };
};

const useFind = (ready: boolean) => {
  const [finding, setFinding] = useState(false);
  useEffect(() => {
    const keys = (event: KeyboardEvent) => {
      if (pressed(event, "f") && ready) {
        event.preventDefault();
        setFinding(true);
      }
    };
    window.addEventListener("keydown", keys);
    return () => window.removeEventListener("keydown", keys);
  }, [ready]);
  return { finding, hide: () => setFinding(false) };
};

const useReview = (folder: Folder | null, latest: Scope | null) => {
  const [review, setReview] = useState<Reviewing | null>(null);
  const place = folder?.path ?? null;
  const flip = useCallback(
    () =>
      setReview((current) =>
        current ? null : { focus: null, scope: latest ?? { kind: "all" } }
      ),
    [latest]
  );
  useEffect(() => {
    const keys = (event: KeyboardEvent) => {
      if (pressed(event, "d", true) && place) {
        event.preventDefault();
        flip();
      }
    };
    window.addEventListener("keydown", keys);
    return () => window.removeEventListener("keydown", keys);
  }, [place, flip]);
  return {
    close: () => setReview(null),
    show: (scope: Scope, focus: string | null) => setReview({ focus, scope }),
    shown: review && place ? { ...review, place } : null,
    toggle: place ? flip : null,
  };
};

const Open = ({
  target,
  folders,
  installed,
  settings,
  hidden,
  tasks,
  onSidebar,
  onTarget,
}: Props & { target: Target }) => {
  const [story, setStory] = useState<Story>(() => {
    const cached = remembered(target.key);
    if (cached) {
      return { entries: cached, state: "ready" };
    }
    return target.id ? { state: "loading" } : { entries: [], state: "ready" };
  });
  const [chosen, setChosen] = useState<{
    model: string | null;
    mode: Mode;
    effort: string | null;
  }>({
    effort: settings?.effort ?? null,
    mode: settings?.mode ?? "ask",
    model: null,
  });
  const [started, setStarted] = useState(target.id !== null);
  const [pinned, setPinned] = useState(true);
  const scroller = useRef<HTMLDivElement>(null);
  const [room, frame] = useWidth<HTMLElement>();
  const live = useLive(target.key);
  const quiet = live.entries.length === 0 && !live.busy;
  const { agent, id, key, updated } = target;
  const historic = !key.startsWith("fresh:");

  const load = useCallback(async () => {
    if (!id || !historic) {
      return;
    }
    try {
      const entries = await read(agent, id);
      remember(key, entries, updated);
      setStory({ entries, state: "ready" });
    } catch (error) {
      setStory({ message: failure(error), state: "failed" });
    }
  }, [agent, id, key, historic, updated]);

  useEffect(() => {
    if (id) {
      begin({
        agent,
        effort: null,
        folder: null,
        id,
        key,
        mode: null,
        model: null,
      });
    }
  }, [agent, id, key]);

  useEffect(() => {
    if (remembered(key) && (!quiet || unchanged(key, updated))) {
      return;
    }
    const refresh = async () => {
      await load();
    };
    refresh();
  }, [load, quiet, key, updated]);

  const entries = useMemo(
    () => [...(story.state === "ready" ? story.entries : []), ...live.entries],
    [story, live.entries]
  );
  const groups = useMemo(() => group(entries), [entries]);
  const latest = useMemo(() => recent(entries), [entries]);
  const version = useMemo(() => rounds(live.entries), [live.entries]);
  const { close, shown, show, toggle } = useReview(target.folder, latest);
  const { finding, hide } = useFind(groups.length > 0);
  const { docked, stow, swap } = useDrawer(target.folder);
  const [phoning, setPhoning] = useState(false);
  const [following, setFollowing] = useState<string | null>(null);
  const recheck = useCallback(async () => {
    setFollowing(await followed().catch(() => null));
  }, []);

  useEffect(() => {
    const first = window.setTimeout(recheck, 0);
    window.addEventListener("focus", recheck);
    return () => {
      window.clearTimeout(first);
      window.removeEventListener("focus", recheck);
    };
  }, [recheck]);
  const missing = lack(installed, agent);

  useLayoutEffect(() => {
    const element = scroller.current;
    if (element && pinned && groups.length > 0) {
      element.scrollTop = element.scrollHeight;
    }
  }, [groups, pinned]);

  const ensure = async () => {
    if (!started && target.folder) {
      setStarted(true);
      await begin({
        agent,
        effort: chosen.effort,
        folder: target.folder.path,
        id: null,
        key,
        mode: chosen.mode,
        model: chosen.model,
      });
    }
  };

  const send = async (text: string, images: Picture[]) => {
    await ensure();
    setPinned(true);
    await submit(key, text, images);
  };

  const pursue = async (text: string) => {
    if (!started && text === "") {
      note(
        key,
        "no goal yet. /goal <what done looks like> starts one, like /goal `bun test` passes"
      );
      return;
    }
    if (!target.folder && !started) {
      note(key, "pick a folder first");
      return;
    }
    await ensure();
    setPinned(true);
    await aim(key, text);
  };

  const move = (to: Agent) => {
    const chatId = live.session ?? id;
    if (!started) {
      onTarget({ ...target, agent: to });
      return;
    }
    if (!chatId) {
      note(key, "wait for the first reply, then move the chat");
      return;
    }
    if (to === agent) {
      note(key, `this chat is already in ${names[to]}`);
      return;
    }
    if (target.folder) {
      tasks.carry(target, chatId, to);
    }
  };

  const model = live.model ?? chosen.model;
  const mode = started ? live.mode : chosen.mode;
  const effort = started ? live.effort : chosen.effort;
  const offered = live.levels.length > 0 ? live.levels : levels[agent];

  const tune = (
    change: { model?: string; mode?: Mode; effort?: string },
    silent = false
  ) => {
    if (started) {
      retune(key, change, silent);
      return;
    }
    setChosen((current) => ({
      effort: picked(change.effort, current.effort),
      mode: change.mode ?? current.mode,
      model: change.model ?? current.model,
    }));
  };

  const cycle = () => tune({ mode: rotate(mode) }, true);

  const status = () =>
    report({
      agent,
      chat: live.session ?? id,
      effort,
      folder: target.folder?.place ?? target.place,
      left: live.left,
      mode,
      model,
      spent: money(live.cost),
    });

  const command = (name: Name, rest: string) =>
    perform(name, rest, {
      changes: toggle,
      effort: offered,
      key,
      move,
      pursue: (text) => {
        pursue(text);
      },
      say: (text) => {
        send(text, []);
      },
      started,
      status,
      target,
      tasks,
      terminal: swap,
      tune,
    });

  const fresh = !target.id && entries.length === 0;
  const composer = (
    <Composer
      agent={agent}
      chatKey={key}
      folder={target.folder}
      effort={effort}
      key={live.restore}
      lead={null}
      levels={offered}
      live={live}
      mode={mode}
      model={model}
      onCommand={command}
      onCycle={cycle}
      onEffort={(level) => tune({ effort: level })}
      onMode={(next) => tune({ mode: next })}
      onModel={(next) => tune({ model: next })}
      onSend={send}
      placeholder={invite(agent, fresh, missing !== null)}
      ready={!missing && (target.id !== null || target.folder !== null)}
      steer={settings?.steer ?? false}
    />
  );

  const chores = {
    archive: () => tasks.archive(target),
    carry: move,
    compact: () => squash(key),
    copy: () => tasks.copy(`sombre resume ${live.session ?? id ?? ""}`),
    create: () => tasks.create(target.folder?.path),
    rename: (name: string) => retitle(key, name),
    reveal: () => (target.folder ? tasks.reveal(target.folder.path) : null),
  };

  if (fresh) {
    return (
      <main className="flex min-h-0 flex-col">
        <Header
          chores={chores}
          hidden={hidden}
          live={live}
          onOpen={null}
          onReview={null}
          onPhone={null}
          onTerminal={null}
          onSidebar={onSidebar}
          phoned={false}
          reviewing={false}
          target={target}
          version={0}
        />
        <div className="flex min-h-0 flex-1 flex-col justify-center gap-6 pb-[12vh]">
          <Home
            agent={agent}
            folder={target.folder}
            folders={folders}
            missing={missing}
            onAgent={(next) => onTarget({ ...target, agent: next })}
            onFolder={(next) =>
              onTarget({ ...target, folder: next, place: next.place })
            }
          />
          {composer}
        </div>
      </main>
    );
  }

  return (
    <main className="relative flex min-h-0" ref={frame}>
      <div className="flex min-h-0 min-w-0 flex-1 flex-col">
        <Header
          chores={chores}
          hidden={hidden}
          live={live}
          onOpen={
            target.folder
              ? () =>
                  launch(target.folder?.path ?? "", settings?.editor ?? null)
              : null
          }
          onPhone={() => setPhoning(true)}
          onReview={toggle}
          phoned={following === key}
          onTerminal={swap}
          onSidebar={onSidebar}
          reviewing={shown !== null}
          target={target}
          version={version}
        />
        <Feed
          agent={agent}
          busy={live.busy}
          chatKey={key}
          groups={groups}
          onCycle={cycle}
          onPinned={setPinned}
          onRetry={load}
          onReview={show}
          pinned={pinned}
          scroller={scroller}
          story={story}
          top={finding ? <Find onClose={hide} root={scroller} /> : null}
        />
        {composer}
        <Phone
          chatKey={key}
          onClose={() => {
            setPhoning(false);
            recheck();
          }}
          open={phoning}
        />
        {docked ? (
          <Suspense fallback={null}>
            <Drawer
              chatKey={key}
              folder={docked.path}
              onClose={stow}
              place={docked.place}
            />
          </Suspense>
        ) : null}
      </div>
      {shown ? (
        <Review
          focus={shown.focus}
          folder={shown.place}
          hidden={hidden}
          latest={latest}
          overlay={room < 760}
          onClose={close}
          onOpen={(path) => {
            launch(`${shown.place}/${path}`, settings?.editor ?? null);
          }}
          onScope={(scope) => show(scope, null)}
          scope={shown.scope}
          version={version}
        />
      ) : null}
    </main>
  );
};

export const Thread = (props: Props) => {
  if (!props.target) {
    return (
      <Start
        hidden={props.hidden}
        onCreate={() => props.tasks.create()}
        onPalette={props.onPalette}
        onPick={props.onPick}
        onSettings={() => props.tasks.settings()}
        onSidebar={props.onSidebar}
        recent={props.recent}
      />
    );
  }
  return <Open {...props} key={props.target.key} target={props.target} />;
};
