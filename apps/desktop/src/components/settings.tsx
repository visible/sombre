import { getVersion } from "@tauri-apps/api/app";
import type { ReactNode } from "react";
import { useEffect, useState } from "react";

import type {
  Agent,
  Change,
  Chat,
  Checked,
  Installed,
  Mode,
  Model,
  Settings,
} from "../lib/api";
import {
  agents,
  check,
  failure,
  models,
  modes,
  names,
  ready as downloaded,
} from "../lib/api";
import { mac, shortcut } from "../lib/platform";
import type { Prefs } from "../lib/sidebar";
import { keyed, toggle } from "../lib/sidebar";
import { Card, Row } from "./card";
import { primary } from "./field";
import { Logo } from "./logo";
import { Missing } from "./missing";
import { Notice } from "./notice";
import { Pick } from "./pick";
import { Caret, Option, Popover, Search, chip } from "./popover";
import type { Edit } from "./providers";
import { Providers } from "./providers";
import { Servers } from "./servers";
import { relaunch } from "./update";

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

const ABOUT: Record<Part, string> = {
  about: "the app, and where it keeps its files",
  agents: "sombre runs the ones it finds on your computer",
  archive: "chats you've hidden from the sidebar",
  general: "how new chats start, and how sombre behaves while it works",
  "mcp servers":
    "tools for sombre's agent. claude code and codex keep their own",
  models:
    "claude code and codex use their own default unless you pick one in a chat",
  providers: "model endpoints for sombre's agent",
  shortcuts: "every key sombre listens for",
  skills: "instructions sombre's agent loads when they fit the task",
};

const drawn: Record<Part, ReactNode> = {
  about: (
    <>
      <circle cx="8" cy="8" r="6" />
      <path d="M8 7.5V11M8 5h.01" />
    </>
  ),
  agents: (
    <>
      <rect height="8" rx="1.5" width="8" x="4" y="4" />
      <path d="M6 2v2M10 2v2M6 12v2M10 12v2M2 6h2M2 10h2M12 6h2M12 10h2" />
    </>
  ),
  archive: (
    <>
      <rect height="3" rx="1" width="12" x="2" y="3" />
      <path d="M3 6v6.5A1.5 1.5 0 0 0 4.5 14h7a1.5 1.5 0 0 0 1.5-1.5V6M6.5 9h3" />
    </>
  ),
  general: (
    <>
      <path d="M3 4.5h5.5M11.5 4.5H13M3 11.5h1.5M7.5 11.5H13" />
      <circle cx="10" cy="4.5" r="1.5" />
      <circle cx="6" cy="11.5" r="1.5" />
    </>
  ),
  "mcp servers": (
    <>
      <rect height="4" rx="1" width="11" x="2.5" y="2.5" />
      <rect height="4" rx="1" width="11" x="2.5" y="9.5" />
      <path d="M5 4.5h.01M5 11.5h.01" />
    </>
  ),
  models: <path d="M8 2 13.5 5v6L8 14 2.5 11V5zM2.5 5 8 8l5.5-3M8 8v6" />,
  providers: <path d="M6 2v3M10 2v3M4.5 5h7v2.5a3.5 3.5 0 0 1-7 0zM8 11v3" />,
  shortcuts: (
    <>
      <rect height="8" rx="1.5" width="13" x="1.5" y="4" />
      <path d="M4 7h.01M6.5 7h.01M9 7h.01M11.5 7h.01M5 9.5h6" />
    </>
  ),
  skills: (
    <path d="M3 3.5A1.5 1.5 0 0 1 4.5 2H13v10H4.5A1.5 1.5 0 0 0 3 13.5zM3 13.5A1.5 1.5 0 0 0 4.5 15H13v-3" />
  ),
};

const Glyph = ({ part }: { part: Part }) => (
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
    {drawn[part]}
  </svg>
);

const chosen = (value: string | null): Part | null =>
  PARTS.find((part) => part === value) ?? null;

type Status =
  | Checked
  | { kind: "idle" }
  | { kind: "checking" }
  | { kind: "failed"; problem: string };

const note = (status: Status) => {
  switch (status.kind) {
    case "checking": {
      return "checking for updates";
    }
    case "newest": {
      return "you're on the newest";
    }
    case "source": {
      return "built from source, so it doesn't update itself";
    }
    case "ready": {
      return `${status.version} is ready. restart to start using it`;
    }
    case "failed": {
      return status.problem;
    }
    default: {
      return null;
    }
  }
};

const Version = () => {
  const [version, setVersion] = useState<string | null>(null);
  const [status, setStatus] = useState<Status>({ kind: "idle" });
  const [restarting, setRestarting] = useState(false);

  useEffect(() => {
    getVersion()
      .then(setVersion)
      .catch(() => null);
    downloaded()
      .then((found) =>
        setStatus((current) =>
          found ? { kind: "ready", version: found } : current
        )
      )
      .catch(() => null);
  }, []);

  const look = async () => {
    setStatus({ kind: "checking" });
    try {
      setStatus(await check());
    } catch (error) {
      setStatus({ kind: "failed", problem: failure(error) });
    }
  };

  const go = async () => {
    setRestarting(true);
    if (!(await relaunch())) {
      setRestarting(false);
    }
  };

  const told = note(status);

  return (
    <Row
      about={
        <output aria-live="polite">
          {version ? `version ${version}` : "\u00A0"}
          {told ? ` · ${told}` : null}
        </output>
      }
      title="sombre"
    >
      {status.kind === "ready" ? (
        <button
          className={primary}
          disabled={restarting}
          onClick={go}
          type="button"
        >
          restart to update
        </button>
      ) : (
        <button
          className={`${chip} border-rim border`}
          disabled={status.kind === "checking"}
          onClick={look}
          type="button"
        >
          check for updates
        </button>
      )}
    </Row>
  );
};

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
      trigger={
        <>
          <span className="text-body truncate">{value}</span>
          <Caret />
        </>
      }
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
  <Card>
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
  const [current, setCurrent] = useState<Part>(
    () => chosen(section) ?? "general"
  );

  const page = (ready: Settings) => {
    switch (current) {
      case "general": {
        return <General onChange={onChange} settings={ready} />;
      }
      case "agents": {
        return (
          <Card>
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
        );
      }
      case "models": {
        return (
          <Card>
            <Row about="used by sombre's own agent" title="sombre's model">
              <Sombre
                onPick={(model) => onChange({ model })}
                value={ready.model}
              />
            </Row>
          </Card>
        );
      }
      case "providers": {
        return (
          <Card>
            <Providers
              onEdit={onEdit}
              presets={ready.presets}
              providers={ready.providers}
            />
          </Card>
        );
      }
      case "mcp servers": {
        return (
          <Card>
            <Servers
              offered={ready.offered}
              onEdit={onEdit}
              servers={ready.servers}
            />
          </Card>
        );
      }
      case "skills": {
        return (
          <Card>
            {ready.skills.length === 0 ? (
              <Row
                about="skills live in .claude/skills or .sombre/skills in a folder or your home"
                title="no skills for this folder"
              />
            ) : (
              ready.skills.map((skill) => (
                <Row about={skill.about} key={skill.name} title={skill.name} />
              ))
            )}
          </Card>
        );
      }
      case "archive": {
        return (
          <Card>
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
        );
      }
      case "shortcuts": {
        return (
          <Card>
            {KEYS.map(([key, about]) => (
              <Row key={key} title={about}>
                <kbd className="border-rim text-body rounded-md border px-1.5 py-0.5 text-[12px]">
                  {key}
                </kbd>
              </Row>
            ))}
          </Card>
        );
      }
      default: {
        return (
          <Card>
            <Version />
            <Row about={ready.home} title="your sombre folder">
              <button
                className={`${chip} border-rim border`}
                onClick={() => onReveal(ready.home)}
                type="button"
              >
                show in finder
              </button>
            </Row>
          </Card>
        );
      }
    }
  };

  return (
    <main
      className="grid min-h-0"
      style={{ gridTemplateColumns: `${width}px minmax(0,1fr)` }}
    >
      <aside className="bg-ink border-rim flex min-h-0 flex-col border-r">
        <header
          className={`flex h-13 shrink-0 items-center ${mac ? "pl-[80px]" : "pl-3"}`}
          data-tauri-drag-region="deep"
        />
        <nav
          aria-label="settings pages"
          className="flex min-h-0 flex-col gap-0.5 overflow-y-auto px-3 pb-3"
        >
          <button
            className="text-fog hover:bg-raise hover:text-paper active:bg-well mb-3 flex h-8 shrink-0 cursor-pointer items-center gap-2.5 rounded-lg px-2 text-left font-sans text-[13.5px]"
            onClick={onClose}
            title="back to chats (esc)"
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
              <path d="M13 8H3.5M7.5 4 3.5 8l4 4" />
            </svg>
            <span className="flex-1">back to chats</span>
            <kbd className="text-[11px]">esc</kbd>
          </button>
          {PARTS.map((part) => (
            <button
              aria-current={part === current ? "page" : undefined}
              className="text-body hover:bg-raise hover:text-paper active:bg-well aria-[current=page]:bg-well aria-[current=page]:text-paper flex h-8 shrink-0 cursor-pointer items-center gap-2.5 rounded-lg px-2 text-left font-sans text-[13.5px]"
              key={part}
              onClick={() => setCurrent(part)}
              type="button"
            >
              <span className="text-fog">
                <Glyph part={part} />
              </span>
              {part}
            </button>
          ))}
        </nav>
      </aside>
      <div className="flex min-h-0 flex-col">
        <header
          className={`border-rim flex h-13 shrink-0 items-center gap-2 border-b pl-5 font-sans ${mac ? "pr-4" : "pr-36"}`}
          data-tauri-drag-region="deep"
        >
          <span className="text-fog text-[13px]">settings</span>
          <span aria-hidden="true" className="text-edge text-[13px]">
            /
          </span>
          <span className="text-paper text-[13.5px] font-medium">
            {current}
          </span>
        </header>
        <div className="min-h-0 flex-1 overflow-y-auto">
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
            <div
              className="mx-auto flex max-w-[680px] flex-col gap-6 px-10 py-10"
              key={current}
            >
              <div className="flex flex-col gap-1">
                <h1 className="text-paper font-sans text-[20px] font-medium">
                  {current}
                </h1>
                <p className="text-fog font-sans text-[13.5px]">
                  {ABOUT[current]}
                </p>
              </div>
              {page(settings)}
            </div>
          )}
        </div>
      </div>
    </main>
  );
};
