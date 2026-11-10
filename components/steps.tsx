import { Tag } from "@/components/tag";

const steps = [
  {
    body: "Open a project and pick Claude Code or Codex. The agent runs on your machine, with your files and your tools.",
    title: "Start a session on your Mac",
  },
  {
    body: "Scan the code on screen or open your sombre.si link. It runs in the browser, so there's nothing to install.",
    title: "Open it on your phone",
  },
  {
    body: "Reply, approve commands, and read diffs as they stream in. When you're back at your desk, it's all there.",
    title: "Keep going from anywhere",
  },
] as const;

export const Steps = () => (
  <section aria-labelledby="steps" className="scroll-mt-4" id="how">
    <div className="grid gap-6 px-5 py-16 sm:px-8 lg:grid-cols-12 lg:py-24">
      <div className="lg:col-span-5">
        <Tag>How it works</Tag>
      </div>
      <h2
        className="text-ink max-w-[18ch] text-[clamp(2rem,3.6vw,3rem)] leading-[1.05] font-medium tracking-[-0.03em] text-balance lg:col-span-7 lg:pl-4"
        id="steps"
      >
        From your desk to your pocket in three steps
      </h2>
    </div>
    <ol className="border-line divide-line grid divide-y border-t md:grid-cols-3 md:divide-x md:divide-y-0">
      {steps.map((step, index) => (
        <li
          className="flex flex-col justify-between gap-6 px-5 py-8 sm:px-8 md:min-h-[240px] md:gap-10"
          key={step.title}
        >
          <span aria-hidden="true" className="text-meta font-mono text-[12px]">
            {String(index + 1).padStart(2, "0")}
          </span>
          <div>
            <h3 className="text-ink text-[19px] font-medium tracking-[-0.01em]">
              {step.title}
            </h3>
            <p className="text-body mt-2 max-w-[36ch] text-[15px] leading-relaxed">
              {step.body}
            </p>
          </div>
        </li>
      ))}
    </ol>
  </section>
);
