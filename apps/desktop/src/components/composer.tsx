import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open as browse } from "@tauri-apps/plugin-dialog";
import type { ClipboardEvent, KeyboardEvent, ReactNode } from "react";
import { useCallback, useEffect, useRef, useState } from "react";

import type { Agent, Folder, Mode, Model, Picture } from "../lib/api";
import { failure, keep as save, models, modes, picture } from "../lib/api";
import type { Name } from "../lib/commands";
import { parse, suggest } from "../lib/commands";
import { encode, pictures, short } from "../lib/format";
import type { Live } from "../lib/live";
import { seconds, waiting } from "../lib/live";
import { draft, halt, hurry, keep, note, unqueue } from "../lib/store";
import { useMention } from "./mention";
import { Models } from "./models";
import { Option, Popover, chip } from "./popover";
import { Pursuit } from "./pursuit";
import { Ring } from "./ring";
import { Slash } from "./slash";
import { Strip } from "./strip";

const catalog = new Map<Agent, Model[]>();
const ENDINGS = ["png", "jpg", "jpeg", "gif", "webp"];
const PHOTOS = 6;

interface Props {
  chatKey: string;
  agent: Agent;
  folder: Folder | null;
  live: Live;
  model: string | null;
  mode: Mode;
  effort: string | null;
  levels: string[];
  ready: boolean;
  steer: boolean;
  placeholder?: string;
  lead?: ReactNode;
  onSend: (text: string, images: Picture[]) => void;
  onModel: (model: string) => void;
  onMode: (mode: Mode) => void;
  onEffort: (level: string) => void;
  onCycle: () => void;
  onCommand: (name: Name, rest: string) => void;
}

const useClock = (since: number | null) => {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    if (since === null) {
      return;
    }
    const timer = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(timer);
  }, [since]);
  return seconds(since, Math.max(now, since ?? 0));
};

const useModels = (agent: Agent) => {
  const [listed, setListed] = useState<Model[] | null>(
    () => catalog.get(agent) ?? null
  );
  useEffect(() => {
    if (catalog.has(agent)) {
      return;
    }
    const fetch = async () => {
      try {
        const found = await models(agent);
        catalog.set(agent, found);
        setListed(found);
      } catch {
        setListed([]);
      }
    };
    fetch();
  }, [agent]);
  return listed;
};

const icons: Record<Mode, string> = { ask: "◇", auto: "◆", yolo: "▲" };

interface ActivityProps {
  chatKey: string;
  live: Live;
  asking: boolean;
  elapsed: number;
  steer: boolean;
}

const Activity = ({ chatKey, live, asking, elapsed, steer }: ActivityProps) => {
  const goal = live.goal?.status === "achieved" ? null : live.goal;
  if (!live.busy && live.queue.length === 0 && !goal) {
    return null;
  }
  return (
    <div className="text-fog flex flex-col gap-1 px-1 pb-2 text-[12.5px]">
      {goal ? <Pursuit chatKey={chatKey} goal={goal} /> : null}
      {live.busy ? (
        <p className="flex items-center gap-[1ch]">
          <span aria-hidden="true" className="text-ember">
            ●
          </span>
          <span>{asking ? "waiting for you" : `working ${elapsed}s`}</span>
          <span className="ml-auto">esc to stop</span>
        </p>
      ) : null}
      {live.queue.map((queued, index) => (
        <p
          className="text-body overflow-hidden text-ellipsis whitespace-nowrap"
          key={`${queued.text}-${queued.images.length}-${live.queue.length - index}`}
        >
          <span aria-hidden="true" className="text-fog">
            ↳{" "}
          </span>
          {queued.text.split("\n")[0]}
          {queued.images.length > 0 ? ` + ${queued.images.length} images` : ""}
        </p>
      ))}
      {live.queue.length > 0 ? (
        <p>{`${steer ? "enter" : "tab"} sends the queue now. ↑ edits it`}</p>
      ) : null}
    </div>
  );
};

const hinted = ({
  ready,
  busy,
  steer,
  placeholder,
}: {
  ready: boolean;
  busy: boolean;
  steer: boolean;
  placeholder?: string;
}) => {
  if (!ready) {
    return placeholder ?? "pick a folder first";
  }
  if (busy) {
    return steer
      ? "enter steers the turn, tab queues"
      : "enter queues for after this turn, tab steers it";
  }
  return placeholder ?? "message the agent, @ for files, / for commands";
};

export const Composer = ({
  chatKey,
  agent,
  folder,
  live,
  model,
  mode,
  effort,
  levels,
  ready,
  steer,
  placeholder,
  lead,
  onSend,
  onModel,
  onMode,
  onEffort,
  onCycle,
  onCommand,
}: Props) => {
  const [text, setText] = useState(() => draft(chatKey));
  const [images, setImages] = useState<Picture[]>([]);
  const [lit, setLit] = useState(0);
  const [caret, setCaret] = useState(0);
  const [dropping, setDropping] = useState(false);
  const field = useRef<HTMLTextAreaElement>(null);
  const elapsed = useClock(live.since);
  const listed = useModels(agent);
  const asking = waiting(live) !== undefined;
  const offered = ready ? suggest(text) : [];
  const shown = offered.length > 0;

  useEffect(() => {
    if (!asking) {
      field.current?.focus();
    }
  }, [asking]);

  const back = useCallback((open: boolean) => {
    if (!open) {
      field.current?.focus();
    }
  }, []);

  const change = (value: string) => {
    setText(value);
    setLit(0);
    keep(chatKey, value);
  };

  const mention = useMention({
    caret,
    fallback: live.folder,
    folder,
    onText: (next, at) => {
      change(next);
      setCaret(at);
      requestAnimationFrame(() => field.current?.setSelectionRange(at, at));
    },
    text,
  });

  const attach = useCallback(
    async (paths: string[]) => {
      const kept = await Promise.all(
        pictures(paths)
          .slice(0, PHOTOS)
          .map(async (path) => {
            try {
              return await picture(path);
            } catch (error) {
              note(chatKey, failure(error));
              return null;
            }
          })
      );
      const found = kept.filter((entry): entry is Picture => entry !== null);
      setImages((current) => [...current, ...found].slice(0, PHOTOS));
    },
    [chatKey]
  );

  useEffect(() => {
    let alive = true;
    let stop: (() => void) | null = null;
    const listen = async () => {
      const unlisten = await getCurrentWebview().onDragDropEvent((event) => {
        if (event.payload.type === "over" || event.payload.type === "enter") {
          setDropping(true);
        } else if (event.payload.type === "drop") {
          setDropping(false);
          attach(event.payload.paths);
        } else {
          setDropping(false);
        }
      });
      if (alive) {
        stop = unlisten;
      } else {
        unlisten();
      }
    };
    listen();
    return () => {
      alive = false;
      stop?.();
    };
  }, [attach]);

  const pick = async () => {
    const chosen = await browse({
      filters: [{ extensions: ENDINGS, name: "images" }],
      multiple: true,
      title: "add images",
    });
    if (Array.isArray(chosen)) {
      await attach(chosen);
    } else if (typeof chosen === "string") {
      await attach([chosen]);
    }
  };

  const paste = async (event: ClipboardEvent<HTMLTextAreaElement>) => {
    const files = [...event.clipboardData.files].filter((file) =>
      file.type.startsWith("image/")
    );
    if (files.length === 0) {
      return;
    }
    event.preventDefault();
    const kept = await Promise.all(
      files.slice(0, PHOTOS - images.length).map(async (file) => {
        try {
          return await save(encode(new Uint8Array(await file.arrayBuffer())));
        } catch (error) {
          note(chatKey, failure(error));
          return null;
        }
      })
    );
    const found = kept.filter((entry): entry is Picture => entry !== null);
    setImages((current) => [...current, ...found].slice(0, PHOTOS));
  };

  const run = (name: Name, rest: string) => {
    change("");
    onCommand(name, rest);
  };

  const submit = () => {
    if ((text.trim() === "" && images.length === 0) || !ready) {
      return;
    }
    const parsed = images.length === 0 ? parse(text) : null;
    if (parsed?.known) {
      run(parsed.name, parsed.rest);
      return;
    }
    onSend(text.trim(), images);
    setImages([]);
    change("");
  };

  const steerNow = async () => {
    const sent = await hurry(chatKey, agent, text);
    if (sent && text.trim() !== "") {
      change("");
    }
  };

  const choose = (index: number) => {
    const command = offered[index];
    if (!command) {
      return;
    }
    if (command.takes) {
      change(`/${command.name} `);
    } else {
      run(command.name, "");
    }
  };

  const slashKeys = (event: KeyboardEvent<HTMLTextAreaElement>) => {
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      const step = event.key === "ArrowDown" ? 1 : -1;
      setLit((current) => (current + step + offered.length) % offered.length);
      return true;
    }
    if (event.key === "Tab" || event.key === "Enter") {
      event.preventDefault();
      choose(lit);
      return true;
    }
    return false;
  };

  const cycled = (event: KeyboardEvent<HTMLTextAreaElement>) => {
    if (event.key !== "Tab" || !event.shiftKey) {
      return false;
    }
    event.preventDefault();
    onCycle();
    return true;
  };

  const menus = (event: KeyboardEvent<HTMLTextAreaElement>) =>
    (shown && slashKeys(event)) || mention.keys(event) || cycled(event);

  const keys = (event: KeyboardEvent<HTMLTextAreaElement>) => {
    if (event.nativeEvent.isComposing || menus(event)) {
      return;
    }
    const enter = event.key === "Enter" && !event.shiftKey;
    const tab = event.key === "Tab" && !event.shiftKey;
    const typed = text.trim() !== "";
    if (live.busy && (enter || tab) && typed) {
      event.preventDefault();
      if (enter === steer) {
        steerNow();
      } else {
        submit();
      }
    } else if (enter || (tab && live.busy)) {
      event.preventDefault();
      if (enter) {
        submit();
      } else {
        steerNow();
      }
    } else if (event.key === "Escape" && live.busy) {
      event.preventDefault();
      halt(chatKey);
    } else if (event.key === "ArrowUp" && text === "") {
      const last = unqueue(chatKey);
      if (last !== null) {
        event.preventDefault();
        change(last.text);
        setImages(last.images);
      }
    }
  };

  const hint = hinted({ busy: live.busy, placeholder, ready, steer });
  const empty = text.trim() === "" && images.length === 0;

  return (
    <div className="mx-auto w-full max-w-[780px] px-6 pb-5">
      <Activity
        asking={asking}
        chatKey={chatKey}
        elapsed={elapsed}
        live={live}
        steer={steer}
      />
      <div
        className={`bg-raise shadow-lift relative rounded-2xl border transition-colors motion-reduce:transition-none ${dropping ? "border-ember/70" : "border-rim focus-within:border-edge"}`}
      >
        {shown ? (
          <Slash
            lit={lit}
            offered={offered}
            onChoose={choose}
            onLight={setLit}
          />
        ) : (
          mention.menu
        )}
        {images.length > 0 ? (
          <Strip
            images={images}
            onRemove={(image) =>
              setImages((current) => current.filter((entry) => entry !== image))
            }
          />
        ) : null}
        <label className="sr-only" htmlFor={`${chatKey}-message`}>
          message
        </label>
        <textarea
          className="text-paper placeholder:text-fog block [field-sizing:content] max-h-60 min-h-[56px] w-full resize-none bg-transparent px-4 pt-4 pb-1 font-sans text-[15px] leading-[1.55] outline-none focus-visible:outline-none disabled:opacity-50"
          disabled={!ready}
          id={`${chatKey}-message`}
          onChange={(event) => {
            change(event.target.value);
            setCaret(event.target.selectionStart);
          }}
          onKeyDown={keys}
          onPaste={paste}
          onSelect={(event) => setCaret(event.currentTarget.selectionStart)}
          placeholder={hint}
          ref={field}
          rows={2}
          spellCheck={false}
          value={text}
        />
        <div className="flex items-center gap-1 px-2.5 pt-1 pb-2.5">
          <button
            aria-label="add images"
            className={`${chip} px-1.5`}
            disabled={!ready || images.length >= PHOTOS}
            onClick={pick}
            title="add images (or paste, or drop them here)"
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
              <path d="M8 3v10M3 8h10" />
            </svg>
          </button>
          {lead}
          <Popover
            disabled={live.busy}
            label="model and effort"
            onOpen={back}
            trigger={
              <>
                <span className="max-w-40 truncate">
                  {model ? short(model) : "default model"}
                </span>
                {effort ? <span className="text-fog">{effort}</span> : null}
              </>
            }
            width={levels.length > 0 ? "w-[26rem]" : "w-72"}
          >
            {(close) => (
              <Models
                close={close}
                effort={effort}
                levels={levels}
                loading={listed === null}
                model={model}
                models={listed ?? []}
                onEffort={onEffort}
                onModel={onModel}
              />
            )}
          </Popover>
          <Popover
            label="permissions"
            onOpen={back}
            trigger={
              <>
                <span
                  aria-hidden="true"
                  className={mode === "yolo" ? "text-ember" : ""}
                >
                  {icons[mode]}
                </span>
                <span>{mode}</span>
              </>
            }
          >
            {(close) =>
              modes.map((entry) => (
                <Option
                  hint={entry.about}
                  icon={icons[entry.mode]}
                  key={entry.mode}
                  label={entry.mode}
                  onPick={() => {
                    onMode(entry.mode);
                    close();
                  }}
                  selected={entry.mode === mode}
                />
              ))
            }
          </Popover>
          <span className="ml-auto flex items-center gap-2.5 pr-1">
            {live.left === null ? null : <Ring left={live.left} />}
            {live.busy && empty ? (
              <button
                aria-label="stop"
                className="border-rim text-paper hover:border-fog active:bg-well flex size-8 cursor-pointer items-center justify-center rounded-full border"
                onClick={() => halt(chatKey)}
                type="button"
              >
                <svg
                  aria-hidden="true"
                  className="size-3"
                  fill="currentColor"
                  viewBox="0 0 12 12"
                >
                  <rect height="10" rx="1.5" width="10" x="1" y="1" />
                </svg>
              </button>
            ) : (
              <button
                aria-label={live.busy ? "queue" : "send"}
                className="bg-ember text-ink flex size-8 cursor-pointer items-center justify-center rounded-full hover:brightness-110 active:brightness-95 disabled:cursor-default disabled:opacity-30 disabled:hover:brightness-100"
                disabled={empty || !ready}
                onClick={submit}
                type="button"
              >
                <svg
                  aria-hidden="true"
                  className="size-3.5"
                  fill="none"
                  stroke="currentColor"
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  strokeWidth="1.8"
                  viewBox="0 0 16 16"
                >
                  <path d="M8 13V3M3.5 7.5 8 3l4.5 4.5" />
                </svg>
              </button>
            )}
          </span>
        </div>
      </div>
    </div>
  );
};
