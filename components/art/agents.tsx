import type { CSSProperties } from "react";

import { Window } from "@/components/art/frame";
import { Lines } from "@/components/art/terminal";
import type { Line } from "@/components/art/terminal";

const options = [
  { name: "Claude Code", note: "Anthropic" },
  { name: "Codex", note: "OpenAI" },
] as const;

const before: Line[] = [
  { text: "sombre new", tone: "command" },
  { text: "Which agent?", tone: "ask" },
];

const after: Line[] = [
  { aside: "~/code/atlas", text: "Project", tone: "ask" },
  { text: "Session ready", tone: "done" },
  { text: "Open sombre.si/k4x2 on your phone", tone: "output" },
];

export const Agents = () => (
  <Window
    className="absolute top-6 left-1/2 w-[min(420px,88%)] -translate-x-1/2"
    name="sombre new"
  >
    <div className="px-4 pt-3.5 pb-10">
      <Lines lines={before} />
      <div
        className="relative my-1.5 ml-5 font-mono text-[11.5px]"
        data-line=""
        style={{ "--i": before.length } as CSSProperties}
      >
        <span
          className="bg-paper ring-line absolute inset-x-0 top-0 h-[30px] rounded-md ring-1"
          data-pick=""
        />
        {options.map((option, index) => (
          <p
            className="relative flex h-[30px] items-center gap-2.5 px-2.5"
            key={option.name}
          >
            <span
              className={`flex size-3 items-center justify-center rounded-full border ${index === 0 ? "border-ink" : "border-meta/50"}`}
            >
              {index === 0 ? (
                <span className="bg-ember size-1.5 rounded-full" />
              ) : null}
            </span>
            <span
              className={index === 0 ? "text-ink font-medium" : "text-body"}
            >
              {option.name}
            </span>
            <span className="text-meta ml-auto font-sans text-[10px]">
              {option.note}
            </span>
          </p>
        ))}
      </div>
      <Lines lines={after} start={before.length + 1} />
    </div>
  </Window>
);
