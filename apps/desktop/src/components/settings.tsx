import { useEffect, useState } from "react";

import type {
  Agent,
  Change,
  Chat,
  Installed,
  Mode,
  Model,
  Settings,
} from "../lib/api";
import { agents, failure, models, modes, names } from "../lib/api";
import { mac, shortcut } from "../lib/platform";
import type { Prefs } from "../lib/sidebar";
import { keyed, toggle } from "../lib/sidebar";
import { Card, Row, anchor } from "./card";
import { Logo } from "./logo";
import { Missing } from "./missing";
import { Notice } from "./notice";
import { Pick } from "./pick";
import { Option, Popover, Search, chip } from "./popover";
import type { Edit } from "./providers";
import { Providers } from "./providers";
import { Servers } from "./servers";

const EFFORTS = ["default", "minimal", "low", "medium", "high", "xhigh", "max"];

type Part =
  | "general"
  | "agents"
  | "models"
  | "providers"
  | "mcp servers"
  | "skills"
  | "archive"
  | "shortcuts"
  | "about";

const PARTS: Part[] = [
  "general",
  "agents",
  "models",
  "providers",
  "mcp servers",
  "skills",
  "archive",
  "shortcuts",
  "about",
];

interface Props {
  settings: Settings | null;
  problem: string | null;
  chats: Chat[];
  prefs: Prefs;
  width: number;
  section: string | null;
  installed: Installed[] | null;
  onChange: (change: Change) => void;
  onEdit: Edit;
  onPrefs: (prefs: Prefs) => void;
  onReveal: (path: string) => void;
  onClose: () => void;
}

const Switch = ({
  checked,
  label,
  onChange,
}: {
  checked: boolean;
  label: string;
  onChange: (checked: boolean) => void;
}) => (
  <label className="bg-rim has-checked:bg-ember keys:has-[:focus-visible]:outline-paper keys:has-[:focus-visible]:outline keys:has-[:focus-visible]:outline-1 keys:has-[:focus-visible]:outline-offset-2 relative inline-flex h-5 w-9 shrink-0 cursor-pointer items-center rounded-full transition-colors">
    <input
      aria-label={label}
      checked={checked}
      className="peer sr-only"
      onChange={(event) => onChange(event.target.checked)}
      type="checkbox"
    />
    <span className="bg-paper absolute left-0.5 size-4 rounded-full transition-transform peer-checked:translate-x-4 motion-reduce:transition-none" />
  </label>
);

const spot = (element: HTMLElement): Part => {
  const line = element.getBoundingClientRect().top + 90;
  const bottom =
    element.scrollTop + element.clientHeight >= element.scrollHeight - 4;
  let found: Part = "general";
  for (const part of PARTS) {
    const node = element.querySelector<HTMLElement>(`#${anchor(part)}`);
    if (node && (bottom || node.getBoundingClientRect().top <= line)) {
      found = part;
    }
  }
  return found;
};

const Sombre = ({
  value,
  onPick,
}: {
  value: string;
  onPick: (model: string) => void;
}) => {
  const [listed, setListed] = useState<Model[] | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const load = async () => {
    if (listed !== null) {
      return;
    }
    try {
      setListed(await models("sombre"));
    } catch (error) {
      setListed([]);
      setProblem(failure(error));
    }
  };
  const shown = (listed ?? []).filter((model) =>
    model.id.toLowerCase().includes(query.toLowerCase().trim())
  );
  return (
    <Popover
      align="end"
      className={`${chip} border-rim max-w-72 border`}
      label="sombre's model"
      onOpen={(open) => {
        if (open) {
          load();
        }
      }}
      side="bottom"
      trigger={<span className="truncate">{value}</span>}
      width="w-96"
    >
      {(close) => (
        <>
          <Search
            onChange={setQuery}
            placeholder="search models"
            value={query}
          />
          <div className="flex min-h-0 flex-col overflow-y-auto overscroll-contain">
            {listed === null ? (
              <p className="text-fog px-2 py-1.5">loading models</p>
            ) : null}
            {problem ? <p className="text-fog px-2 py-1.5">{problem}</p> : null}
            {shown.map((model) => (
              <Option
                hint={model.about}
                key={model.id}
                label={model.id}
                onPick={() => {
                  onPick(model.id);
                  close();
                }}
                selected={model.id === value}
              />
            ))}
          </div>
        </>
      )}
    </Popover>
  );
};

const KEYS: [string, string][] = [
  [shortcut("k"), "command palette"],
  [shortcut("n"), "new chat"],
  [shortcut("f", true), "search chats"],
  [shortcut("s", true), "settle this chat and open the next"],
  [shortcut("f"), "find in this chat"],
  [shortcut("d", true), "show or hide changes"],
  [shortcut("b"), "show or hide the sidebar"],
  [shortcut(","), "settings"],
  [shortcut("/"), "these shortcuts"],
  [`${shortcut("[")} ${shortcut("]")}`, "back and forward between chats"],
  ["⌃`", "terminal"],
  [
    `${shortcut("+")} ${shortcut("-")} ${shortcut("0")}`,
    "bigger, smaller, or normal text",
  ],
  ["@", "mention a file"],
  ["enter", "send"],
  ["shift enter", "new line"],
  ["tab", "steer or queue while the agent works"],
  ["⇧tab", "switch permissions: ask, auto, yolo"],
  ["esc", "stop the turn"],
  ["↑", "edit the last queued message"],
  ["/", "commands"],
  ["F2", "rename the selected chat"],
];

const General = ({
  settings,
  onChange,
}: {
  settings: Settings;
  onChange: (change: Change) => void;
}) => (
  <Card id="general" title="general">
    <Row about="new chats start with this agent" title="default agent">
      <Pick<Agent>
        label="default agent"
        onPick={(agent) => onChange({ agent })}
        options={agents.map((agent) => ({
          label: (
            <span className="flex items-center gap-2">
              <Logo agent={agent} />
              {names[agent]}
            </span>
          ),
          value: agent,
        }))}
        value={settings.agent}
      />
    </Row>
    <Row about="how much the agent may do without asking" title="permissions">
      <Pick<Mode>
        label="permissions"
        onPick={(mode) => onChange({ mode })}
        options={modes.map((entry) => ({
          hint: entry.about,
          label: entry.mode,
          value: entry.mode,
        }))}
        value={settings.mode}
      />
    </Row>
    <Row about="sombre uses the closest level each agent takes" title="effort">
      <Pick<string>
        label="effort"
        onPick={(effort) => onChange({ effort })}
        options={EFFORTS.map((level) => ({ label: level, value: level }))}
        value={settings.effort ?? "default"}
      />
    </Row>
    <Row
      about="for the open buttons in chats and changes"
      title="open files with"
    >
      <Pick<string>
        label="open files with"
        onPick={(editor) => onChange({ editor })}
        options={[
          { label: "default app", value: "default" },
          ...settings.editors.map((entry) => ({
            label: entry.name,
            value: entry.id,
          })),
        ]}
        value={settings.editor ?? "default"}
      />
    </Row>
    <Row
      about="when a chat finishes or needs you while sombre is in the background"
      title="notifications"
    >
      <Switch
        checked={settings.notify}
        label="notifications"
        onChange={(notify) => onChange({ notify })}
      />
    </Row>
    <Row
      about="stops your computer sleeping while sombre is open"
      title="keep this computer awake"
    >
      <Switch
        checked={settings.awake}
        label="keep this computer awake"
        onChange={(awake) => onChange({ awake })}
      />
    </Row>
    <Row
      about={
        settings.steer
          ? "enter steers a running turn, tab queues"
          : "enter queues for after the turn, tab steers it"
      }
      title="enter steers while the agent works"
    >
      <Switch
        checked={settings.steer}
        label="enter steers"
        onChange={(steer) => onChange({ steer })}
      />
    </Row>
    <Row
      about="chats sombre runs show up in claude code's own /resume"
      title="share chats with claude code"
    >
      <Switch
        checked={settings.publish}
        label="share chats with claude code"
        onChange={(publish) => onChange({ publish })}
      />
    </Row>
  </Card>
);

export const Preferences = ({
  settings,
  problem,
  chats,
  prefs,
  width,
  section,
  installed,
  onChange,
  onEdit,
  onPrefs,
  onReveal,
  onClose,
}: Props) => {
  const archived = chats.filter((chat) => prefs.archived.includes(keyed(chat)));
  const ready = settings !== null;
  const [current, setCurrent] = useState<Part>("general");

  useEffect(() => {
    if (section && ready) {
      document.querySelector(`#${anchor(section)}`)?.scrollIntoView();
    }
  }, [section, ready]);

  return (
    <main
      className="grid min-h-0"
      style={{ gridTemplateColumns: `${width}px minmax(0,1fr)` }}
    >
      <aside className="bg-ink border-rim flex min-h-0 flex-col border-r">
        <header
          className={`flex h-13 shrink-0 items-center gap-1 pr-2 ${mac ? "pl-[80px]" : "pl-2"}`}
          data-tauri-drag-region="deep"
        >
          <button
            aria-label="back to chats"
            className="text-fog hover:bg-raise hover:text-paper active:bg-well flex h-7 cursor-pointer items-center gap-2 rounded-lg px-2"
            onClick={onClose}
            title="back to chats (esc)"
            type="button"
          >
            <span aria-hidden="true">←</span>
            <span className="text-paper font-semibold">settings</span>
          </button>
        </header>
        <nav
          aria-label="settings sections"
          className="flex min-h-0 flex-col gap-px overflow-y-auto px-2 pb-3"
        >
          {PARTS.map((part) => (
            <button
              aria-current={part === current ? "true" : undefined}
              className="text-body hover:bg-raise hover:text-paper active:bg-well aria-[current]:bg-raise aria-[current]:text-paper h-8 shrink-0 cursor-pointer rounded-md px-2 text-left text-[13px]"
              key={part}
              onClick={() => {
                setCurrent(part);
                document
                  .querySelector(`#${anchor(part)}`)
                  ?.scrollIntoView({ behavior: "smooth" });
              }}
              type="button"
            >
              {part}
            </button>
          ))}
        </nav>
      </aside>
      <div className="flex min-h-0 flex-col">
        <header
          className={`border-rim flex h-13 shrink-0 items-center justify-end border-b pl-4 ${mac ? "pr-4" : "pr-36"}`}
          data-tauri-drag-region="deep"
        >
          <button
            aria-label="close settings"
            className="border-rim text-fog hover:border-fog hover:text-paper active:bg-well h-7 cursor-pointer rounded-md border px-2 text-[12px]"
            onClick={onClose}
            title="close settings"
            type="button"
          >
            esc
          </button>
        </header>
        <div
          className="min-h-0 flex-1 overflow-y-auto"
          onScroll={(event) => setCurrent(spot(event.currentTarget))}
        >
          {settings === null ? (
            <Notice
              late={!problem}
              onRetry={problem ? () => onChange({}) : undefined}
              title={
                problem
                  ? "couldn't read your settings"
                  : "reading your settings"
              }
            >
              {problem ? <p>{problem}</p> : null}
            </Notice>
          ) : (
            <div className="mx-auto flex max-w-[680px] flex-col gap-7 px-8 py-8">
              <General onChange={onChange} settings={settings} />
              <Card
                about="sombre runs the ones it finds on your computer"
                id="agents"
                title="agents"
              >
                {(installed ?? []).map((entry) =>
                  entry.version ? (
                    <Row
                      about={`version ${entry.version}`}
                      key={entry.agent}
                      title={
                        <span className="flex items-center gap-2">
                          <Logo agent={entry.agent} />
                          {names[entry.agent]}
                        </span>
                      }
                    />
                  ) : (
                    <Row
                      below={<Missing entry={entry} />}
                      key={entry.agent}
                      title={
                        <span className="flex items-center gap-2">
                          <Logo agent={entry.agent} />
                          {names[entry.agent]}
                        </span>
                      }
                    />
                  )
                )}
              </Card>
              <Card id="models" title="models">
                <Row
                  about="claude code and codex use their own default unless you pick one in a chat"
                  title="sombre's model"
                >
                  <Sombre
                    onPick={(model) => onChange({ model })}
                    value={settings.model}
                  />
                </Row>
              </Card>
              <Card
                about="model endpoints for sombre's agent"
                id="providers"
                title="providers"
              >
                <Providers
                  onEdit={onEdit}
                  presets={settings.presets}
                  providers={settings.providers}
                />
              </Card>
              <Card
                about="claude code and codex keep their own"
                id="mcp servers"
                title="mcp servers"
              >
                <Servers
                  offered={settings.offered}
                  onEdit={onEdit}
                  servers={settings.servers}
                />
              </Card>
              <Card id="skills" title="skills">
                {settings.skills.length === 0 ? (
                  <Row
                    about="skills live in .claude/skills or .sombre/skills in a folder or your home"
                    title="no skills for this folder"
                  />
                ) : (
                  settings.skills.map((skill) => (
                    <Row
                      about={skill.about}
                      key={skill.name}
                      title={skill.name}
                    />
                  ))
                )}
              </Card>
              <Card id="archive" title="archive">
                {archived.length === 0 ? (
                  <Row
                    about="archive a chat from its menu to hide it from the sidebar"
                    title="nothing archived"
                  />
                ) : (
                  archived.map((chat) => (
                    <Row
                      about={`${chat.project} · ${chat.ago}`}
                      key={keyed(chat)}
                      title={chat.title}
                    >
                      <button
                        className={`${chip} border-rim border`}
                        onClick={() =>
                          onPrefs({
                            ...prefs,
                            archived: toggle(prefs.archived, keyed(chat)),
                          })
                        }
                        type="button"
                      >
                        restore
                      </button>
                    </Row>
                  ))
                )}
              </Card>
              <Card id="shortcuts" title="shortcuts">
                {KEYS.map(([key, about]) => (
                  <Row key={key} title={about}>
                    <kbd className="border-rim text-body rounded-md border px-1.5 py-0.5 text-[12px]">
                      {key}
                    </kbd>
                  </Row>
                ))}
              </Card>
              <Card id="about" title="about">
                <Row about="version 0.1.0" title="sombre" />
                <Row about={settings.home} title="your sombre folder">
                  <button
                    className={`${chip} border-rim border`}
                    onClick={() => onReveal(settings.home)}
                    type="button"
                  >
                    show in finder
                  </button>
                </Row>
              </Card>
            </div>
          )}
        </div>
      </div>
    </main>
  );
};
