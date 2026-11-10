import type { CSSProperties, ReactNode } from "react";

import { linked } from "@/lib/timeline";
import type { Icon, Kind, Scheduled } from "@/lib/timeline";

type Device = "desktop" | "phone";

interface Props {
  entries: Scheduled[];
  device: Device;
}

interface Body {
  entry: Scheduled;
  device: Device;
  before: boolean;
  after: boolean;
}

const text = {
  desktop: "text-[13px] leading-relaxed",
  phone: "text-[12px] leading-snug",
} as const;

const layout = {
  desktop: "items-center",
  phone: "flex-col items-start",
} as const;

const shown = { desktop: 3, phone: 1 } as const;

const icons: Record<Icon, ReactNode> = {
  edit: <path d="M8.5 2.5 10 4 4.5 9.5 2.5 10l.5-2Z" />,
  run: <path d="m2.5 3.5 3 2.5-3 2.5M6.5 9.5h3" />,
  search: <path d="M5.5 9a3.5 3.5 0 1 0 0-7 3.5 3.5 0 0 0 0 7ZM10 10 8 8" />,
};

const shimmer = (entry: Scheduled): CSSProperties => ({
  animationDelay: `${entry.delay + 300}ms`,
  animationIterationCount: Math.max(0.5, (entry.span - 300) / 1200),
});

const Step = ({ entry, device, before, after }: Body) => {
  const files = entry.files ?? [];
  const visible = files.slice(0, shown[device]);
  const more = files.length - visible.length;
  return (
    <div className="relative grid grid-cols-[14px_1fr] gap-x-2.5">
      {before ? (
        <span className="bg-line absolute top-[-12px] left-[6.5px] h-[15px] w-px" />
      ) : null}
      {after ? (
        <span className="bg-line absolute top-[17px] bottom-0 left-[6.5px] w-px" />
      ) : null}
      <svg
        className="text-meta mt-[3px] size-3.5"
        fill="none"
        stroke="currentColor"
        strokeLinecap="round"
        strokeLinejoin="round"
        strokeWidth="1.25"
        viewBox="0 0 12 12"
      >
        {entry.icon ? icons[entry.icon] : null}
      </svg>
      <div className="min-w-0">
        <p className="flex items-baseline justify-between gap-3 text-[12px]">
          <span className="shimmer text-meta truncate" style={shimmer(entry)}>
            {entry.text}
          </span>
          {entry.detail ? (
            <span className="text-meta shrink-0 font-mono text-[11px]">
              {entry.detail}
            </span>
          ) : null}
        </p>
        {visible.length > 0 ? (
          <p className="mt-1.5 flex flex-wrap items-center gap-1">
            {visible.map((file) => (
              <span
                className="border-line bg-sheet text-body flex items-center gap-1 rounded-full border px-2 py-0.5 text-[11px]"
                key={file}
              >
                <span className="text-meta font-mono text-[8px] font-medium">
                  {file.endsWith(".tsx") ? "TSX" : "TS"}
                </span>
                {file}
              </span>
            ))}
            {more > 0 ? (
              <span className="text-meta px-1 text-[11px]">+{more} more</span>
            ) : null}
          </p>
        ) : null}
      </div>
    </div>
  );
};

const bodies: Record<Kind, (props: Body) => ReactNode> = {
  agent: ({ entry, device }) => (
    <p className={`text-ink max-w-[92%] ${text[device]}`}>{entry.text}</p>
  ),
  approval: ({ entry, device }) => (
    <p
      className={`border-line text-body flex justify-between gap-x-3 gap-y-1.5 rounded-lg border px-2.5 py-2 font-mono text-[11px] ${layout[device]}`}
    >
      <span className="truncate">{entry.text}</span>
      <span className="bg-ember text-ink shrink-0 rounded-full px-2 py-0.5 font-sans font-medium">
        {entry.detail}
      </span>
    </p>
  ),
  handoff: ({ entry }) => (
    <p className="text-ink flex items-center gap-3 text-[11px] font-medium">
      <span className="bg-ember h-px flex-1" />
      {entry.text}
      <span className="bg-ember h-px flex-1" />
    </p>
  ),
  step: Step,
  user: ({ entry, device }) => (
    <p
      className={`bg-paper text-ink ml-auto w-fit max-w-[85%] rounded-2xl rounded-br-md px-3 py-2 ${text[device]}`}
    >
      {entry.text}
    </p>
  ),
};

export const Thread = ({ entries, device }: Props) => (
  <div className="flex min-h-0 flex-1 flex-col justify-end [mask-image:linear-gradient(to_bottom,transparent,black_18%)]">
    {entries.map((entry, index) => {
      const Render = bodies[entry.kind];
      return (
        <div
          className="reveal"
          key={entry.id}
          style={{ animationDelay: `${entry.delay}ms` }}
        >
          <div className="min-h-0 overflow-hidden pt-3">
            <Render
              after={linked(entries, index)}
              before={index > 0 && linked(entries, index - 1)}
              device={device}
              entry={entry}
            />
          </div>
        </div>
      );
    })}
  </div>
);
