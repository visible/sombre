import type { ReactNode } from "react";

import { Agents } from "@/components/art/agents";
import { Approve } from "@/components/art/approve";
import { Local } from "@/components/art/local";
import { Research } from "@/components/art/research";
import { Inview } from "@/components/inview";
import { Tag } from "@/components/tag";

interface Feature {
  art: ReactNode;
  body: string;
  cell: string;
  title: string;
}

const features: Feature[] = [
  {
    art: <Local />,
    body: "The agent runs locally. Your phone sends messages and receives updates, nothing more.",
    cell: "border-b lg:col-span-7 lg:border-r",
    title: "Your code stays on your Mac",
  },
  {
    art: <Approve />,
    body: "When the agent wants to run a command or edit a file, you decide from wherever you are.",
    cell: "border-b lg:col-span-5",
    title: "Approve changes in one tap",
  },
  {
    art: <Agents />,
    body: "Use Claude Code or Codex, the agents you already know, with the setup you already have.",
    cell: "border-b lg:col-span-5 lg:border-r lg:border-b-0",
    title: "Bring your own agent",
  },
  {
    art: <Research />,
    body: "Every search, file read, and step the agent takes streams to both screens as it happens.",
    cell: "lg:col-span-7",
    title: "One conversation, two screens",
  },
];

export const Features = () => (
  <section
    aria-labelledby="features"
    className="border-line scroll-mt-4 border-t"
    data-session="allow"
    id="why"
  >
    <div className="grid gap-6 px-5 py-16 sm:px-8 lg:grid-cols-12 lg:py-24">
      <div className="lg:col-span-5">
        <Tag>Why Sombre</Tag>
      </div>
      <h2
        className="text-ink max-w-[16ch] text-[clamp(2rem,3.6vw,3rem)] leading-[1.05] font-medium tracking-[-0.03em] text-balance lg:col-span-7 lg:pl-4"
        data-split=""
        id="features"
      >
        Built for the walk to lunch
      </h2>
    </div>
    <ul className="border-line grid border-t lg:grid-cols-12">
      {features.map((feature) => (
        <li
          className={`border-line flex flex-col ${feature.cell}`}
          key={feature.title}
        >
          <div className="px-5 pt-8 sm:px-8 sm:pt-10">
            <h3 className="text-ink text-[clamp(1.35rem,2.2vw,1.75rem)] leading-tight font-medium tracking-[-0.025em]">
              {feature.title}
            </h3>
            <p className="text-body mt-3 max-w-[40ch] text-[16px] leading-relaxed">
              {feature.body}
            </p>
          </div>
          <Inview className="relative mt-auto h-[280px] overflow-hidden sm:h-[300px]">
            {feature.art}
          </Inview>
        </li>
      ))}
    </ul>
  </section>
);
