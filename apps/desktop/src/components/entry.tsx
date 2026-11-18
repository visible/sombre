import { useState } from "react";

import type { Entry as Item } from "../lib/api";
import { took } from "../lib/format";
import { Copy } from "./copy";
import { Markdown } from "./markdown";
import { Pictures } from "./pictures";

const LONG = 700;
const LINES = 12;

const Prompt = ({
  text,
  steered,
  images,
}: {
  text: string;
  steered?: boolean;
  images?: string[];
}) => {
  const long = text.length > LONG || text.split("\n").length > LINES;
  const [open, setOpen] = useState(false);
  const clamped = long && !open;
  return (
    <div
      className={`text-paper bg-raise mt-3.5 rounded-r-lg border-l-2 py-2.5 pr-4 pl-4 font-sans text-[15px] leading-[1.6] first:mt-0 ${steered ? "border-fog/50 border-dashed" : "border-ember"}`}
    >
      {steered ? <span className="sr-only">sent while it worked: </span> : null}
      <div className="flex min-w-0 flex-col items-start">
        <p
          className={`[overflow-wrap:anywhere] whitespace-pre-wrap ${clamped ? "max-h-56 overflow-hidden [mask-image:linear-gradient(to_bottom,black_70%,transparent)]" : ""}`}
        >
          {text}
        </p>
        {long ? (
          <button
            className="text-fog hover:text-body active:text-paper mt-1 cursor-pointer rounded-sm text-[12.5px]"
            onClick={() => setOpen((current) => !current)}
            type="button"
          >
            {open ? "show less" : "show all"}
          </button>
        ) : null}
        <Pictures images={images ?? []} />
      </div>
    </div>
  );
};

const marks: Record<string, string> = {
  completed: "✓",
  in_progress: "›",
};

interface Props {
  entry: Item;
}

export const Entry = ({ entry }: Props) => {
  switch (entry.kind) {
    case "user": {
      return (
        <Prompt
          images={entry.images}
          steered={entry.steered}
          text={entry.text}
        />
      );
    }
    case "text": {
      return (
        <div className="group/reply relative">
          <Markdown text={entry.text} />
          <Copy
            className="absolute -top-0.5 -right-2 opacity-0 group-hover/reply:opacity-100 focus-visible:opacity-100"
            text={entry.text}
          />
        </div>
      );
    }
    case "plan": {
      return (
        <section className="pl-[2ch]">
          <p className="text-paper mb-1">
            <span aria-hidden="true" className="text-fog">
              ●{" "}
            </span>
            plan
          </p>
          <ol className="pl-[2ch]">
            {entry.tasks.map((task) => (
              <li
                className={`grid grid-cols-[2ch_minmax(0,1fr)] ${task.status === "in_progress" ? "text-paper" : "text-fog"}`}
                key={`${task.status}:${task.text}`}
              >
                <span
                  aria-hidden="true"
                  className={task.status in marks ? "text-ember" : ""}
                >
                  {marks[task.status] ?? "·"}
                </span>
                <span
                  className={
                    task.status === "completed"
                      ? "decoration-rim line-through"
                      : ""
                  }
                >
                  {task.text}
                </span>
              </li>
            ))}
          </ol>
        </section>
      );
    }
    case "note": {
      return (
        <p className="text-fog pl-[2ch] [overflow-wrap:anywhere] whitespace-pre-wrap">
          {entry.text}
        </p>
      );
    }
    case "failed": {
      const [headline, ...rest] = entry.text
        .split("\n")
        .filter((line) => line.trim());
      return (
        <div className="pl-[2ch] [overflow-wrap:anywhere]">
          <p className="text-paper pl-[2ch] -indent-[2ch]">
            <span aria-hidden="true" className="text-ember">
              ✕{" "}
            </span>
            {headline}
          </p>
          {rest.map((line) => (
            <p className="text-fog pl-[2ch]" key={line}>
              {line}
            </p>
          ))}
        </div>
      );
    }
    case "done": {
      const summary = took(entry.millis);
      return summary ? (
        <p className="text-fog -mt-1.5 pl-[2ch] text-[12.5px]">{summary}</p>
      ) : null;
    }
    default: {
      return null;
    }
  }
};
