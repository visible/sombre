import type { KeyboardEvent } from "react";
import { useEffect, useRef } from "react";

import type { Choice } from "../lib/api";
import { armed, typing } from "../lib/arm";
import { diff } from "../lib/format";
import type { Asked } from "../lib/live";
import { asked } from "../lib/live";

interface Props {
  entry: Asked;
  agent: string;
  onAnswer: (choice: Choice) => void;
  onCycle: () => void;
}

const steps: Record<string, number> = {
  ArrowDown: 1,
  ArrowLeft: -1,
  ArrowRight: 1,
  ArrowUp: -1,
};

const move = (event: KeyboardEvent<HTMLButtonElement>) => {
  const step = steps[event.key];
  if (step === undefined) {
    return false;
  }
  event.preventDefault();
  const buttons = [
    ...(event.currentTarget.parentElement?.querySelectorAll("button") ?? []),
  ];
  const at = buttons.indexOf(event.currentTarget);
  buttons.at((at + step) % buttons.length)?.focus();
  return true;
};

const answers: Record<Choice, string> = {
  always: "allowed for this chat",
  no: "denied",
  yes: "allowed",
};

export const Ask = ({ entry, agent, onAnswer, onCycle }: Props) => {
  const first = useRef<HTMLButtonElement>(null);
  const since = useRef(Number.POSITIVE_INFINITY);

  useEffect(() => {
    if (entry.answer !== null) {
      return;
    }
    since.current = performance.now();
    if (!typing(document.activeElement)) {
      first.current?.focus();
    }
  }, [entry.answer]);

  const choose = (choice: Choice) => {
    if (armed(since.current, performance.now())) {
      onAnswer(choice);
    }
  };

  if (entry.answer !== null) {
    return (
      <p
        className={`pl-[2ch] ${entry.answer === "no" ? "text-fog" : "text-paper"}`}
      >
        <span
          aria-hidden="true"
          className={entry.answer === "no" ? "" : "text-ember"}
        >
          {entry.answer === "no" ? "✕ " : "✔ "}
        </span>
        {entry.tool === "Bash"
          ? `$ ${entry.detail.split("\n")[0]}`
          : entry.detail.split("\n")[0]}
        <span className="text-fog">{`  ${answers[entry.answer]}`}</span>
      </p>
    );
  }

  const lines = diff(entry.change.slice(0, 40));

  const keys = (event: KeyboardEvent<HTMLButtonElement>) => {
    if (event.repeat) {
      event.preventDefault();
      return;
    }
    if (event.key === "Tab" && event.shiftKey) {
      event.preventDefault();
      if (armed(since.current, performance.now())) {
        onCycle();
      }
      return;
    }
    if (move(event)) {
      return;
    }
    const key = event.key.toLowerCase();
    if (key === "y") {
      choose("yes");
    } else if (key === "a" && entry.always) {
      choose("always");
    } else if (key === "n" || key === "escape") {
      event.stopPropagation();
      choose("no");
    }
  };

  return (
    <section
      aria-label={asked(entry.tool)}
      className="border-rim bg-raise ml-[2ch] flex flex-col gap-3 rounded-xl border p-3.5"
    >
      <h2 className="text-paper font-semibold">{asked(entry.tool)}</h2>
      {entry.detail ? (
        <pre className="bg-ink text-paper overflow-x-auto rounded-lg px-3 py-2.5 text-[13px] [overflow-wrap:anywhere] whitespace-pre-wrap">
          {entry.tool === "Bash" ? `$ ${entry.detail}` : entry.detail}
        </pre>
      ) : null}
      {entry.reason ? <p className="text-fog">{entry.reason}</p> : null}
      {lines.length > 0 ? (
        <pre className="bg-ink overflow-x-auto rounded-lg px-3 py-2.5 text-[13px]">
          {lines.map((line) => (
            <span
              className={`block ${line.tone === "gone" ? "text-fog decoration-rim line-through" : "text-paper"}`}
              key={line.key}
            >
              {line.text}
            </span>
          ))}
        </pre>
      ) : null}
      <div className="flex flex-wrap gap-2">
        <button
          className="bg-ember text-ink focus-visible:outline-paper min-h-9 cursor-pointer rounded-lg px-3.5 font-semibold hover:brightness-110 active:brightness-95"
          onClick={() => choose("yes")}
          onKeyDown={keys}
          ref={first}
          type="button"
        >
          yes <span className="font-normal opacity-70">y</span>
        </button>
        {entry.always ? (
          <button
            className="border-rim text-paper hover:border-fog active:bg-well min-h-9 cursor-pointer rounded-lg border px-3.5 text-left"
            onClick={() => choose("always")}
            onKeyDown={keys}
            type="button"
          >
            {`yes, and ${entry.always}`} <span className="text-fog">a</span>
          </button>
        ) : null}
        <button
          className="border-rim text-paper hover:border-fog active:bg-well min-h-9 cursor-pointer rounded-lg border px-3.5 text-left"
          onClick={() => choose("no")}
          onKeyDown={keys}
          type="button"
        >
          {`no, and tell ${agent} what to do instead`}{" "}
          <span className="text-fog">esc</span>
        </button>
      </div>
    </section>
  );
};
