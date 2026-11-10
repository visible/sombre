import type { ReactNode } from "react";

import { Tag } from "@/components/tag";

interface Props {
  tag: string;
  title: string;
  lead: string;
  children: ReactNode;
}

export const Shell = ({ tag, title, lead, children }: Props) => (
  <main>
    <section
      aria-labelledby="title"
      className="grid gap-6 px-5 pt-14 pb-16 sm:px-8 lg:grid-cols-12 lg:py-24"
    >
      <div className="lg:col-span-5">
        <Tag>{tag}</Tag>
      </div>
      <div className="lg:col-span-7 lg:pl-4">
        <h1
          className="text-ink max-w-[16ch] text-[clamp(2.4rem,4.8vw,4.2rem)] leading-[1.02] font-medium tracking-[-0.035em] text-balance"
          id="title"
        >
          {title}
        </h1>
        <p className="text-body mt-6 max-w-[48ch] text-[17px] leading-relaxed">
          {lead}
        </p>
      </div>
    </section>
    {children}
  </main>
);
