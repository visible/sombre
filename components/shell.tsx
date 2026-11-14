import type { ReactNode } from "react";

import { Tag } from "@/components/tag";

interface Fact {
  label: string;
  value: string;
}

interface Props {
  facts?: readonly Fact[];
  tag: string;
  title: string;
  lead: string;
  children: ReactNode;
}

export const Shell = ({ facts, tag, title, lead, children }: Props) => (
  <main id="main" tabIndex={-1}>
    <section
      aria-labelledby="title"
      className="grid gap-6 px-5 pt-14 pb-16 sm:px-8 lg:grid-cols-12 lg:py-24"
    >
      <div className="flex flex-col justify-between gap-10 lg:col-span-5">
        <Tag>{tag}</Tag>
        {facts ? (
          <dl className="order-last hidden max-w-[19rem] font-mono text-[12px] leading-[1.5] lg:block">
            {facts.map((fact) => (
              <div
                className="border-line flex justify-between gap-6 border-t py-2.5"
                key={fact.label}
              >
                <dt className="text-meta">{fact.label}</dt>
                <dd className="text-ink text-right">{fact.value}</dd>
              </div>
            ))}
          </dl>
        ) : null}
      </div>
      <div className="lg:col-span-7 lg:pl-4">
        <h1
          className="text-ink max-w-[14ch] text-[clamp(2.8rem,6.4vw,5.6rem)] leading-[0.98] font-medium tracking-[-0.045em] text-balance"
          id="title"
        >
          <span className="mask -ml-[0.04em] block">
            <span className="up block">{title}</span>
          </span>
        </h1>
        <p className="text-body mt-6 max-w-[48ch] text-[17px] leading-relaxed">
          {lead}
        </p>
      </div>
    </section>
    {children}
  </main>
);
