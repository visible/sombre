import { useEffect, useRef, useState } from "react";

import type { Agent } from "../lib/api";
import { agents, names } from "../lib/api";
import type { Target } from "../lib/feed";
import type { Live } from "../lib/live";
import { mac, shortcut } from "../lib/platform";
import { useWidth } from "../lib/width";
import { Git } from "./git";
import { Logo } from "./logo";
import { Divider, Option, Popover } from "./popover";

export interface Chores {
  rename: (name: string) => void;
  compact: () => void;
  reveal: () => void;
  copy: () => void;
  archive: () => void;
  create: () => void;
  carry: (to: Agent) => void;
}

interface Props {
  target: Target;
  live: Live;
  hidden: boolean;
  reviewing: boolean;
  version: number;
  onSidebar: () => void;
  onReview: (() => void) | null;
  onOpen: (() => void) | null;
  onTerminal: (() => void) | null;
  onPhone: (() => void) | null;
  phoned: boolean;
  chores: Chores;
}

const Handset = ({
  onPress,
  phoned,
}: {
  onPress: () => void;
  phoned: boolean;
}) => (
  <button
    aria-label={
      phoned ? "pair a phone, a phone follows this chat" : "pair a phone"
    }
    className="text-fog hover:bg-raise hover:text-paper active:bg-well focus-visible:text-paper relative flex size-8 cursor-pointer items-center justify-center rounded-lg disabled:cursor-default disabled:opacity-40"
    onClick={onPress}
    title={phoned ? "a phone follows this chat" : "pair a phone"}
    type="button"
  >
    <svg
      aria-hidden="true"
      className="size-4"
      fill="none"
      stroke="currentColor"
      strokeLinecap="round"
      strokeLinejoin="round"
      strokeWidth="1.4"
      viewBox="0 0 16 16"
    >
      <rect height="13" rx="2" width="8" x="4" y="1.5" />
      <path d="M7 12h2" />
    </svg>
    {phoned ? (
      <span
        aria-hidden="true"
        className="bg-ember absolute top-1.5 right-1.5 size-1.5 rounded-full"
      />
    ) : null}
  </button>
);

export const Panel = () => (
  <svg
    aria-hidden="true"
    className="size-4"
    fill="none"
    stroke="currentColor"
    strokeWidth="1.3"
    viewBox="0 0 16 16"
  >
    <rect height="11" rx="2" width="13" x="1.5" y="2.5" />
    <path d="M6 2.5v11" />
  </svg>
);

export const Header = ({
  target,
  live,
  hidden,
  reviewing,
  version,
  onSidebar,
  onReview,
  onOpen,
  onTerminal,
  onPhone,
  phoned,
  chores,
}: Props) => {
  const [editing, setEditing] = useState(false);
  const [name, setName] = useState(target.title);
  const field = useRef<HTMLInputElement>(null);
  const [width, bar] = useWidth<HTMLElement>();
  const roomy = width >= 520;
  const started = target.id !== null || live.session !== null;

  useEffect(() => {
    if (editing) {
      field.current?.select();
    }
  }, [editing]);

  const commit = () => {
    setEditing(false);
    if (name.trim() !== "" && name.trim() !== target.title) {
      chores.rename(name.trim());
    }
  };

  return (
    <header
      className={`border-rim flex h-13 shrink-0 items-center gap-2 border-b pr-4 ${hidden && mac ? "pl-[88px]" : "pl-3"} ${mac ? "" : "pr-36"}`}
      data-tauri-drag-region="deep"
      ref={bar}
    >
      {hidden ? (
        <button
          aria-label="show the sidebar"
          className="text-fog hover:bg-raise hover:text-paper active:bg-well flex size-7 shrink-0 cursor-pointer items-center justify-center rounded-lg"
          onClick={onSidebar}
          title={`show the sidebar ${shortcut("b")}`}
          type="button"
        >
          <Panel />
        </button>
      ) : null}
      <div className="flex min-w-0 flex-1 items-center gap-1.5 pl-1">
        <span className="text-fog flex shrink-0 items-center gap-2 font-sans text-[13px]">
          <span className="opacity-70">
            <Logo agent={target.agent} />
          </span>
          {width >= 440 ? (
            <span className="max-w-48 truncate">
              {target.folder?.project ?? names[target.agent]}
            </span>
          ) : null}
          <span aria-hidden="true" className="text-edge">
            /
          </span>
        </span>
        {editing ? (
          <input
            aria-label="rename this chat"
            className="border-edge bg-well text-paper h-8 min-w-0 flex-1 rounded-lg border px-2 font-sans text-[13.5px] font-medium outline-none"
            onBlur={commit}
            onChange={(event) => setName(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter") {
                commit();
              } else if (event.key === "Escape") {
                setEditing(false);
              }
            }}
            ref={field}
            spellCheck={false}
            value={name}
          />
        ) : (
          <Popover
            align="start"
            className="text-paper hover:bg-raise aria-expanded:bg-raise active:bg-well flex h-8 min-w-0 cursor-pointer items-center gap-1.5 rounded-lg px-2 font-sans text-[13.5px] font-medium disabled:cursor-default disabled:hover:bg-transparent"
            disabled={!started}
            label="chat actions"
            side="bottom"
            trigger={<span className="truncate">{target.title}</span>}
            width="w-72"
          >
            {(close) => (
              <>
                <Option
                  icon="✎"
                  label="rename"
                  onPick={() => {
                    close();
                    setName(target.title);
                    setEditing(true);
                  }}
                />
                <Option
                  disabled={live.busy}
                  hint="summarize to free up context"
                  icon="≡"
                  label="compact"
                  onPick={() => {
                    close();
                    chores.compact();
                  }}
                />
                <Option
                  icon="+"
                  label="new chat in this folder"
                  onPick={() => {
                    close();
                    chores.create();
                  }}
                  shortcut={shortcut("n")}
                />
                <Divider />
                {agents
                  .filter((agent) => agent !== target.agent)
                  .map((agent) => (
                    <Option
                      disabled={live.busy}
                      hint="the whole chat comes along"
                      icon={<Logo agent={agent} />}
                      key={agent}
                      label={`move to ${names[agent]}`}
                      onPick={() => {
                        close();
                        chores.carry(agent);
                      }}
                    />
                  ))}
                <Divider />
                <Option
                  icon="↗"
                  label="show in finder"
                  onPick={() => {
                    close();
                    chores.reveal();
                  }}
                />
                <Option
                  icon="⧉"
                  label="copy resume command"
                  onPick={() => {
                    close();
                    chores.copy();
                  }}
                />
                <Divider />
                <Option
                  icon="↓"
                  label="archive"
                  onPick={() => {
                    close();
                    chores.archive();
                  }}
                />
              </>
            )}
          </Popover>
        )}
      </div>
      <div className="text-fog flex shrink-0 items-center gap-1 text-[12px]">
        {target.folder && started ? (
          <>
            <Git
              compact={!roomy}
              folder={target.folder.path}
              version={version}
            />
            <span aria-hidden="true" className="bg-rim mx-1.5 h-4 w-px" />
          </>
        ) : null}
        {onOpen ? (
          <button
            aria-label="open in your editor"
            className="text-fog hover:bg-raise hover:text-paper active:bg-well focus-visible:text-paper flex size-8 cursor-pointer items-center justify-center rounded-lg disabled:cursor-default disabled:opacity-40"
            onClick={onOpen}
            title="open this folder in your editor"
            type="button"
          >
            <svg
              aria-hidden="true"
              className="size-4"
              fill="none"
              stroke="currentColor"
              strokeLinecap="round"
              strokeLinejoin="round"
              strokeWidth="1.4"
              viewBox="0 0 16 16"
            >
              <path d="M9.5 2.5h4v4M13.5 2.5 8 8M11.5 9.5v3a1 1 0 0 1-1 1h-7a1 1 0 0 1-1-1v-7a1 1 0 0 1 1-1h3" />
            </svg>
          </button>
        ) : null}
        {onPhone ? <Handset onPress={onPhone} phoned={phoned} /> : null}
        {onTerminal ? (
          <button
            aria-label="terminal"
            className="text-fog hover:bg-raise hover:text-paper active:bg-well focus-visible:text-paper flex size-8 cursor-pointer items-center justify-center rounded-lg disabled:cursor-default disabled:opacity-40"
            onClick={onTerminal}
            title="terminal ⌃`"
            type="button"
          >
            <svg
              aria-hidden="true"
              className="size-4"
              fill="none"
              stroke="currentColor"
              strokeLinecap="round"
              strokeLinejoin="round"
              strokeWidth="1.4"
              viewBox="0 0 16 16"
            >
              <path d="m3 4.5 3.5 3.5L3 11.5M8.5 12H13" />
            </svg>
          </button>
        ) : null}
        {onReview ? (
          <button
            aria-label="changes"
            aria-pressed={reviewing}
            className="text-fog hover:bg-raise hover:text-paper active:bg-well focus-visible:text-paper aria-pressed:bg-raise aria-pressed:text-paper flex size-8 cursor-pointer items-center justify-center rounded-lg disabled:cursor-default disabled:opacity-40"
            onClick={onReview}
            title={`changes ${shortcut("d", true)}`}
            type="button"
          >
            <svg
              aria-hidden="true"
              className="size-4"
              fill="none"
              stroke="currentColor"
              strokeLinecap="round"
              strokeWidth="1.4"
              viewBox="0 0 16 16"
            >
              <path d="M5 2.5v6M2 5.5h6M9 12.5h5" />
            </svg>
          </button>
        ) : null}
      </div>
    </header>
  );
};
