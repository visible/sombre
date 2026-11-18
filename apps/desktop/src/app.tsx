import { listen } from "@tauri-apps/api/event";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import {
  Suspense,
  lazy,
  useCallback,
  useEffect,
  useMemo,
  useState,
} from "react";

import { Controls } from "./components/controls";
import { Logo } from "./components/logo";
import type { Entry } from "./components/palette";
import { Palette } from "./components/palette";
import { Resizer } from "./components/resizer";
import type { Actions, Listing } from "./components/sidebar";
import { Sidebar } from "./components/sidebar";
import type { Tasks } from "./components/thread";
import { Thread } from "./components/thread";
import { Toasts } from "./components/toasts";
import { Wipe } from "./components/wipe";
import { badge, notify } from "./lib/alert";
import type { Agent, Change, Chat, Folder, Settings } from "./lib/api";
import {
  agents,
  chats,
  take,
  configure,
  defaults,
  failure,
  folder as describe,
  names,
  settings as fetchSettings,
} from "./lib/api";
import type { Target } from "./lib/feed";
import { fresh, target } from "./lib/feed";
import { useInstalled } from "./lib/installed";
import { marks } from "./lib/live";
import { mac, pressed, shortcut } from "./lib/platform";
import { SAVED as LAST, pick as recall, save } from "./lib/resume";
import type { Prefs } from "./lib/sidebar";
import { keyed, parse } from "./lib/sidebar";
import {
  alerts,
  begin,
  forget,
  halt,
  keyOf,
  move,
  note,
  retitle,
  squash,
  useBusy,
  useMarks,
  view as watching,
  warm,
  watch,
} from "./lib/store";
import { toast } from "./lib/toasts";
import { useTrail } from "./lib/trail";
import { useZoom } from "./lib/zoom";

const Preferences = lazy(async () => {
  const loaded = await import("./components/settings");
  return { default: loaded.Preferences };
});

let resumed = false;
let warmed = false;

const SAVED = "sombre.prefs";

const action = (entry: Omit<Entry, "group">): Entry => ({
  ...entry,
  group: "actions",
});

export const App = () => {
  const [listing, setListing] = useState<Listing>({ state: "loading" });
  const [open, setOpen] = useState<Target | null>(null);
  const [folders, setFolders] = useState<Folder[]>([]);
  const [settings, setSettings] = useState<Settings | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const [prefs, setPrefs] = useState<Prefs>(() =>
    parse(localStorage.getItem(SAVED))
  );
  const [view, setView] = useState<"chat" | "settings">("chat");
  const [section, setSection] = useState<string | null>(null);
  const [palette, setPalette] = useState(false);
  const [doomed, setDoomed] = useState<{ folder: string; name: string } | null>(
    null
  );

  const busy = useBusy();
  const installed = useInstalled();
  const place = open?.folder?.path ?? null;
  useZoom();
  const walk = useCallback((next: Target) => {
    setView("chat");
    setOpen(next);
  }, []);
  useTrail(open, walk);

  useEffect(() => {
    if (open?.id) {
      localStorage.setItem(LAST, save(open.agent, open.id));
    }
  }, [open]);

  useEffect(() => {
    localStorage.setItem(SAVED, JSON.stringify(prefs));
  }, [prefs]);

  const load = useCallback(async () => {
    try {
      const found = await chats();
      setListing({ chats: found, state: "ready" });
      if (!warmed) {
        warmed = true;
        warm(found).catch(() => null);
      }
      const last = resumed ? null : recall(localStorage.getItem(LAST), found);
      resumed = true;
      setOpen((current) => {
        if (!current) {
          return last ? target(last, keyOf(last.agent, last.id)) : null;
        }
        const match = found.find(
          (chat) => keyOf(chat.agent, chat.id) === current.key
        );
        return match ? target(match, current.key) : current;
      });
      const known = await defaults();
      setFolders(known.folders);
    } catch (error) {
      setListing((current) =>
        current.state === "ready"
          ? current
          : { message: failure(error), state: "failed" }
      );
    }
  }, []);

  const configureWith = useCallback(
    async (change: Change) => {
      try {
        setSettings(await configure(change, place));
        setProblem(null);
      } catch (error) {
        setProblem(failure(error));
      }
    },
    [place]
  );

  const edit = useCallback(
    async (call: (place: string | null) => Promise<Settings>) => {
      try {
        setSettings(await call(place));
        setProblem(null);
        return null;
      } catch (error) {
        return failure(error);
      }
    },
    [place]
  );

  useEffect(() => {
    const read = async () => {
      try {
        setSettings(await fetchSettings(place));
        setProblem(null);
      } catch (error) {
        setProblem(failure(error));
      }
    };
    read();
  }, [place]);

  useEffect(() => {
    const going = listen<string>("continue", async ({ payload: key }) => {
      const found = await chats().catch(() => []);
      setListing({ chats: found, state: "ready" });
      const chosen = found.find((chat) => keyOf(chat.agent, chat.id) === key);
      if (!chosen) {
        return;
      }
      setOpen(target(chosen, key));
      setView("chat");
      await begin({
        agent: chosen.agent,
        effort: null,
        folder: null,
        id: chosen.id,
        key,
        mode: null,
        model: null,
      });
      await take(key).catch(() => null);
    });
    return () => {
      going.then((stop) => stop()).catch(() => null);
    };
  }, []);

  useEffect(() => {
    watch(load);
    const refresh = async () => {
      await load();
    };
    refresh();
    window.addEventListener("focus", refresh);
    return () => window.removeEventListener("focus", refresh);
  }, [load]);

  const create = useCallback(
    async (path?: string) => {
      const agent = settings?.agent ?? "claude";
      const known =
        folders.find((entry) => entry.path === path) ??
        (path ? null : folders[0]);
      let picked = known ?? null;
      if (path && !known) {
        try {
          picked = await describe(path);
        } catch {
          picked = null;
        }
      }
      setView("chat");
      setOpen(fresh(agent, picked, Date.now()));
    },
    [folders, settings]
  );

  const pick = useCallback((chat: Chat) => {
    setView("chat");
    setOpen(target(chat, keyOf(chat.agent, chat.id)));
  }, []);

  const carry = useCallback(async (chosen: Target, id: string, to: Agent) => {
    if (!chosen.folder) {
      return;
    }
    const moved = await move(
      { agent: chosen.agent, folder: chosen.folder.path, key: chosen.key },
      id,
      to
    );
    if (!moved) {
      return;
    }
    const before = `${chosen.agent}:${id}`;
    const after = `${to}:${moved.session}`;
    const next = keyOf(to, moved.session);
    const found = await chats().catch(() => null);
    const match = found?.find(
      (chat) => chat.agent === to && chat.id === moved.session
    );
    forget(chosen.key);
    forget(next);
    setPrefs((current) => ({
      ...current,
      pinned: current.pinned.map((entry) => (entry === before ? after : entry)),
    }));
    if (found) {
      setListing({ chats: found, state: "ready" });
    }
    setOpen(
      match
        ? target(match, next)
        : { ...chosen, agent: to, id: moved.session, key: next }
    );
    setView("chat");
    note(next, moved.note);
  }, []);

  const tasks: Tasks = useMemo(
    () => ({
      archive: (chosen) => {
        if (!chosen.id) {
          return;
        }
        const saved = `${chosen.agent}:${chosen.id}`;
        setPrefs((current) => ({
          ...current,
          archived: current.archived.includes(saved)
            ? current.archived
            : [...current.archived, saved],
          pinned: current.pinned.filter((entry) => entry !== saved),
        }));
        setOpen((current) => (current?.key === chosen.key ? null : current));
      },
      carry: (chosen, id, to) => {
        carry(chosen, id, to);
      },
      copy: (text) => {
        navigator.clipboard.writeText(text);
      },
      create: (path) => {
        create(path);
      },
      reveal: (path) => {
        revealItemInDir(path);
      },
      settings: (part) => {
        setSection(part ?? null);
        setView("settings");
      },
    }),
    [create, carry]
  );

  const actions: Actions = useMemo(
    () => ({
      copy: tasks.copy,
      create: tasks.create,
      open: pick,
      purge: (project) => {
        setDoomed({ folder: project.folder, name: project.name });
      },
      rename: async (chat, name) => {
        const chatKey = keyOf(chat.agent, chat.id);
        await begin({
          agent: chat.agent,
          effort: null,
          folder: null,
          id: chat.id,
          key: chatKey,
          mode: null,
          model: null,
        });
        await retitle(chatKey, name);
      },
      retry: () => {
        load();
      },
      reveal: tasks.reveal,
      settings: tasks.settings,
    }),
    [tasks, pick, load]
  );

  useEffect(() => {
    const keys = (event: KeyboardEvent) => {
      if (pressed(event, "k")) {
        event.preventDefault();
        setPalette((current) => !current);
      } else if (pressed(event, "n")) {
        event.preventDefault();
        create(open?.folder?.path);
      } else if (pressed(event, ",")) {
        event.preventDefault();
        setView((current) => (current === "settings" ? "chat" : "settings"));
      } else if (pressed(event, "/")) {
        event.preventDefault();
        setSection("shortcuts");
        setView("settings");
      } else if (pressed(event, "b")) {
        event.preventDefault();
        setPrefs((current) => ({ ...current, hidden: !current.hidden }));
      } else if (event.key === "Escape" && view === "settings" && !palette) {
        setView("chat");
      }
    };
    window.addEventListener("keydown", keys);
    return () => window.removeEventListener("keydown", keys);
  }, [create, open, view, palette]);

  const all = useMemo(
    () => (listing.state === "ready" ? listing.chats : []),
    [listing]
  );
  const working = new Set(busy.split("\n"));
  const flags = useMarks();
  const waiting = [...marks(flags).values()].filter(
    (flag) => flag !== "busy"
  ).length;

  useEffect(() => {
    watching(view === "chat" ? (open?.key ?? null) : null);
  }, [open, view]);

  useEffect(() => {
    badge(waiting);
  }, [waiting]);

  useEffect(() => {
    alerts((key, event) => {
      const chat = all.find((entry) => keyOf(entry.agent, entry.id) === key);
      const agent = chat?.agent ?? open?.agent ?? "claude";
      const title = chat?.title ?? open?.title ?? "a chat";
      const said = {
        ask: `${names[agent]} needs your ok`,
        done: `${names[agent]} finished`,
        failed: `${names[agent]} stopped with an error`,
      }[event];
      toast({
        action: {
          label: "open",
          run: () => (chat ? pick(chat) : setView("chat")),
        },
        body: title,
        title: said,
      });
      if (!document.hasFocus() && settings?.notify !== false) {
        notify(title, said);
      }
    });
  }, [all, open, pick, settings]);
  const recent = all
    .filter((chat) => !prefs.archived.includes(keyed(chat)))
    .slice(0, 6);

  const cleared = (folder: string, gone: Chat[]) => {
    const keys = new Set(gone.map(keyed));
    for (const chat of gone) {
      forget(keyOf(chat.agent, chat.id));
    }
    setPrefs((current) => ({
      ...current,
      archived: current.archived.filter((entry) => !keys.has(entry)),
      folded: current.folded.filter((entry) => entry !== folder),
      opened: current.opened.filter((entry) => entry !== folder),
      pinned: current.pinned.filter((entry) => !keys.has(entry)),
    }));
    setOpen((current) => (current?.folder?.path === folder ? null : current));
    setDoomed(null);
    load();
  };

  const entries: Entry[] = [
    action({
      icon: "+",
      id: "new",
      label: "new chat",
      run: () => create(open?.folder?.path),
      shortcut: shortcut("n"),
    }),
    ...folders.slice(0, 6).map((entry) =>
      action({
        hint: entry.place,
        icon: "+",
        id: `new:${entry.path}`,
        label: `new chat in ${entry.project}`,
        run: () => create(entry.path),
      })
    ),
    action({
      icon: "⚙",
      id: "settings",
      label: "settings",
      run: () => tasks.settings(),
      shortcut: shortcut(","),
    }),
    action({
      icon: "⚙",
      id: "providers",
      label: "add a provider",
      run: () => tasks.settings("providers"),
    }),
    action({
      icon: "⚙",
      id: "servers",
      label: "mcp servers",
      run: () => tasks.settings("mcp servers"),
    }),
    action({
      icon: "▤",
      id: "sidebar",
      label: prefs.hidden ? "show the sidebar" : "hide the sidebar",
      run: () =>
        setPrefs((current) => ({ ...current, hidden: !current.hidden })),
      shortcut: shortcut("b"),
    }),
    ...(open?.id
      ? [
          action({
            icon: "≡",
            id: "compact",
            label: "compact this chat",
            run: () => squash(open.key),
          }),
          action({
            icon: "↗",
            id: "reveal",
            label: "show this folder in finder",
            run: () => open.folder && tasks.reveal(open.folder.path),
          }),
          action({
            icon: "⧉",
            id: "copy",
            label: "copy resume command",
            run: () => tasks.copy(`sombre resume ${open.id}`),
          }),
          ...agents
            .filter((agent) => agent !== open.agent)
            .map((agent) =>
              action({
                icon: <Logo agent={agent} />,
                id: `move:${agent}`,
                label: `move this chat to ${names[agent]}`,
                run: () => open.id && carry(open, open.id, agent),
              })
            ),
          action({
            icon: "↓",
            id: "archive",
            label: "archive this chat",
            run: () => tasks.archive(open),
          }),
          action({
            icon: "?",
            id: "help",
            label: "keys and commands",
            run: () =>
              note(open.key, "type / in the message box for every command"),
          }),
        ]
      : []),
    ...(open && working.has(open.key)
      ? [
          action({
            icon: "■",
            id: "stop",
            label: "stop the turn",
            run: () => halt(open.key),
          }),
        ]
      : []),
    ...all
      .filter((chat) => !prefs.archived.includes(keyed(chat)))
      .map((chat): Entry => ({
        group: "chats",
        hint: `${chat.project} · ${names[chat.agent]} · ${chat.ago}`,
        icon: <Logo agent={chat.agent} />,
        id: keyed(chat),
        label: chat.title,
        run: () => pick(chat),
      })),
  ];

  return (
    <div
      className="grid h-dvh"
      style={{
        gridTemplateColumns:
          prefs.hidden || view === "settings"
            ? "minmax(0,1fr)"
            : `${prefs.width}px minmax(0,1fr)`,
      }}
    >
      {prefs.hidden || view === "settings" ? null : (
        <div className="relative grid min-h-0 min-w-0 grid-cols-[minmax(0,1fr)]">
          <Sidebar
            actions={actions}
            current={view === "chat" ? (open?.key ?? null) : null}
            listing={listing}
            onPrefs={setPrefs}
            prefs={prefs}
          />
          <Resizer
            onWidth={(width) => setPrefs((current) => ({ ...current, width }))}
            width={prefs.width}
          />
        </div>
      )}
      {view === "settings" ? (
        <Suspense fallback={null}>
          <Preferences
            chats={all}
            installed={installed}
            key={section}
            onChange={configureWith}
            onClose={() => setView("chat")}
            onEdit={edit}
            onPrefs={setPrefs}
            onReveal={(path) => tasks.reveal(path)}
            prefs={prefs}
            problem={problem}
            section={section}
            settings={settings}
            width={prefs.width}
          />
        </Suspense>
      ) : (
        <Thread
          folders={folders}
          hidden={prefs.hidden}
          installed={installed}
          onPalette={() => setPalette(true)}
          onPick={pick}
          recent={recent}
          onSidebar={() =>
            setPrefs((current) => ({ ...current, hidden: false }))
          }
          onTarget={setOpen}
          settings={settings}
          target={open}
          tasks={tasks}
        />
      )}
      <Palette
        entries={entries}
        onClose={() => setPalette(false)}
        open={palette}
      />
      <Wipe
        chats={all}
        onClose={() => setDoomed(null)}
        onDone={cleared}
        target={doomed}
      />
      <Toasts />
      {mac ? null : <Controls />}
    </div>
  );
};
