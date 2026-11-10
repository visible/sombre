import type { CSSProperties } from "react";

export interface Row {
  indent: number;
  widths: readonly number[];
}

interface Props {
  rows: readonly Row[];
  numbers?: boolean;
  unit?: number;
}

const tones = ["bg-ink", "bg-meta/45", "bg-line", "bg-meta/45"] as const;

export const Pills = ({ rows, numbers = false, unit = 4 }: Props) => (
  <div className="space-y-2.5">
    {rows.map((row, line) => (
      <div
        className="flex items-center gap-1.5"
        key={`${row.indent}-${row.widths.join(",")}-${line}`}
      >
        {numbers ? (
          <span className="text-meta/70 w-4 shrink-0 font-mono text-[9px]">
            {line + 1}
          </span>
        ) : null}
        <span
          className="shrink-0"
          style={{ width: `${row.indent * unit}px` }}
        />
        {row.widths.map((width, column) => (
          <span
            className={`h-2 shrink-0 rounded-full ${tones[(line + column) % tones.length]}`}
            data-type=""
            key={`${line}-${column}-${width}`}
            style={
              {
                "--i": line + column * 0.4,
                width: `${width * unit}px`,
              } as CSSProperties
            }
          />
        ))}
      </div>
    ))}
  </div>
);
