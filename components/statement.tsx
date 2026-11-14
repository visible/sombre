import Link from "next/link";

import { Tag } from "@/components/tag";

export const Statement = () => (
  <section
    aria-labelledby="stays"
    className="border-line grid gap-6 border-t px-5 py-20 sm:px-8 lg:grid-cols-12 lg:py-32"
    data-session="edit"
  >
    <div className="lg:col-span-5">
      <Tag>What travels</Tag>
    </div>
    <div className="lg:col-span-7 lg:pl-4">
      <h2
        className="text-ink max-w-[22ch] text-[clamp(1.75rem,3.3vw,2.75rem)] leading-[1.12] font-medium tracking-[-0.03em]"
        data-split=""
        id="stays"
      >
        The agent, the files and the tools never leave your Mac. Only the
        conversation goes out the door.
      </h2>
      <Link
        className="text-ink group mt-8 flex w-fit items-center gap-2 text-[15px] font-medium"
        href="/security"
      >
        <span className="draw at-rest">How Sombre handles security</span>
        <span
          aria-hidden="true"
          className="disc bg-ink text-sheet flex size-5 items-center justify-center overflow-clip rounded-full"
        >
          <svg className="nudge-right size-3" fill="none" viewBox="0 0 12 12">
            <path
              d="M2.5 6h7M6.5 3l3 3-3 3"
              stroke="currentColor"
              strokeLinecap="round"
              strokeLinejoin="round"
              strokeWidth="1.5"
            />
          </svg>
        </span>
      </Link>
    </div>
  </section>
);
