import type { Chat } from "../lib/api";
import { names } from "../lib/api";
import { mac, shortcut } from "../lib/platform";
import { prefetch } from "../lib/store";
import { Panel } from "./header";
import { Logo } from "./logo";
import { Mark } from "./mark";

interface Props {
  recent: Chat[];
  hidden: boolean;
  onSidebar: () => void;
  onPick: (chat: Chat) => void;
  onCreate: () => void;
  onPalette: () => void;
  onSettings: () => void;
}

const quiet =
  "hover:text-paper active:text-body cursor-pointer rounded-sm disabled:cursor-default disabled:opacity-50";

export const Start = ({
  recent,
  hidden,
  onSidebar,
  onPick,
  onCreate,
  onPalette,
  onSettings,
}: Props) => (
  <main className="flex min-h-0 flex-col">
    <div
      className={`flex h-13 shrink-0 items-center ${hidden && mac ? "pl-[88px]" : "pl-3"}`}
      data-tauri-drag-region="deep"
    >
      {hidden ? (
        <button
          aria-label="show the sidebar"
          className="text-fog hover:bg-raise hover:text-paper active:bg-well flex size-7 cursor-pointer items-center justify-center rounded-lg"
          onClick={onSidebar}
          type="button"
        >
          <Panel />
        </button>
      ) : null}
    </div>
    <div className="flex min-h-0 flex-1 flex-col items-center justify-center overflow-y-auto px-6 pb-13">
      <div className="flex w-full max-w-[640px] flex-col items-stretch gap-6">
        <div className="flex flex-col items-center gap-1.5 text-center">
          <Mark className="text-ember mb-3 h-6 w-10" />
          <h1 className="text-paper text-[22px] leading-snug">
            what should we work on?
          </h1>
          <p className="text-fog">
            {recent.length > 0
              ? "start a new chat, or pick one up where you left off"
              : "claude code, codex, and sombre chats all show up here"}
          </p>
        </div>
        <button
          className="border-rim bg-raise text-fog hover:border-edge hover:text-body active:bg-well flex h-14 w-full cursor-pointer items-center justify-between rounded-2xl border px-5 text-left"
          onClick={onCreate}
          type="button"
        >
          <span>ask anything in a new chat</span>
          <kbd className="text-[11px]">{shortcut("n")}</kbd>
        </button>
        {recent.length > 0 ? (
          <section className="flex flex-col gap-1">
            <h2 className="text-fog px-3 text-[12px]">recent</h2>
            <ul className="flex flex-col">
              {recent.map((chat) => (
                <li key={`${chat.agent}:${chat.id}`}>
                  <button
                    className="hover:bg-raise focus-visible:bg-raise active:bg-well flex w-full cursor-pointer items-center gap-3 rounded-lg px-3 py-2 text-left"
                    onClick={() => onPick(chat)}
                    onFocus={() => {
                      prefetch(chat.agent, chat.id, chat.updated).catch(
                        () => null
                      );
                    }}
                    onPointerEnter={() => {
                      prefetch(chat.agent, chat.id, chat.updated).catch(
                        () => null
                      );
                    }}
                    type="button"
                  >
                    <Logo agent={chat.agent} />
                    <span className="sr-only">{names[chat.agent]}</span>
                    <span className="text-body min-w-0 flex-1 truncate text-[13px]">
                      {chat.title}
                    </span>
                    <span className="text-fog shrink-0 text-[12px]">
                      {`${chat.project} · ${chat.ago}`}
                    </span>
                  </button>
                </li>
              ))}
            </ul>
          </section>
        ) : null}
        <div className="text-fog flex justify-center gap-5 text-[12px]">
          <button className={quiet} onClick={onPalette} type="button">
            palette <kbd className="text-[11px]">{shortcut("k")}</kbd>
          </button>
          <button className={quiet} onClick={onSettings} type="button">
            settings <kbd className="text-[11px]">{shortcut(",")}</kbd>
          </button>
        </div>
      </div>
    </div>
  </main>
);
