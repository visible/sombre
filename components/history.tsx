"use client";

import { useLayoutEffect, useRef, useState } from "react";
import type { KeyboardEvent } from "react";

import { Live, Window } from "@/components/art/frame";
import { Lines } from "@/components/art/terminal";
import { Mobile } from "@/components/mobile";
import { Tag } from "@/components/tag";
import { devices, history } from "@/lib/history";
import type { Device, Screen } from "@/lib/history";

const start = history.findIndex((event) => event.id === "handoff");

const glyphs: Record<Device, string> = {
  mac: "M2.5 4.5a1 1 0 0 1 1-1h9a1 1 0 0 1 1 1v6h-11Zm-1 6h13",
  phone:
    "M5.5 1.5h5a1 1 0 0 1 1 1v11a1 1 0 0 1-1 1h-5a1 1 0 0 1-1-1v-11a1 1 0 0 1 1-1Zm2 11h1",
};

const dot = (current: boolean, device: Device) => {
  if (current) {
    return "border-ember bg-ember";
  }
  return device === "phone" ? "border-ember bg-paper" : "border-meta bg-paper";
};

const state = (index: number, active: number) =>
  index > active ? "opacity-0" : "opacity-100";

const Glyph = ({ device }: { device: Device }) => (
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
    <path d={glyphs[device]} />
  </svg>
);

const last = (index: number): Screen | undefined =>
  history
    .slice(0, index + 1)
    .toReversed()
    .find((event) => event.phone)?.phone;

export const History = () => {
  const [active, setActive] = useState(start);
  const [offset, setOffset] = useState(0);
  const buttons = useRef<(HTMLButtonElement | null)[]>([]);
  const blocks = useRef<(HTMLDivElement | null)[]>([]);
  const view = useRef<HTMLDivElement>(null);
  const event = history[active] ?? history[0];
  const screen = last(active) ?? history[start]?.phone;
  const phone = event?.device === "phone";

  useLayoutEffect(() => {
    const measure = () => {
      const block = blocks.current[active];
      const room = view.current?.clientHeight ?? 0;
      if (block) {
        setOffset(
          Math.max(0, block.offsetTop + block.offsetHeight - room * 0.62)
        );
      }
    };
    measure();
    const sizes = new ResizeObserver(measure);
    if (view.current) {
      sizes.observe(view.current);
    }
    return () => sizes.disconnect();
  }, [active]);

  const move = (key: KeyboardEvent<HTMLButtonElement>, index: number) => {
    const step = { ArrowDown: 1, ArrowUp: -1 }[key.key];
    if (!step) {
      return;
    }
    key.preventDefault();
    const next = (index + step + history.length) % history.length;
    buttons.current[next]?.focus();
  };

  if (!event) {
    return null;
  }

  return (
    <section
      aria-labelledby="log"
      className="border-line scroll-mt-4 border-t"
      id="history"
    >
      <div className="grid gap-6 px-5 py-16 sm:px-8 lg:grid-cols-12 lg:py-24">
        <div className="lg:col-span-5">
          <Tag>Session history</Tag>
        </div>
        <div className="lg:col-span-7 lg:pl-4">
          <h2
            className="text-ink max-w-[18ch] text-[clamp(2rem,3.6vw,3rem)] leading-[1.05] font-medium tracking-[-0.03em] text-balance"
            id="log"
          >
            Every step, from either screen
          </h2>
          <p className="text-body mt-5 max-w-[46ch] text-[15px] leading-relaxed">
            One history per session. Scrub through it to replay what the agent
            did on your Mac and what you did from your phone.
          </p>
        </div>
      </div>
      <div className="border-line grid border-t lg:grid-cols-12">
        <ol className="lg:border-line relative min-w-0 px-5 py-10 sm:px-8 lg:col-span-5 lg:border-r">
          <span
            aria-hidden="true"
            className="bg-line absolute top-[52px] bottom-[52px] left-[27px] w-px sm:left-[39px]"
          />
          {history.map((item, index) => {
            const current = index === active;
            return (
              <li key={item.id}>
                <button
                  aria-pressed={current}
                  className="group relative flex w-full items-center gap-4 rounded-lg py-2.5 pr-3 text-left"
                  onFocus={() => setActive(index)}
                  onKeyDown={(key) => move(key, index)}
                  onMouseEnter={() => setActive(index)}
                  ref={(node) => {
                    buttons.current[index] = node;
                  }}
                  type="button"
                >
                  <span
                    aria-hidden="true"
                    className={`relative z-10 ml-[1px] size-[11px] shrink-0 rounded-full border-2 transition-colors ${dot(current, item.device)}`}
                  />
                  <span
                    className={`min-w-0 flex-1 truncate text-[15px] transition-colors ${current ? "text-ink" : "text-body group-hover:text-ink"}`}
                  >
                    {item.title}
                  </span>
                  <span className="text-meta flex shrink-0 items-center gap-1.5 font-mono text-[11px]">
                    <Glyph device={item.device} />
                    {item.time}
                  </span>
                </button>
              </li>
            );
          })}
        </ol>
        <div className="border-line relative h-[440px] min-w-0 overflow-hidden border-t lg:col-span-7 lg:h-auto lg:min-h-[480px] lg:border-t-0">
          <p aria-live="polite" className="sr-only">
            {event.title}. {devices[event.device]} at {event.time}.{" "}
            {event.detail} {event.meta ?? ""}
          </p>
          <div aria-hidden="true" className="absolute inset-0" data-stage="">
            <Window
              className="absolute top-10 right-5 -bottom-10 left-5 flex flex-col sm:right-8 sm:left-8 lg:right-10 lg:left-12"
              name="atlas"
              title={
                <span className="flex items-center gap-2">
                  <span className="text-meta font-mono text-[10px]">
                    {event.time}
                  </span>
                  <Live>{phone ? "Phone" : "Mac"}</Live>
                </span>
              }
            >
              <div
                className="relative min-h-0 flex-1 overflow-hidden [mask-image:linear-gradient(to_bottom,transparent_8px,black_120px)]"
                ref={view}
              >
                <div
                  className="space-y-4 px-5 pt-6 pr-6 pb-40 transition-[translate] duration-500 ease-[cubic-bezier(0.3,0.7,0.2,1)] motion-reduce:transition-none"
                  style={{ translate: `0 ${-offset}px` }}
                >
                  {history.map((item, index) => (
                    <div
                      className={`relative transition-opacity duration-300 motion-reduce:transition-none ${state(index, active)}`}
                      data-block=""
                      key={item.id}
                      ref={(node) => {
                        blocks.current[index] = node;
                      }}
                    >
                      <span
                        className={`bg-ember absolute top-1 bottom-1 -left-3 w-[2px] rounded-full transition-opacity ${index === active ? "opacity-100" : "opacity-0"}`}
                      />
                      <p className="text-meta mb-0.5 flex items-center gap-1.5 font-mono text-[10px]">
                        <Glyph device={item.device} />
                        {item.time}
                      </p>
                      <Lines lines={item.rows} />
                    </div>
                  ))}
                </div>
              </div>
            </Window>
            {screen ? (
              <Mobile
                className={`absolute right-10 -bottom-14 h-[330px] w-[214px] transition-[translate,opacity] duration-500 ease-[cubic-bezier(0.3,0.7,0.2,1)] motion-reduce:transition-none sm:right-16 lg:right-20 ${phone ? "translate-y-0 opacity-100" : "translate-y-16 opacity-0"}`}
                screen={screen}
              />
            ) : null}
          </div>
        </div>
      </div>
    </section>
  );
};
