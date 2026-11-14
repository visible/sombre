import { Tag } from "@/components/tag";

const steps = [
  {
    body: "Open a project and pick Claude Code or Codex. The agent runs on your machine, with your files and your tools.",
    place: "At your desk",
    time: "9:41",
    title: "Start a session on your Mac",
  },
  {
    body: "Scan the code on screen or open your sombre.si link. It runs in the browser, so there's nothing to install.",
    place: "On your way out",
    time: "12:08",
    title: "Open it on your phone",
  },
  {
    body: "Reply, approve commands, and read diffs as they stream in. When you're back at your desk, it's all there.",
    place: "Wherever you are",
    time: "12:10",
    title: "Keep going from anywhere",
  },
] as const;

export const Steps = () => (
  <section
    aria-labelledby="steps"
    className="scroll-mt-4"
    data-session="handoff"
    id="how"
  >
    <div className="grid gap-6 px-5 py-16 sm:px-8 lg:grid-cols-12 lg:py-24">
      <div className="lg:col-span-5">
        <Tag>How it works</Tag>
      </div>
      <h2
        className="text-ink max-w-[18ch] text-[clamp(2rem,3.6vw,3rem)] leading-[1.05] font-medium tracking-[-0.03em] text-balance lg:col-span-7 lg:pl-4"
        data-split=""
        id="steps"
      >
        From your desk to your pocket in three steps
      </h2>
    </div>
    <ol>
      {steps.map((step) => (
        <li
          className="step relative grid gap-x-6 gap-y-4 px-5 py-9 sm:px-8 lg:grid-cols-12 lg:py-12"
          key={step.title}
        >
          <span
            aria-hidden="true"
            className="bg-line absolute inset-x-0 top-0 h-px"
            data-rule=""
          />
          <div className="lg:col-span-5" data-rise="">
            <p>
              <span className="text-ink block text-[clamp(2.6rem,6vw,5rem)] leading-[0.9] font-medium tracking-[-0.05em] tabular-nums">
                {step.time}
              </span>
              <span className="text-meta mt-3 flex items-center gap-2 font-mono text-[12px]">
                <span
                  aria-hidden="true"
                  className="step-dot bg-ember size-1.5 rounded-full"
                />
                {step.place}
              </span>
            </p>
          </div>
          <div className="lg:col-span-7 lg:self-end lg:pl-4" data-rise="">
            <h3 className="text-ink text-[clamp(1.6rem,2.9vw,2.5rem)] leading-[1.06] font-medium tracking-[-0.03em]">
              {step.title}
            </h3>
            <p className="text-body mt-4 max-w-[44ch] text-[16px] leading-relaxed">
              {step.body}
            </p>
          </div>
        </li>
      ))}
    </ol>
  </section>
);
