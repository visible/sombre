import type { ReactNode } from "react";

import { Caret, Option, Popover, chip } from "./popover";

interface Props<T extends string> {
  label: string;
  value: T;
  options: { value: T; label: ReactNode; hint?: string }[];
  onPick: (value: T) => void;
}

export const Pick = <T extends string>({
  label,
  value,
  options,
  onPick,
}: Props<T>) => (
  <Popover
    align="end"
    className={`${chip} border-rim border`}
    label={label}
    side="bottom"
    trigger={
      <>
        <span className="text-body">
          {options.find((option) => option.value === value)?.label ?? value}
        </span>
        <Caret />
      </>
    }
  >
    {(close) =>
      options.map((option) => (
        <Option
          hint={option.hint}
          key={option.value}
          label={option.label}
          onPick={() => {
            onPick(option.value);
            close();
          }}
          selected={option.value === value}
        />
      ))
    }
  </Popover>
);
