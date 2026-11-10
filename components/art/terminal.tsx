import type { CSSProperties, ReactNode } from "react";

import type { Tone } from "@/lib/terminal";

export interface Line {
  tone: Tone;
  text: ReactNode;
  aside?: ReactNode;
}

interface Props {
  lines: readonly Line[];
  start?: number;
  size?: "regular" | "small";
}

const glyphs: Record<Tone, ReactNode> = {
  add: <span className="text-ink font-medium">+</span>,
  ask: <span className="text-ink font-medium">?</span>,
  command: <span className="text-ink font-medium">&#x276F;</span>,
  done: <span className="bg-ember mt-[7px] block size-[7px] rounded-[2px]" />,
  output: null,
  remove: <span className="text-meta">-</span>,
};

const tones: Record<Tone, string> = {
  add: "text-ink bg-paper rounded-sm px-1 -mx-1",
  ask: "text-ink",
  command: "text-ink font-medium",
  done: "text-body",
  output: "text-meta",
  remove: "text-meta line-through decoration-meta/50",
};

const sizes = {
  regular: "text-[11.5px] leading-[22px]",
  small: "text-[10px] leading-[19px]",
} as const;

export const Lines = ({ lines, start = 0, size = "regular" }: Props) => (
  <div className={`font-mono ${sizes[size]}`}>
    {lines.map((line, index) => (
      <p
        className="flex items-start gap-2 whitespace-nowrap"
        data-line=""
        key={`${line.tone}-${index}`}
        style={{ "--i": start + index } as CSSProperties}
      >
        <span className="flex w-3 shrink-0 justify-center">
          {glyphs[line.tone]}
        </span>
        <span className={`min-w-0 truncate ${tones[line.tone]}`}>
          {line.text}
        </span>
        {line.aside ? (
          <span className="text-meta ml-auto shrink-0 pl-3">{line.aside}</span>
        ) : null}
      </p>
    ))}
  </div>
);

export const Caret = ({ at }: { at: number }) => (
  <p
    className="flex items-center gap-2 font-mono text-[11.5px] leading-[22px]"
    data-line=""
    style={{ "--i": at } as CSSProperties}
  >
    <span className="text-ink flex w-3 justify-center font-medium">
      &#x276F;
    </span>
    <span
      className="bg-ink block h-[14px] w-[7px]"
      data-caret=""
      style={{ "--i": at } as CSSProperties}
    />
  </p>
);

export const Chip = ({ children }: { children: ReactNode }) => (
  <span className="border-line bg-paper text-body inline-flex items-center gap-1 rounded-md border px-1.5 font-sans text-[10px] leading-[18px]">
    {children}
  </span>
);
