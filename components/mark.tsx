import type { CSSProperties } from "react";

import { rays } from "@/lib/rays";

interface Props {
  className?: string;
}

const horizon = 14;

const lines = rays({
  count: 13,
  height: horizon,
  inner: 7,
  jitter: 0,
  outer: 12.5,
  width: 40,
}).slice(1, -1);

const middle = (lines.length - 1) / 2;

export const Mark = ({ className }: Props) => (
  <svg
    aria-hidden="true"
    className={`overflow-visible ${className ?? ""}`}
    fill="none"
    viewBox="0 0 40 15"
  >
    <g stroke="currentColor" strokeLinecap="round" strokeWidth="1.1">
      {lines.map((line, index) => (
        <line
          data-spoke=""
          key={`${line.x2}-${line.y2}`}
          style={{ "--i": Math.abs(index - middle) } as CSSProperties}
          x1={line.x1}
          x2={line.x2}
          y1={line.y1}
          y2={line.y2}
        />
      ))}
      <line x1="1.5" x2="38.5" y1={horizon} y2={horizon} />
    </g>
  </svg>
);
