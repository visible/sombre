import type { ReactNode } from "react";
import { useCallback, useEffect, useRef, useState } from "react";

import { fit } from "../lib/place";

export const chip =
  "flex h-7 shrink-0 cursor-pointer items-center gap-1.5 rounded-lg px-2 text-[12.5px] text-fog hover:bg-well hover:text-body active:bg-rim aria-expanded:bg-well aria-expanded:text-paper disabled:cursor-default disabled:opacity-40 disabled:hover:bg-transparent";

export const Caret = () => (
  <svg
    aria-hidden="true"
    className="size-3 shrink-0 opacity-70"
    fill="none"
    stroke="currentColor"
    strokeLinecap="round"
    strokeLinejoin="round"
    strokeWidth="1.5"
    viewBox="0 0 16 16"
  >
    <path d="m4.5 6.5 3.5 3.5 3.5-3.5" />
  </svg>
);

type Side = "top" | "bottom";

const place = (
  panel: HTMLElement,
  trigger: HTMLElement,
  side: Side,
  align: "start" | "end",
  flip: boolean
): Side => {
  panel.style.maxHeight = "";
  const spot = fit(
    trigger.getBoundingClientRect(),
    { height: panel.offsetHeight, width: panel.offsetWidth },
    { height: window.innerHeight, width: window.innerWidth },
    side,
    align,
    flip
  );
  panel.style.maxHeight = `${spot.height}px`;
  panel.style.top = spot.top === null ? "" : `${spot.top}px`;
  panel.style.bottom = spot.bottom === null ? "" : `${spot.bottom}px`;
  panel.style.left = `${spot.left}px`;
  return spot.top === null ? "top" : "bottom";
};

const shown = (trigger: HTMLElement, panel: HTMLElement) => {
  const box = trigger.getBoundingClientRect();
  const found = document
    .elementsFromPoint(box.left + box.width / 2, box.top + box.height / 2)
    .find((element) => !panel.contains(element));
  return found !== undefined && trigger.contains(found);
};

const move = (panel: HTMLElement, step: number) => {
  const options = [
    ...panel.querySelectorAll<HTMLElement>("[data-option]:not(:disabled)"),
  ];
  if (options.length === 0) {
    return;
  }
  const active = document.activeElement;
  const at = active instanceof HTMLElement ? options.indexOf(active) : -1;
  let next = (at + step + options.length) % options.length;
  if (at === -1) {
    next = step > 0 ? 0 : options.length - 1;
  }
  options[next]?.focus();
};

interface Props {
  label: string;
  title?: string;
  trigger: ReactNode;
  className?: string;
  side?: "top" | "bottom";
  align?: "start" | "end";
  width?: string;
  disabled?: boolean;
  open?: boolean;
  onOpen?: (open: boolean) => void;
  children: (close: () => void) => ReactNode;
}

export const Popover = ({
  label,
  title,
  trigger,
  className = chip,
  side = "top",
  align = "start",
  width = "w-72",
  disabled,
  open: forced,
  onOpen,
  children,
}: Props) => {
  const [inner, setInner] = useState(false);
  const button = useRef<HTMLButtonElement>(null);
  const panel = useRef<HTMLDivElement>(null);
  const chosen = useRef<Side>(side);
  const open = forced ?? inner;

  const set = useCallback(
    (next: boolean) => {
      setInner(next);
      onOpen?.(next);
    },
    [onOpen]
  );

  const close = useCallback(() => set(false), [set]);

  const attach = useCallback(
    (node: HTMLDivElement | null) => {
      panel.current = node;
      if (node && button.current) {
        chosen.current = place(node, button.current, side, align, true);
      }
    },
    [side, align]
  );

  useEffect(() => {
    if (!open) {
      return;
    }
    const first = panel.current?.querySelector<HTMLElement>(
      "[data-focus], [data-option]:not(:disabled)"
    );
    first?.focus();
    let typed = false;
    const keys = (event: KeyboardEvent) => {
      typed = true;
      if (!panel.current) {
        return;
      }
      if (event.key === "Escape") {
        event.preventDefault();
        event.stopPropagation();
        set(false);
      } else if (event.key === "ArrowDown" || event.key === "ArrowUp") {
        event.preventDefault();
        move(panel.current, event.key === "ArrowDown" ? 1 : -1);
      }
    };
    const outside = (event: PointerEvent) => {
      typed = false;
      const { target } = event;
      if (
        target instanceof Node &&
        !panel.current?.contains(target) &&
        !button.current?.contains(target)
      ) {
        set(false);
      }
    };
    const shut = () => set(false);
    const opener = button.current;
    const inside = (target: EventTarget | null) =>
      target instanceof Node && panel.current?.contains(target) === true;
    let frame = 0;
    const follow = (event?: Event) => {
      if (event && inside(event.target)) {
        return;
      }
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(() => {
        if (!panel.current || !opener) {
          return;
        }
        if (shown(opener, panel.current)) {
          place(panel.current, opener, chosen.current, align, false);
        } else {
          set(false);
        }
      });
    };
    const hold = (event: WheelEvent) => {
      if (!inside(event.target)) {
        event.preventDefault();
      }
    };
    const watcher = new ResizeObserver(() => follow());
    if (panel.current) {
      watcher.observe(panel.current);
    }
    document.addEventListener("keydown", keys, true);
    document.addEventListener("pointerdown", outside, true);
    document.addEventListener("scroll", follow, true);
    document.addEventListener("wheel", hold, { capture: true, passive: false });
    window.addEventListener("resize", shut);
    window.addEventListener("blur", shut);
    return () => {
      cancelAnimationFrame(frame);
      watcher.disconnect();
      document.removeEventListener("keydown", keys, true);
      document.removeEventListener("pointerdown", outside, true);
      document.removeEventListener("scroll", follow, true);
      document.removeEventListener("wheel", hold, { capture: true });
      window.removeEventListener("resize", shut);
      window.removeEventListener("blur", shut);
      const lost =
        !document.activeElement || document.activeElement === document.body;
      if (typed && lost) {
        opener?.focus();
      }
    };
  }, [open, set, align]);

  return (
    <>
      <button
        aria-expanded={open}
        aria-label={label}
        className={className}
        disabled={disabled}
        onClick={() => set(!open)}
        ref={button}
        title={title}
        type="button"
      >
        {trigger}
      </button>
      {open ? (
        <div
          className={`border-rim bg-raise animate-pop fixed z-50 flex flex-col overflow-hidden rounded-xl border p-1 shadow-[0_12px_40px_rgb(0_0_0/0.5)] motion-reduce:animate-none ${width}`}
          ref={attach}
        >
          {children(close)}
        </div>
      ) : null}
    </>
  );
};

interface OptionProps {
  label: ReactNode;
  hint?: ReactNode;
  selected?: boolean;
  shortcut?: string;
  icon?: ReactNode;
  disabled?: boolean;
  onPick: () => void;
}

export const Option = ({
  label,
  hint,
  selected,
  shortcut,
  icon,
  disabled,
  onPick,
}: OptionProps) => (
  <button
    aria-pressed={selected}
    className="text-body hover:bg-well focus-visible:bg-well active:bg-rim flex w-full shrink-0 cursor-pointer items-start gap-2 rounded-lg px-2 py-1.5 text-left focus-visible:outline-none disabled:cursor-default disabled:opacity-40"
    data-option=""
    disabled={disabled}
    onClick={onPick}
    type="button"
  >
    <span
      aria-hidden="true"
      className="text-ember flex h-[1.4em] w-3.5 shrink-0 items-center justify-center text-[12px]"
    >
      {selected ? "✓" : icon}
    </span>
    <span className="flex min-w-0 flex-1 flex-col">
      <span className={`truncate text-[13px] ${selected ? "text-paper" : ""}`}>
        {label}
      </span>
      {hint ? (
        <span className="text-fog truncate text-[11.5px] leading-snug">
          {hint}
        </span>
      ) : null}
    </span>
    {shortcut ? (
      <kbd className="text-fog shrink-0 pt-0.5 text-[11px]">{shortcut}</kbd>
    ) : null}
  </button>
);

export const Heading = ({ children }: { children: ReactNode }) => (
  <p className="text-fog shrink-0 px-2 pt-2 pb-1 text-[11px]">{children}</p>
);

export const Divider = () => <hr className="border-rim my-1 shrink-0" />;

interface SearchProps {
  value: string;
  placeholder: string;
  onChange: (value: string) => void;
}

export const Search = ({ value, placeholder, onChange }: SearchProps) => (
  <div className="border-rim mb-1 shrink-0 border-b px-2 pt-1 pb-2">
    <input
      aria-label={placeholder}
      className="text-paper placeholder:text-fog h-7 w-full bg-transparent text-[13px] outline-none"
      data-focus=""
      onChange={(event) => onChange(event.target.value)}
      placeholder={placeholder}
      spellCheck={false}
      value={value}
    />
  </div>
);
