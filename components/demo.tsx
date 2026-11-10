import { Handoff } from "@/components/handoff";
import { Thread } from "@/components/thread";
import { moment, schedule, thread } from "@/lib/timeline";

const entries = schedule(thread);
const handoff = moment(entries, "handoff");
const ms = (value: number) => `${value}ms`;

const rail = [
  { d: "M3 4.5h10M3 8h10M3 11.5h6", label: "Sessions", shortcut: "⌘1" },
  {
    d: "M8 4v4l2.5 1.5M14 8A6 6 0 1 1 2 8a6 6 0 0 1 12 0Z",
    label: "History",
    shortcut: "⌘2",
  },
  {
    d: "M7.5 12a4.5 4.5 0 1 0 0-9 4.5 4.5 0 0 0 0 9ZM13.5 13.5l-2.8-2.8",
    label: "Search",
    shortcut: "⌘K",
  },
  { d: "M4 8h8M8 4v8", label: "New session", shortcut: "⌘N" },
] as const;

export const Demo = () => (
  <figure
    aria-labelledby="demo"
    className="relative mx-auto lg:h-[600px] lg:w-[940px]"
  >
    <figcaption className="sr-only" id="demo">
      An example session. A developer asks the agent on their Mac to fix a flaky
      checkout test, then continues on their phone, approves a test run, and
      gets the fix confirmed.
    </figcaption>
    <div
      aria-hidden="true"
      className="bg-sheet flex h-[390px] overflow-hidden rounded-xl shadow-[0_40px_80px_-40px_rgb(90_25_0/0.55)] ring-1 ring-black/10 sm:h-[430px] lg:absolute lg:top-10 lg:left-0 lg:h-[470px] lg:w-[720px]"
    >
      <div className="border-line hidden w-12 shrink-0 flex-col items-center gap-1 border-r pt-12 sm:flex">
        {rail.map((item, index) => (
          <span
            className={`group/tip hover:bg-paper relative flex size-8 items-center justify-center rounded-lg transition-colors ${index === 0 ? "bg-paper text-ink" : "text-meta"}`}
            key={item.label}
          >
            <svg
              className="size-4"
              fill="none"
              stroke="currentColor"
              strokeLinecap="round"
              strokeLinejoin="round"
              strokeWidth="1.4"
              viewBox="0 0 16 16"
            >
              <path d={item.d} />
            </svg>
            <span className="bg-ink text-sheet pointer-events-none absolute top-1/2 left-[calc(100%+8px)] z-10 flex -translate-x-1 -translate-y-1/2 items-center gap-2 rounded-full px-3 py-1.5 text-[11px] font-medium whitespace-nowrap opacity-0 transition-[opacity,translate] duration-150 group-hover/tip:translate-x-0 group-hover/tip:opacity-100">
              {item.label}
              <span className="text-fog">{item.shortcut}</span>
            </span>
          </span>
        ))}
      </div>
      <div className="flex min-w-0 flex-1 flex-col">
        <div className="border-line flex items-center gap-3 border-b px-4 py-3">
          <div className="flex gap-1.5 sm:hidden">
            <span className="bg-line size-2.5 rounded-full" />
            <span className="bg-line size-2.5 rounded-full" />
            <span className="bg-line size-2.5 rounded-full" />
          </div>
          <p className="truncate text-[13px]">
            <span className="text-ink font-medium">atlas</span>
            <span className="text-meta ml-2 font-mono text-[11px]">
              ~/code/atlas
            </span>
          </p>
          <p
            className="appear text-ink ml-auto flex shrink-0 items-center gap-1.5 text-[11px]"
            style={{ animationDelay: ms(handoff + 500) }}
          >
            <span className="bg-ember size-1.5 rounded-full" />
            Phone connected
          </p>
        </div>
        <div className="flex min-h-0 flex-1 flex-col px-4 pb-32 sm:px-6 lg:pr-16 lg:pb-5">
          <Thread device="desktop" entries={entries} />
        </div>
      </div>
    </div>
    <div
      aria-hidden="true"
      data-phone=""
      className="rise relative -mt-28 mr-3 ml-auto flex h-[440px] w-[232px] flex-col rounded-[2.25rem] bg-white p-1.5 shadow-[inset_0_1px_0_white,0_50px_90px_-30px_rgb(60_15_0/0.55),0_10px_24px_-12px_rgb(60_15_0/0.3)] ring-1 ring-black/[0.08] sm:mr-8 lg:absolute lg:top-[110px] lg:left-[680px] lg:m-0 lg:h-[480px] lg:w-[256px]"
      style={{ animationDelay: ms(handoff - 1100) }}
    >
      <div className="bg-sheet flex min-h-0 flex-1 flex-col overflow-hidden rounded-[1.9rem] px-3.5 pt-3 pb-3 ring-1 ring-black/[0.05]">
        <div className="bg-line mx-auto h-1.5 w-16 rounded-full" />
        <div className="mt-3 flex items-center justify-between">
          <p className="text-ink text-[13px] font-medium">atlas</p>
          <p
            className="appear text-ink flex items-center gap-1.5 text-[10px]"
            style={{ animationDelay: ms(handoff + 500) }}
          >
            <span className="bg-ember size-1.5 rounded-full" />
            Live on your Mac
          </p>
        </div>
        <Thread device="phone" entries={entries} />
        <div className="border-line text-meta mt-3 rounded-full border px-3.5 py-2 text-[11px]">
          Reply to the agent
        </div>
      </div>
    </div>
    <Handoff start={handoff - 700} />
  </figure>
);
