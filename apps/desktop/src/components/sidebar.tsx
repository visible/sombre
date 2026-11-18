import type { KeyboardEvent, ReactNode } from "react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import type { Chat } from "../lib/api";
import { names } from "../lib/api";
import {
  left,
  settle,
  snooze,
  unread,
  unsettle,
  wake,
  wakes,
  woke,
} from "../lib/inbox";
import { initials } from "../lib/initials";
import type { Flag } from "../lib/live";
import { marks } from "../lib/live";
import { mac, pressed, shortcut } from "../lib/platform";
import type { Place, Prefs, Project } from "../lib/sidebar";
import {
  keyed,
  places,
  sections,
  shelves,
  short,
  toggle,
} from "../lib/sidebar";
import { keyOf, seenAt, useMarks, useSeen } from "../lib/store";
import { Logo } from "./logo";
import { Mark } from "./mark";
import { Notice } from "./notice";
import { Divider, Option, Popover } from "./popover";
import { Update } from "./update";

export type Listing =
  | { state: "loading" }
  | { state: "failed"; message: string }
  | { state: "ready"; chats: Chat[] };

export interface Actions {
  open: (chat: Chat) => void;
  create: (folder?: string) => void;
  rename: (chat: Chat, name: string) => void;
  reveal: (folder: string) => void;
  copy: (text: string) => void;
  settings: () => void;
  retry: () => void;
  purge: (place: Pick<Place, "folder" | "name">) => void;
}

interface Props {
  listing: Listing;
  current: string | null;
  prefs: Prefs;
  actions: Actions;
  onPrefs: (prefs: Prefs) => void;
}

const SETTLED = 10;
const MORE = 25;

const move = (event: KeyboardEvent<HTMLButtonElement>) => {
  if (event.key !== "ArrowDown" && event.key !== "ArrowUp") {
    return;
  }
  const rows = [
    ...document.querySelectorAll<HTMLButtonElement>("button[data-chat]"),
  ];
  const at = rows.indexOf(event.currentTarget);
  const next =
    event.key === "ArrowDown"
      ? Math.min(at + 1, rows.length - 1)
      : Math.max(at - 1, 0);
  event.preventDefault();
  rows[next]?.focus();
};

const useNow = () => {
  const [now, setNow] = useState(() => Math.floor(Date.now() / 1000));
  useEffect(() => {
    const timer = window.setInterval(
      () => setNow(Math.floor(Date.now() / 1000)),
      30_000
    );
    return () => window.clearInterval(timer);
  }, []);
  return now;
};

const icon =
  "text-fog hover:bg-raise hover:text-paper active:bg-well flex cursor-pointer items-center justify-center rounded-lg";

const Tile = ({ name }: { name: string }) => (
  <span
    aria-hidden="true"
    className="border-rim bg-raise text-fog flex h-4.5 w-5 shrink-0 items-center justify-center rounded border text-[9px] font-semibold tracking-tight"
  >
    {initials(name)}
  </span>
);

interface Note {
  text: string;
  tone: "ember" | "fog";
}

const Status = ({
  mark,
  ago,
  note,
}: {
  mark: Flag | null;
  ago: string;
  note: Note | null;
}) => {
  if (mark === "busy") {
    return (
      <span className="text-ember" title="working">
        <span aria-hidden="true">●</span>
        <span className="sr-only">working</span>
      </span>
    );
  }
  if (mark === "ask") {
    return <span className="text-ember">needs you</span>;
  }
  if (mark === "failed") {
    return <span className="text-paper">failed</span>;
  }
  if (mark === "new") {
    return <span className="text-ember">new</span>;
  }
  if (note) {
    return (
      <span className={note.tone === "ember" ? "text-ember" : "text-fog"}>
        {note.text}
      </span>
    );
  }
  return <span className="text-fog">{short(ago)}</span>;
};

export interface Tray {
  shelf: "inbox" | "snoozed" | "settled" | "pinned";
  now: number;
  onSettle: () => void;
  onUnsettle: () => void;
  onSnooze: (until: number) => void;
  onWake: () => void;
}

const Check = () => (
  <svg
    aria-hidden="true"
    className="size-3.5"
    fill="none"
    stroke="currentColor"
    strokeLinecap="round"
    strokeLinejoin="round"
    strokeWidth="1.5"
    viewBox="0 0 16 16"
  >
    <path d="m3.5 8.5 3 3 6-7" />
  </svg>
);

const Back = () => (
  <svg
    aria-hidden="true"
    className="size-3.5"
    fill="none"
    stroke="currentColor"
    strokeLinecap="round"
    strokeLinejoin="round"
    strokeWidth="1.4"
    viewBox="0 0 16 16"
  >
    <path d="M6 4 3 7l3 3" />
    <path d="M3.5 7H10a3 3 0 0 1 0 6H8" />
  </svg>
);

const Pane = ({ children }: { children: ReactNode }) => {
  const box = useRef<HTMLDivElement>(null);
  useEffect(() => {
    box.current?.querySelector<HTMLElement>("[data-option]")?.focus();
  }, []);
  return (
    <div className="flex min-h-0 flex-col" ref={box}>
      {children}
    </div>
  );
};

const Snoozes = ({
  tray,
  close,
  onBack,
}: {
  tray: Tray;
  close: () => void;
  onBack: () => void;
}) => (
  <Pane key="snooze">
    <Option icon="‹" label="back" onPick={onBack} />
    <Divider />
    {wakes(new Date(tray.now * 1000)).map((option) => (
      <Option
        hint={new Date(option.until * 1000).toLocaleString([], {
          hour: "numeric",
          minute: "2-digit",
          weekday: "short",
        })}
        key={option.label}
        label={option.label}
        onPick={() => {
          close();
          tray.onSnooze(option.until);
        }}
      />
    ))}
  </Pane>
);

const Sorting = ({ tray, close }: { tray: Tray; close: () => void }) => {
  if (tray.shelf === "pinned") {
    return null;
  }
  const settled = tray.shelf === "settled";
  return (
    <>
      <Option
        icon={settled ? <Back /> : <Check />}
        label={settled ? "back to the inbox" : "settle"}
        onPick={() => {
          close();
          if (settled) {
            tray.onUnsettle();
          } else {
            tray.onSettle();
          }
        }}
        shortcut={settled ? undefined : shortcut("s", true)}
      />
      {tray.shelf === "snoozed" ? (
        <Option
          icon="◷"
          label="wake it now"
          onPick={() => {
            close();
            tray.onWake();
          }}
        />
      ) : null}
    </>
  );
};

interface RowProps {
  chat: Chat;
  current: boolean;
  mark: Flag | null;
  pinned: boolean;
  actions: Actions;
  tile: boolean;
  dim: boolean;
  note: Note | null;
  tray: Tray | null;
  onPin: () => void;
  onArchive: () => void;
}

const Menu = ({
  chat,
  pinned,
  actions,
  tray,
  onPin,
  onArchive,
  onRename,
  close,
}: Pick<
  RowProps,
  "chat" | "pinned" | "actions" | "tray" | "onPin" | "onArchive"
> & {
  onRename: () => void;
  close: () => void;
}) => {
  const [step, setStep] = useState<"main" | "snooze">("main");
  if (tray && step === "snooze") {
    return <Snoozes close={close} onBack={() => setStep("main")} tray={tray} />;
  }
  return (
    <Pane key="main">
      {tray ? (
        <>
          <Sorting close={close} tray={tray} />
          {tray.shelf === "pinned" ? null : (
            <Option
              hint="it comes back on its own"
              icon="☾"
              label="snooze"
              onPick={() => setStep("snooze")}
            />
          )}
          {tray.shelf === "pinned" ? null : <Divider />}
        </>
      ) : null}
      <Option
        icon="◆"
        label={pinned ? "unpin" : "pin"}
        onPick={() => {
          onPin();
          close();
        }}
      />
      <Option
        icon="✎"
        label="rename"
        onPick={() => {
          close();
          onRename();
        }}
        shortcut="F2"
      />
      <Option
        icon="+"
        label="new chat in this folder"
        onPick={() => {
          close();
          actions.create(chat.folder);
        }}
      />
      <Divider />
      <Option
        icon="↗"
        label="show in finder"
        onPick={() => {
          close();
          actions.reveal(chat.folder);
        }}
      />
      <Option
        icon="⧉"
        label="copy resume command"
        onPick={() => {
          close();
          actions.copy(`sombre resume ${chat.id}`);
        }}
      />
      <Divider />
      <Option
        hint="restore it from settings"
        icon="↓"
        label="archive"
        onPick={() => {
          close();
          onArchive();
        }}
      />
    </Pane>
  );
};

const tone = (current: boolean, dim: boolean) => {
  if (current) {
    return "bg-well text-paper";
  }
  return dim
    ? "text-fog hover:bg-raise hover:text-body active:bg-well"
    : "text-paper hover:bg-raise active:bg-well";
};

const Quick = ({ tray }: { tray: Tray | null }) => {
  if (!tray || tray.shelf === "pinned" || tray.shelf === "snoozed") {
    return null;
  }
  const settled = tray.shelf === "settled";
  return (
    <button
      aria-label={settled ? "back to the inbox" : "settle"}
      className={`${icon} size-6`}
      onClick={settled ? tray.onUnsettle : tray.onSettle}
      title={settled ? "back to the inbox" : `settle ${shortcut("s", true)}`}
      type="button"
    >
      {settled ? <Back /> : <Check />}
    </button>
  );
};

const Row = (props: RowProps) => {
  const { chat, current, mark, actions, tile, dim, note, tray } = props;
  const [editing, setEditing] = useState(false);
  const [menu, setMenu] = useState(false);
  const [name, setName] = useState(chat.title);
  const field = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (editing) {
      field.current?.select();
    }
  }, [editing]);

  const edit = () => {
    setName(chat.title);
    setEditing(true);
  };

  const commit = () => {
    setEditing(false);
    if (name.trim() !== "" && name.trim() !== chat.title) {
      actions.rename(chat, name.trim());
    }
  };

  if (editing) {
    return (
      <input
        aria-label={`rename ${chat.title}`}
        className="border-edge bg-well text-paper h-8 w-full rounded-md border px-2 text-[13px] outline-none"
        onBlur={commit}
        onChange={(event) => setName(event.target.value)}
        onKeyDown={(event) => {
          if (event.key === "Enter") {
            commit();
          } else if (event.key === "Escape") {
            setName(chat.title);
            setEditing(false);
          }
        }}
        ref={field}
        spellCheck={false}
        value={name}
      />
    );
  }

  const quick = tray && tray.shelf !== "pinned" && tray.shelf !== "snoozed";

  return (
    <div className="group relative">
      <button
        aria-current={current ? "true" : undefined}
        className={`flex h-8 w-full cursor-pointer items-center gap-2 rounded-md pl-2 text-left text-[13px] ${quick ? "pr-16" : "pr-9"} ${tone(current, dim)}`}
        data-chat=""
        onClick={() => actions.open(chat)}
        onContextMenu={(event) => {
          event.preventDefault();
          setMenu(true);
        }}
        onDoubleClick={edit}
        onKeyDown={(event) => {
          if (event.key === "F2") {
            edit();
          } else {
            move(event);
          }
        }}
        title={tile ? `${chat.project}  ${names[chat.agent]}` : undefined}
        type="button"
      >
        {tile ? (
          <Tile name={chat.project} />
        ) : (
          <span
            className={`flex w-3.5 shrink-0 justify-center ${current ? "" : "opacity-50 group-hover:opacity-100"}`}
          >
            <Logo agent={chat.agent} />
          </span>
        )}
        <span className="sr-only">{names[chat.agent]}</span>
        <span
          className={`min-w-0 flex-1 truncate ${mark === "new" || mark === "failed" ? "font-semibold" : ""}`}
        >
          {chat.title}
        </span>
      </button>
      <span
        className={`pointer-events-none absolute top-0 right-2 flex h-8 items-center text-[11.5px] group-focus-within:opacity-0 group-hover:opacity-0 ${menu ? "opacity-0" : ""}`}
      >
        <Status ago={chat.ago} mark={mark} note={note} />
      </span>
      <span
        className={`absolute top-1 right-1 flex items-center ${menu ? "opacity-100" : "opacity-0 group-focus-within:opacity-100 group-hover:opacity-100"}`}
      >
        <Quick tray={tray} />
        <Popover
          align="start"
          className={`${icon} aria-expanded:bg-well aria-expanded:text-paper size-6`}
          label={`actions for ${chat.title}`}
          onOpen={setMenu}
          open={menu}
          side="bottom"
          trigger={<span aria-hidden="true">···</span>}
          width="w-72"
        >
          {(close) => <Menu {...props} close={close} onRename={edit} />}
        </Popover>
      </span>
    </div>
  );
};

interface GroupProps {
  project: Project;
  folded: boolean;
  opened: boolean;
  children: ReactNode;
  onFold: () => void;
  onMore: () => void;
  onCreate: () => void;
  onReveal: () => void;
  onPurge: () => void;
}

const Group = ({
  project,
  folded,
  opened,
  children,
  onFold,
  onMore,
  onCreate,
  onReveal,
  onPurge,
}: GroupProps) => {
  const [menu, setMenu] = useState(false);
  return (
    <section className="flex flex-col gap-px">
      <div
        className="group flex h-7 items-center rounded-md pr-1"
        onContextMenu={(event) => {
          event.preventDefault();
          setMenu(true);
        }}
      >
        <button
          aria-expanded={!folded}
          className="text-fog hover:text-body flex h-7 min-w-0 flex-1 cursor-pointer items-center gap-1.5 rounded-md px-2 text-left text-[12px]"
          onClick={onFold}
          title={project.place}
          type="button"
        >
          <span aria-hidden="true" className="w-2.5 text-[9px]">
            {folded ? "▸" : "▾"}
          </span>
          <span className="truncate">{project.name}</span>
        </button>
        <span
          className={
            menu
              ? "opacity-100"
              : "opacity-0 group-focus-within:opacity-100 group-hover:opacity-100"
          }
        >
          <Popover
            align="start"
            className={`${icon} aria-expanded:bg-well aria-expanded:text-paper size-6`}
            label={`actions for ${project.name}`}
            onOpen={setMenu}
            open={menu}
            side="bottom"
            trigger={<span aria-hidden="true">···</span>}
            width="w-72"
          >
            {(close) => (
              <>
                <Option
                  icon="+"
                  label="new chat here"
                  onPick={() => {
                    close();
                    onCreate();
                  }}
                />
                <Option
                  icon="↗"
                  label="show in finder"
                  onPick={() => {
                    close();
                    onReveal();
                  }}
                />
                <Divider />
                <Option
                  hint="every agent. asks first"
                  icon="✕"
                  label="delete all its chats"
                  onPick={() => {
                    close();
                    onPurge();
                  }}
                />
              </>
            )}
          </Popover>
        </span>
        <button
          aria-label={`new chat in ${project.name}`}
          className={`${icon} size-6 opacity-0 group-hover:opacity-100 focus-visible:opacity-100`}
          onClick={onCreate}
          type="button"
        >
          +
        </button>
      </div>
      {folded ? null : (
        <>
          <ul className="flex flex-col gap-px">{children}</ul>
          {project.more > 0 || opened ? (
            <button
              className="text-fog hover:text-body cursor-pointer self-start rounded-md py-1 pr-2 pl-[1.85rem] text-[12px]"
              onClick={onMore}
              type="button"
            >
              {opened && project.more === 0
                ? "show less"
                : `show ${project.more} more`}
            </button>
          ) : null}
        </>
      )}
    </section>
  );
};

const Chevron = ({ open }: { open: boolean }) => (
  <svg
    aria-hidden="true"
    className={`size-3 transition-transform motion-reduce:transition-none ${open ? "rotate-90" : ""}`}
    fill="none"
    stroke="currentColor"
    strokeLinecap="round"
    strokeLinejoin="round"
    strokeWidth="1.5"
    viewBox="0 0 16 16"
  >
    <path d="m6 4 4 4-4 4" />
  </svg>
);

interface ShelfProps {
  label: string;
  count: number;
  open: boolean;
  children: ReactNode;
  onToggle: () => void;
}

const Stack = ({ label, count, open, children, onToggle }: ShelfProps) =>
  count === 0 ? null : (
    <section className="flex flex-col gap-px">
      <button
        aria-expanded={open}
        className="text-fog hover:text-body flex h-7 cursor-pointer items-center gap-1 rounded-md px-2 text-left text-[12px]"
        onClick={onToggle}
        type="button"
      >
        <span>{label}</span>
        <Chevron open={open} />
        <span className="ml-auto tabular-nums">{count}</span>
      </button>
      {open ? children : null}
    </section>
  );

const Folders = () => (
  <svg
    aria-hidden="true"
    className="size-4"
    fill="none"
    stroke="currentColor"
    strokeLinejoin="round"
    strokeWidth="1.3"
    viewBox="0 0 16 16"
  >
    <path d="M2 4.5A1.5 1.5 0 0 1 3.5 3h3l1.5 1.5h4.5A1.5 1.5 0 0 1 14 6v5.5a1.5 1.5 0 0 1-1.5 1.5h-9A1.5 1.5 0 0 1 2 11.5z" />
  </svg>
);

interface PlacesProps {
  chats: Chat[];
  prefs: Prefs;
  actions: Actions;
  onPrefs: (prefs: Prefs) => void;
}

const Places = ({ chats, prefs, actions, onPrefs }: PlacesProps) => {
  const listed = useMemo(() => places(chats, prefs), [chats, prefs]);
  const chosen = listed.find((place) => prefs.only.includes(place.folder));
  const label = chosen && prefs.only.length === 1 ? chosen.name : "all";
  return (
    <Popover
      align="start"
      className="text-body hover:bg-raise hover:text-paper active:bg-well aria-expanded:bg-raise flex h-8 w-full cursor-pointer items-center gap-2 rounded-lg px-2 text-left text-[13px]"
      label="projects"
      side="bottom"
      trigger={
        <>
          <Folders />
          <span className="flex-1">projects</span>
          <span className="text-fog max-w-28 truncate text-[12px]">
            {label}
          </span>
        </>
      }
      width="w-72"
    >
      {(close) => (
        <>
          <Option
            label="all projects"
            onPick={() => {
              close();
              onPrefs({ ...prefs, only: [] });
            }}
            selected={prefs.only.length === 0}
          />
          <Divider />
          <div className="flex min-h-0 flex-col overflow-y-auto">
            {listed.map((place) => (
              <Option
                hint={place.place}
                icon={<Tile name={place.name} />}
                key={place.folder}
                label={place.name}
                onPick={() => {
                  close();
                  onPrefs({ ...prefs, only: [place.folder] });
                }}
                selected={prefs.only.includes(place.folder)}
              />
            ))}
          </div>
          {chosen && prefs.only.length === 1 ? (
            <>
              <Divider />
              <Option
                icon="+"
                label={`new chat in ${chosen.name}`}
                onPick={() => {
                  close();
                  actions.create(chosen.folder);
                }}
              />
              <Option
                icon="↗"
                label="show in finder"
                onPick={() => {
                  close();
                  actions.reveal(chosen.folder);
                }}
              />
              <Option
                hint="every agent. asks first"
                icon="✕"
                label="delete all its chats"
                onPick={() => {
                  close();
                  actions.purge(chosen);
                }}
              />
            </>
          ) : null}
          <Divider />
          <Option
            hint="the old layout, one group per folder"
            label="group by project"
            onPick={() => {
              close();
              onPrefs({ ...prefs, grouped: !prefs.grouped });
            }}
            selected={prefs.grouped}
          />
        </>
      )}
    </Popover>
  );
};

const Pencil = () => (
  <svg
    aria-hidden="true"
    className="size-4"
    fill="none"
    stroke="currentColor"
    strokeLinecap="round"
    strokeLinejoin="round"
    strokeWidth="1.3"
    viewBox="0 0 16 16"
  >
    <path d="M8.5 2.5H4A1.5 1.5 0 0 0 2.5 4v8A1.5 1.5 0 0 0 4 13.5h8a1.5 1.5 0 0 0 1.5-1.5V7.5" />
    <path d="m11.5 2 2.5 2.5L8.5 10H6V7.5z" />
  </svg>
);

const Gear = () => (
  <svg
    aria-hidden="true"
    className="size-4"
    fill="none"
    stroke="currentColor"
    strokeLinecap="round"
    strokeWidth="1.3"
    viewBox="0 0 16 16"
  >
    <circle cx="8" cy="8" r="2.2" />
    <path d="M8 1.5v2M8 12.5v2M1.5 8h2M12.5 8h2M3.4 3.4l1.4 1.4M11.2 11.2l1.4 1.4M3.4 12.6l1.4-1.4M11.2 4.8l1.4-1.4" />
  </svg>
);

const Search = ({
  query,
  onQuery,
}: {
  query: string;
  onQuery: (query: string) => void;
}) => {
  const search = useRef<HTMLInputElement>(null);

  useEffect(() => {
    const keys = (event: globalThis.KeyboardEvent) => {
      if (pressed(event, "f", true)) {
        event.preventDefault();
        search.current?.focus();
        search.current?.select();
      }
    };
    window.addEventListener("keydown", keys);
    return () => window.removeEventListener("keydown", keys);
  }, []);

  return (
    <label className="border-rim focus-within:border-edge flex h-8 items-center gap-2 rounded-lg border px-2.5">
      <svg
        aria-hidden="true"
        className="text-fog size-3.5 shrink-0"
        fill="none"
        stroke="currentColor"
        strokeLinecap="round"
        strokeWidth="1.4"
        viewBox="0 0 16 16"
      >
        <circle cx="7" cy="7" r="4.5" />
        <path d="m10.5 10.5 3 3" />
      </svg>
      <span className="sr-only">search chats</span>
      <input
        className="text-paper placeholder:text-fog h-full min-w-0 flex-1 bg-transparent text-[13px] outline-none"
        onChange={(event) => onQuery(event.target.value)}
        onKeyDown={(event) => {
          if (event.key === "Escape") {
            onQuery("");
          } else if (event.key === "ArrowDown") {
            event.preventDefault();
            document
              .querySelector<HTMLButtonElement>("button[data-chat]")
              ?.focus();
          }
        }}
        placeholder="search"
        ref={search}
        spellCheck={false}
        type="search"
        value={query}
      />
      <kbd className="text-fog text-[11px]">{shortcut("f", true)}</kbd>
    </label>
  );
};

const noted = (
  shelf: Tray["shelf"] | null,
  until: number | undefined,
  looked: number,
  now: number,
  current: boolean
): Note | null => {
  if (shelf === "snoozed" && until !== undefined) {
    return { text: `in ${left(until, now)}`, tone: "fog" };
  }
  if (woke(until, looked, now) && !current) {
    return { text: "woke", tone: "ember" };
  }
  return null;
};

const after = (list: Chat[], chat: Chat) => {
  const at = list.findIndex((entry) => keyed(entry) === keyed(chat));
  return list[at + 1] ?? list[at - 1] ?? null;
};

export const Sidebar = ({
  listing,
  current,
  prefs,
  actions,
  onPrefs,
}: Props) => {
  const shown = useMarks();
  const status = useMemo(() => marks(shown), [shown]);
  const seen = useSeen();
  const now = useNow();
  const [query, setQuery] = useState("");
  const [limit, setLimit] = useState(SETTLED);
  const chats = useMemo(
    () => (listing.state === "ready" ? listing.chats : []),
    [listing]
  );
  const shelved = useMemo(
    () =>
      shelves(
        chats,
        prefs,
        query,
        (chat) => status.get(keyOf(chat.agent, chat.id)) ?? null,
        now
      ),
    [chats, prefs, query, status, now]
  );
  const grouped = useMemo(
    () => sections(chats, prefs, query, current),
    [chats, prefs, query, current]
  );
  const searching = query.trim() !== "";

  const settleChat = useCallback(
    (chat: Chat) => {
      const following = after(shelved.inbox, chat);
      onPrefs({ ...prefs, ...settle(prefs, keyed(chat), chat.updated) });
      if (keyOf(chat.agent, chat.id) === current && following) {
        actions.open(following);
      }
    },
    [shelved, prefs, onPrefs, current, actions]
  );

  const tray = (chat: Chat, shelf: Tray["shelf"]): Tray => {
    const saved = keyed(chat);
    return {
      now,
      onSettle: () => settleChat(chat),
      onSnooze: (until) =>
        onPrefs({ ...prefs, ...snooze(prefs, saved, until) }),
      onUnsettle: () => onPrefs({ ...prefs, ...unsettle(prefs, saved) }),
      onWake: () => onPrefs({ ...prefs, ...wake(prefs, saved) }),
      shelf,
    };
  };

  useEffect(() => {
    const keys = (event: globalThis.KeyboardEvent) => {
      if (!pressed(event, "s", true) || !current) {
        return;
      }
      event.preventDefault();
      const chat = chats.find(
        (entry) => keyOf(entry.agent, entry.id) === current
      );
      if (chat) {
        settleChat(chat);
      }
    };
    window.addEventListener("keydown", keys);
    return () => window.removeEventListener("keydown", keys);
  }, [chats, current, settleChat]);

  const row = (chat: Chat, shelf: Tray["shelf"] | null, tile: boolean) => {
    const chatKey = keyOf(chat.agent, chat.id);
    const saved = keyed(chat);
    const mark = status.get(chatKey) ?? null;
    const looked = seenAt(seen, chatKey);
    const until = prefs.snoozed[saved];
    const isCurrent = chatKey === current;
    const note = noted(shelf, until, looked, now, isCurrent);
    const fresh = unread(chat, looked) && !isCurrent;
    const urgent = mark === "ask" || mark === "failed";
    return (
      <li key={saved}>
        <Row
          actions={actions}
          chat={chat}
          current={isCurrent}
          dim={!(fresh || urgent || note?.tone === "ember")}
          mark={mark}
          note={note}
          onArchive={() =>
            onPrefs({
              ...prefs,
              archived: toggle(prefs.archived, saved),
              pinned: prefs.pinned.filter((entry) => entry !== saved),
            })
          }
          onPin={() =>
            onPrefs({ ...prefs, pinned: toggle(prefs.pinned, saved) })
          }
          pinned={prefs.pinned.includes(saved)}
          tile={tile}
          tray={shelf ? tray(chat, shelf) : null}
        />
      </li>
    );
  };

  const open = (name: string) => prefs.shelves.includes(name);
  const flip = (name: string) =>
    onPrefs({ ...prefs, shelves: toggle(prefs.shelves, name) });

  const inbox = (
    <>
      {shelved.pinned.length > 0 ? (
        <section className="flex flex-col gap-px">
          <p className="text-fog flex h-7 items-center px-2 text-[12px]">
            pinned
          </p>
          <ul className="flex flex-col gap-px">
            {shelved.pinned.map((chat) => row(chat, "pinned", true))}
          </ul>
        </section>
      ) : null}
      <section className="flex flex-col gap-px">
        {shelved.inbox.length === 0 ? (
          <p className="text-fog px-2 py-1 text-[12px]">all caught up</p>
        ) : (
          <ul className="flex flex-col gap-px">
            {shelved.inbox.map((chat) => row(chat, "inbox", true))}
          </ul>
        )}
      </section>
      <div className="mt-auto flex flex-col gap-1 pt-2">
        <Stack
          count={shelved.snoozed.length}
          label="snoozed"
          onToggle={() => flip("snoozed")}
          open={open("snoozed")}
        >
          <ul className="flex flex-col gap-px">
            {shelved.snoozed.map((chat) => row(chat, "snoozed", true))}
          </ul>
        </Stack>
        <Stack
          count={shelved.settled.length}
          label="settled"
          onToggle={() => flip("settled")}
          open={open("settled")}
        >
          <ul className="flex flex-col gap-px">
            {shelved.settled
              .slice(0, limit)
              .map((chat) => row(chat, "settled", true))}
          </ul>
          {shelved.settled.length > limit ? (
            <button
              className="text-fog hover:text-body cursor-pointer self-start rounded-md px-2 py-1 text-[12px]"
              onClick={() => setLimit((count) => count + MORE)}
              type="button"
            >
              {`show ${Math.min(MORE, shelved.settled.length - limit)} more`}
            </button>
          ) : null}
        </Stack>
      </div>
    </>
  );

  const folders = (
    <>
      {grouped.pinned.length > 0 ? (
        <section className="flex flex-col gap-px">
          <p className="text-fog flex h-7 items-center px-2 text-[12px]">
            pinned
          </p>
          <ul className="flex flex-col gap-px">
            {grouped.pinned.map((chat) => row(chat, null, false))}
          </ul>
        </section>
      ) : null}
      {grouped.projects.map((project) => (
        <Group
          folded={prefs.folded.includes(project.folder) && !searching}
          key={project.folder}
          onCreate={() => actions.create(project.folder)}
          onFold={() =>
            onPrefs({ ...prefs, folded: toggle(prefs.folded, project.folder) })
          }
          onMore={() =>
            onPrefs({ ...prefs, opened: toggle(prefs.opened, project.folder) })
          }
          onPurge={() => actions.purge(project)}
          onReveal={() => actions.reveal(project.folder)}
          opened={prefs.opened.includes(project.folder)}
          project={project}
        >
          {project.chats.map((chat) => row(chat, null, false))}
        </Group>
      ))}
    </>
  );

  const results = (
    <ul className="flex flex-col gap-px">
      {[
        ...shelved.pinned,
        ...shelved.inbox,
        ...shelved.snoozed,
        ...shelved.settled,
      ]
        .toSorted((one, two) => two.updated - one.updated)
        .map((chat) => row(chat, null, true))}
    </ul>
  );

  const body = () => {
    if (searching) {
      return results;
    }
    return prefs.grouped ? folders : inbox;
  };

  return (
    <aside className="bg-ink border-rim flex min-h-0 flex-col border-r">
      <header
        className={`flex h-13 shrink-0 items-center gap-2 pr-2 ${mac ? "pl-[88px]" : "pl-4"}`}
        data-tauri-drag-region="deep"
      >
        <Mark className="text-ember h-2.5 w-4" />
        <span className="text-paper flex-1 font-semibold">sombre</span>
        <button
          aria-label="new chat"
          className={`${icon} size-7`}
          onClick={() => actions.create()}
          title={`new chat ${shortcut("n")}`}
          type="button"
        >
          <Pencil />
        </button>
      </header>
      <div className="flex flex-col gap-1 px-2 pb-2">
        <Search onQuery={setQuery} query={query} />
        <Places
          actions={actions}
          chats={chats}
          onPrefs={onPrefs}
          prefs={prefs}
        />
      </div>
      <nav
        aria-label="chats"
        className="flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto px-2 pb-3"
      >
        {listing.state === "loading" ? (
          <Notice late title="reading your chats" />
        ) : null}
        {listing.state === "failed" ? (
          <Notice
            onRetry={() => actions.retry()}
            title="couldn't read your chats"
          >
            <p>{listing.message}</p>
          </Notice>
        ) : null}
        {listing.state === "ready" && shelved.total === 0 ? (
          <Notice title={query ? "nothing matches" : "no chats yet"}>
            <p>
              {query
                ? "try fewer words, or press esc"
                : "start one with the new chat button"}
            </p>
          </Notice>
        ) : null}
        {listing.state === "ready" && shelved.total > 0 ? body() : null}
      </nav>
      <Update />
      <footer className="border-rim flex shrink-0 items-center gap-1 border-t p-2">
        <button
          className="text-fog hover:bg-raise hover:text-paper active:bg-well flex h-8 flex-1 cursor-pointer items-center gap-2 rounded-lg px-2 text-left text-[13px]"
          onClick={() => actions.settings()}
          type="button"
        >
          <Gear />
          <span className="flex-1">settings</span>
          <kbd className="text-[11px]">{shortcut(",")}</kbd>
        </button>
      </footer>
    </aside>
  );
};
