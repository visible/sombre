import { memo, useState } from "react";

import type { Entry } from "../lib/api";
import type { Tone } from "../lib/format";
import { clip, diff } from "../lib/format";

const SHOWN = 5;

const tones: Record<Tone, string> = {
  add: "text-paper",
  gone: "text-fog line-through decoration-rim",
  same: "text-fog",
};

interface Props {
  step: Entry & { kind: "step" };
}

const Row = ({ step }: Props) => {
  const [open, setOpen] = useState(step.failed);
  const [full, setFull] = useState(false);
  const output = clip(step.output, full ? Number.POSITIVE_INFINITY : SHOWN);
  const more = step.output !== "" || step.patch.length > 0;

  return (
    <div className="grid grid-cols-[1ch_auto_minmax(0,1fr)] gap-x-[1ch]">
      <span
        aria-hidden="true"
        className={step.failed ? "text-ember" : "text-fog"}
      >
        {step.failed ? "✕" : "●"}
      </span>
      <span className="text-body min-w-[4ch]">{step.label}</span>
      {step.detail || more ? (
        <button
          aria-expanded={open}
          className={`text-fog hover:text-body active:text-paper cursor-pointer truncate rounded-sm text-left ${open ? "text-body [overflow-wrap:anywhere] whitespace-pre-wrap" : ""}`}
          onClick={() => setOpen(!open)}
          type="button"
        >
          {step.detail || "output"}
        </button>
      ) : null}
      {open && step.output ? (
        <pre className="text-fog col-span-2 col-start-2 mt-0.5 text-[12.5px] [overflow-wrap:anywhere] whitespace-pre-wrap">
          {`└ ${output.lines.join("\n  ")}`}
        </pre>
      ) : null}
      {open && output.hidden > 0 ? (
        <button
          className="text-fog hover:text-body active:text-paper col-span-2 col-start-2 mb-1 ml-[2ch] cursor-pointer justify-self-start rounded-sm text-[12.5px]"
          onClick={() => setFull(true)}
          type="button"
        >
          {`… +${output.hidden} ${output.hidden === 1 ? "line" : "lines"}`}
        </button>
      ) : null}
      {open && step.patch.length > 0 ? (
        <pre className="col-span-2 col-start-2 mt-0.5 overflow-x-auto text-[12.5px]">
          {diff(step.patch).map((line) => (
            <span className={`block ${tones[line.tone]}`} key={line.key}>
              {line.text}
            </span>
          ))}
        </pre>
      ) : null}
    </div>
  );
};

export const Step = memo(Row);
