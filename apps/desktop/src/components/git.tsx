import { openUrl } from "@tauri-apps/plugin-opener";
import { useEffect, useMemo, useState } from "react";

import type { Repo } from "../lib/api";
import {
  branch as create,
  commit,
  failure,
  push,
  repo as read,
  request,
} from "../lib/api";
import { counted, suggest, summary } from "../lib/git";
import { split } from "../lib/patch";
import { toast } from "../lib/toasts";
import { Field, primary, secondary } from "./field";
import { Divider, Option, Popover } from "./popover";
import { Sheet } from "./sheet";

const SHOWN = 8;

interface Props {
  folder: string;
  version: number;
  compact: boolean;
}

const Icon = () => (
  <svg
    aria-hidden="true"
    className="size-3.5"
    fill="none"
    stroke="currentColor"
    strokeLinecap="round"
    strokeWidth="1.4"
    viewBox="0 0 16 16"
  >
    <circle cx="4.5" cy="3.5" r="1.5" />
    <circle cx="4.5" cy="12.5" r="1.5" />
    <circle cx="11.5" cy="5.5" r="1.5" />
    <path d="M4.5 5v6M11.5 7c0 3-7 2-7 4" />
  </svg>
);

const pushing = (state: Repo) => {
  if (!state.remote) {
    return "this repository has no remote";
  }
  if (!state.branch) {
    return "check out a branch first";
  }
  if (!state.upstream) {
    return "publishes this branch";
  }
  return state.ahead > 0
    ? `${counted(state.ahead, "commit", "commits")} to push`
    : "up to date";
};

export const Git = ({ folder, version, compact }: Props) => {
  const [state, setState] = useState<Repo | null>(null);
  const [round, setRound] = useState(0);
  const [doing, setDoing] = useState<string | null>(null);
  const [sheet, setSheet] = useState<"commit" | "branch" | null>(null);
  const [message, setMessage] = useState("");
  const [name, setName] = useState("");
  const asked = useMemo(
    () => ({ folder, round, version }),
    [folder, round, version]
  );

  useEffect(() => {
    let alive = true;
    const load = async () => {
      try {
        const found = await read(asked.folder);
        if (alive) {
          setState(found);
        }
      } catch {
        if (alive) {
          setState(null);
        }
      }
    };
    load();
    return () => {
      alive = false;
    };
  }, [asked]);

  useEffect(() => {
    const again = () => setRound((current) => current + 1);
    window.addEventListener("focus", again);
    return () => window.removeEventListener("focus", again);
  }, []);

  if (!state) {
    return null;
  }

  const act = async (
    label: string,
    progress: string,
    work: () => Promise<void>
  ) => {
    setDoing(progress);
    try {
      await work();
    } catch (error) {
      toast({ body: failure(error), title: `couldn't ${label}`, wrap: true });
    }
    setDoing(null);
    setRound((current) => current + 1);
  };

  const opened = () =>
    act("open a pull request", "opening", async () => {
      const url = await request(folder);
      await openUrl(url);
      toast({
        action: { label: "view", run: () => openUrl(url) },
        body: url,
        title: "pull request ready",
      });
    });

  const pushed = () =>
    act("push", "pushing", async () => {
      const where = await push(folder);
      toast({
        action: { label: "pull request", run: opened },
        title: `pushed ${where}`,
      });
    });

  const committed = (then: boolean) =>
    act("commit", "committing", async () => {
      const sha = await commit(folder, message);
      setSheet(null);
      toast({
        action:
          state.remote && !then ? { label: "push", run: pushed } : undefined,
        body: message.trim(),
        title: `committed ${sha}`,
      });
      if (then) {
        const where = await push(folder);
        toast({
          action: { label: "pull request", run: opened },
          title: `pushed ${where}`,
        });
      }
    });

  const branched = () =>
    act("make the branch", "branching", async () => {
      const made = await create(folder, name);
      setSheet(null);
      setName("");
      toast({ title: `switched to ${made}` });
    });

  const label = doing ?? state.branch ?? "detached";
  const about = summary(state.branch ?? "detached", state.changed, state.ahead);

  return (
    <>
      <Popover
        align="end"
        className="border-rim text-fog hover:border-edge hover:text-paper active:bg-well aria-expanded:border-edge aria-expanded:text-paper flex h-7 min-w-0 cursor-pointer items-center gap-1.5 rounded-full border px-2.5 font-mono text-[12px]"
        disabled={doing !== null}
        label={`git: ${about}`}
        side="bottom"
        title={about}
        trigger={
          <>
            <Icon />
            {compact && doing === null ? null : (
              <span className="max-w-36 truncate">{label}</span>
            )}
            {state.ahead > 0 ? (
              <span className="text-body tabular-nums">{`↑${state.ahead}`}</span>
            ) : null}
            {state.changed > 0 ? (
              <span
                aria-hidden="true"
                className="bg-ember size-1.5 shrink-0 rounded-full"
              />
            ) : null}
          </>
        }
        width="w-72"
      >
        {(close) => (
          <>
            <Option
              disabled={state.changed === 0}
              hint={
                state.changed > 0
                  ? counted(state.changed, "changed file", "changed files")
                  : "nothing to commit"
              }
              icon="✓"
              label="commit"
              onPick={() => {
                close();
                setMessage(suggest(state.files));
                setSheet("commit");
              }}
            />
            <Option
              disabled={
                !(state.remote && state.branch) ||
                (state.upstream !== null && state.ahead === 0)
              }
              hint={pushing(state)}
              icon="↑"
              label={state.upstream ? "push" : "publish branch"}
              onPick={() => {
                close();
                pushed();
              }}
            />
            <Option
              disabled={!(state.remote && state.branch)}
              hint="opens it on github with gh"
              icon="⇡"
              label="pull request"
              onPick={() => {
                close();
                opened();
              }}
            />
            <Divider />
            <Option
              hint={`starts from ${state.branch ?? "this commit"}`}
              icon="+"
              label="new branch"
              onPick={() => {
                close();
                setSheet("branch");
              }}
            />
          </>
        )}
      </Popover>
      <Sheet
        busy={doing !== null}
        onClose={() => setSheet(null)}
        open={sheet === "commit"}
        title={`commit ${counted(state.changed, "change", "changes")} on ${state.branch ?? "this commit"}`}
      >
        <label className="mt-4 flex flex-col gap-1">
          <span className="text-fog text-[11.5px]">message</span>
          <textarea
            className="border-rim bg-well text-paper placeholder:text-fog focus:border-fog min-h-20 resize-none rounded-md border px-2 py-1.5 text-[13px] outline-none"
            onChange={(event) => setMessage(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter" && (event.metaKey || event.ctrlKey)) {
                event.preventDefault();
                committed(false);
              }
            }}
            placeholder="what this commit does"
            spellCheck={false}
            value={message}
          />
        </label>
        <ul className="text-fog mt-3 flex flex-col gap-0.5 text-[12px]">
          {state.files.slice(0, SHOWN).map((path) => {
            const parts = split(path);
            return (
              <li className="truncate" key={path}>
                {parts.folder}
                <span className="text-body">{parts.name}</span>
              </li>
            );
          })}
          {state.changed > SHOWN ? (
            <li>{`and ${state.changed - SHOWN} more`}</li>
          ) : null}
        </ul>
        <div className="mt-5 flex justify-end gap-2">
          <button
            className={secondary}
            disabled={doing !== null}
            onClick={() => setSheet(null)}
            type="button"
          >
            cancel
          </button>
          {state.remote ? (
            <button
              className={secondary}
              disabled={doing !== null || message.trim() === ""}
              onClick={() => committed(true)}
              type="button"
            >
              commit and push
            </button>
          ) : null}
          <button
            className={primary}
            disabled={doing !== null || message.trim() === ""}
            onClick={() => committed(false)}
            type="button"
          >
            {doing === "committing" ? "committing" : "commit"}
          </button>
        </div>
      </Sheet>
      <Sheet
        busy={doing !== null}
        onClose={() => setSheet(null)}
        open={sheet === "branch"}
        title="new branch"
      >
        <div className="mt-4 flex items-end gap-2">
          <Field
            focus={sheet === "branch"}
            label="name"
            onCancel={() => setSheet(null)}
            onChange={setName}
            onSubmit={branched}
            placeholder="fix/login-redirect"
            value={name}
          />
          <button
            className={primary}
            disabled={doing !== null || name.trim() === ""}
            onClick={branched}
            type="button"
          >
            create
          </button>
        </div>
      </Sheet>
    </>
  );
};
