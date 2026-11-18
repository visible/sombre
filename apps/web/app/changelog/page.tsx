import type { Metadata } from "next";

import { Shell } from "@/components/shell";
import { format, notes, releases } from "@/lib/changelog";
import { page } from "@/lib/meta";

export const metadata: Metadata = page(
  "/changelog",
  "Changelog",
  "Every Sombre release, newest first: what's new and what's fixed in the CLI, the desktop app, and on your phone."
);

interface GroupProps {
  title: string;
  items: string[];
  dot: string;
}

const Group = ({ title, items, dot }: GroupProps) => (
  <section className="mt-8">
    <h3 className="text-meta font-mono text-[11px] tracking-[0.06em] uppercase">
      {title}
    </h3>
    <ul className="mt-3 space-y-2">
      {items.map((item) => (
        <li
          className="text-body flex gap-3 text-[15px] leading-relaxed"
          key={item}
        >
          <span
            aria-hidden="true"
            className={`${dot} mt-[9px] size-[6px] shrink-0 rounded-[1.5px]`}
          />
          {item}
        </li>
      ))}
    </ul>
  </section>
);

const [latest] = releases;

const Changelog = () => (
  <Shell
    facts={[
      { label: "Latest", value: latest?.version ?? "" },
      { label: "Released", value: format(latest?.date ?? "") },
    ]}
    lead="Every Sombre release, newest first. The CLI and the desktop app update themselves, so you always have the latest."
    tag="Changelog"
    title="What changed"
  >
    <ol className="border-line divide-line divide-y border-t">
      {releases.map((release) => (
        <li
          className="grid scroll-mt-20 gap-4 px-5 py-10 sm:px-8 lg:grid-cols-12 lg:py-14"
          id={`v${release.version}`}
          key={release.version}
        >
          <div className="flex flex-col items-start gap-1 lg:col-span-5">
            <p
              aria-hidden="true"
              className="text-ink font-mono text-[15px] font-medium"
            >
              {release.version}
            </p>
            <time
              className="text-meta font-mono text-[12px]"
              dateTime={release.date}
            >
              {format(release.date)}
            </time>
            <a
              className="text-ink mt-3 inline-block text-[13px]"
              href={notes(release.version)}
            >
              <span className="draw at-rest inline-block py-0.5">
                Release notes and downloads
              </span>
            </a>
          </div>
          <div className="lg:col-span-7 lg:pl-4">
            <h2 className="text-ink text-[clamp(1.35rem,2.2vw,1.75rem)] leading-tight font-medium tracking-[-0.025em]">
              <span className="sr-only">{`Version ${release.version}: `}</span>
              {release.title}
            </h2>
            <Group dot="bg-ember" items={release.added} title="New" />
            {release.fixed.length > 0 ? (
              <Group dot="bg-fog" items={release.fixed} title="Fixed" />
            ) : null}
          </div>
        </li>
      ))}
    </ol>
  </Shell>
);

export default Changelog;
