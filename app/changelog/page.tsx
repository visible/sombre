import type { Metadata } from "next";

import { Shell } from "@/components/shell";
import { format, releases } from "@/lib/changelog";

export const metadata: Metadata = {
  description:
    "What's new in Sombre, newest first: every change to the app and this site as we build toward the first release.",
  title: "Changelog",
};

const Changelog = () => (
  <Shell
    facts={[
      { label: "Latest", value: format(releases[0]?.date ?? "") },
      { label: "Releases", value: String(releases.length) },
    ]}
    lead="What's new in Sombre, newest first. Small, frequent updates while we build toward the first release."
    tag="Changelog"
    title="What changed"
  >
    <ol className="border-line divide-line divide-y border-t">
      {releases.map((release) => (
        <li
          className="grid gap-4 px-5 py-10 sm:px-8 lg:grid-cols-12 lg:py-14"
          key={release.date}
        >
          <time
            className="text-meta font-mono text-[12px] lg:col-span-5"
            dateTime={release.date}
          >
            {format(release.date)}
          </time>
          <div className="lg:col-span-7 lg:pl-4">
            <h2 className="text-ink text-[clamp(1.35rem,2.2vw,1.75rem)] leading-tight font-medium tracking-[-0.025em]">
              {release.title}
            </h2>
            <ul className="mt-4 space-y-2">
              {release.items.map((item) => (
                <li
                  className="text-body flex gap-3 text-[15px] leading-relaxed"
                  key={item}
                >
                  <span
                    aria-hidden="true"
                    className="bg-ember mt-[9px] size-[6px] shrink-0 rounded-[1.5px]"
                  />
                  {item}
                </li>
              ))}
            </ul>
          </div>
        </li>
      ))}
    </ol>
  </Shell>
);

export default Changelog;
