"use client";

import { usePathname } from "next/navigation";
import type { RefObject } from "react";
import { useEffect, useRef, useSyncExternalStore } from "react";

import { still } from "@/lib/motion";
import { session, view } from "@/lib/session";
import type { View } from "@/lib/session";

const glyphs = {
  mac: "M2.5 4.5a1 1 0 0 1 1-1h9a1 1 0 0 1 1 1v6h-11Zm-1 6h13",
  phone:
    "M5.5 1.5h5a1 1 0 0 1 1 1v11a1 1 0 0 1-1 1h-5a1 1 0 0 1-1-1v-11a1 1 0 0 1 1-1Zm2 11h1",
} as const;

const check = (
  <svg aria-hidden="true" className="size-3" fill="none" viewBox="0 0 12 12">
    <path
      d="m3 6.5 2 2 4-5"
      stroke="currentColor"
      strokeLinecap="round"
      strokeLinejoin="round"
      strokeWidth="1.5"
    />
  </svg>
);

const aside = (current: View) => {
  if (current.replay) {
    return "Replay";
  }
  return current.busy ? "" : current.meta;
};

const Chip = ({ current }: { current: View }) => {
  if (current.chip === "approve") {
    return (
      <button
        className="pill bg-ember text-ink flex h-8 shrink-0 cursor-pointer items-center rounded-full px-3.5 text-[12px] font-medium [--flood:var(--color-sheet)] focus-visible:outline-white"
        aria-label="Approve"
        onClick={() => session.approve()}
        type="button"
      >
        <span className="roll">
          <span>Approve</span>
          <span aria-hidden="true">Approve</span>
        </span>
      </button>
    );
  }
  if (current.chip === "approved") {
    return (
      <span className="pocket-line text-ember flex h-8 shrink-0 items-center gap-1 text-[12px] font-medium">
        {check}
        Approved
      </span>
    );
  }
  return null;
};

const displayed = (selector: string) =>
  [...document.querySelectorAll<HTMLElement>(selector)].filter(
    (node) => node.getClientRects().length > 0
  );

interface WatchProps {
  root: RefObject<HTMLElement | null>;
  dot: RefObject<HTMLSpanElement | null>;
}

const Watch = ({ root, dot }: WatchProps) => {
  useEffect(() => {
    const node = root.current;
    if (!node) {
      return;
    }
    let away = false;
    let landed = false;
    const pulse =
      still() || !dot.current
        ? null
        : dot.current.animate(
            [
              { opacity: 1, transform: "scale(1)" },
              { opacity: 0, transform: "scale(2.6)" },
            ],
            {
              direction: "alternate",
              duration: 1300,
              easing: "cubic-bezier(0.37, 0, 0.63, 1)",
              iterations: Number.POSITIVE_INFINITY,
            }
          );
    pulse?.pause();
    const sync = () => {
      const shown = away && !landed;
      node.toggleAttribute("data-shown", shown);
      node.inert = !shown;
      if (shown) {
        pulse?.play();
      } else {
        pulse?.pause();
      }
    };
    let top = -1;
    let size = -1;
    const tick = () => {
      const y = window.scrollY;
      const height = document.documentElement.scrollHeight;
      if (y === top && height === size) {
        return;
      }
      top = y;
      size = height;
      const tall = window.innerHeight;
      const [desk] = displayed("[data-desk]");
      const [ground] = displayed("[data-ground]");
      away = desk
        ? desk.getBoundingClientRect().bottom < tall * 0.62
        : displayed("[data-pocket]").length > 0;
      landed = ground ? ground.getBoundingClientRect().top < tall * 0.9 : false;
      for (const section of displayed("[data-session]")) {
        if (section.getBoundingClientRect().top < tall * 0.58) {
          session.advance(section.dataset.session ?? "");
        }
      }
      sync();
    };
    const grown = new ResizeObserver(tick);
    grown.observe(document.body);
    window.addEventListener("scroll", tick, { passive: true });
    window.addEventListener("resize", tick);
    tick();
    return () => {
      grown.disconnect();
      window.removeEventListener("scroll", tick);
      window.removeEventListener("resize", tick);
      pulse?.cancel();
    };
  }, [root, dot]);
  return null;
};

export const Pocket = () => {
  const pathname = usePathname();
  const state = useSyncExternalStore(
    session.subscribe,
    session.read,
    session.read
  );
  const root = useRef<HTMLElement>(null);
  const dot = useRef<HTMLSpanElement>(null);
  const current = view(state);
  const note = aside(current);

  useEffect(() => {
    if (!state.running) {
      return;
    }
    const call = window.setTimeout(session.finish, 1700);
    return () => {
      window.clearTimeout(call);
    };
  }, [state.running]);

  return (
    <>
      <Watch dot={dot} key={pathname} root={root} />
      <aside aria-label="Example session" className="pocket" inert ref={root}>
        <div className="bg-ink text-sheet flex h-12 items-center gap-3 rounded-full pr-2 pl-4 shadow-[0_18px_40px_-14px_rgb(15_16_17/0.55),0_4px_10px_-4px_rgb(15_16_17/0.3)] ring-1 ring-white/10">
          <span aria-hidden="true" className="relative flex size-2 shrink-0">
            <span
              className="bg-ember absolute inset-0 rounded-full"
              ref={dot}
            />
            <span className="bg-ember relative size-2 rounded-full" />
          </span>
          <span className="text-fog flex shrink-0 items-center gap-1.5 font-mono text-[11px] tabular-nums">
            <svg
              aria-hidden="true"
              className="size-3.5"
              fill="none"
              stroke="currentColor"
              strokeLinecap="round"
              strokeLinejoin="round"
              strokeWidth="1.3"
              viewBox="0 0 16 16"
            >
              <path d={glyphs[current.device]} />
            </svg>
            {current.time}
          </span>
          <div
            aria-live={current.replay ? "off" : "polite"}
            className="min-w-0 flex-1 overflow-clip text-[13px] font-medium"
          >
            <span
              className={`pocket-line block truncate ${current.busy ? "pocket-busy" : ""}`}
              key={current.title}
            >
              {current.title}
            </span>
          </div>
          {note ? (
            <span
              className="pocket-line text-fog shrink-0 font-mono text-[11px] max-[420px]:hidden"
              key={note}
            >
              {note}
            </span>
          ) : null}
          <Chip current={current} />
          {current.chip ? null : <span aria-hidden="true" className="w-2" />}
        </div>
      </aside>
    </>
  );
};
