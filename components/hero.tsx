import { Access } from "@/components/access";
import { Demo } from "@/components/demo";
import { Horizon } from "@/components/horizon";

export const Hero = () => (
  <section aria-labelledby="title" data-desk="">
    <div className="@container px-5 pt-8 pb-12 sm:px-8 lg:pt-6 lg:pb-12">
      <p className="text-ink font-mono text-[12px] leading-[1.35] tracking-[0.02em] uppercase lg:hidden">
        For macOS.
        <br />
        Phone in any browser.
      </p>
      <div className="mt-6 grid gap-x-6 lg:mt-0 lg:grid-cols-12">
        <h1
          className="text-ink grid font-medium lg:col-span-12 lg:col-start-1 lg:row-span-2 lg:row-start-1 lg:grid-cols-subgrid lg:grid-rows-subgrid"
          id="title"
        >
          <span className="mask -ml-[0.05em] block text-[clamp(4rem,20.6cqi,17rem)] leading-none tracking-[-0.06em] whitespace-nowrap lg:col-span-12">
            <span className="up block">Step away.</span>
          </span>{" "}
          <span className="mask mt-1 block text-[clamp(1.9rem,4.5cqi,3.8rem)] leading-[1.04] tracking-[-0.035em] text-balance lg:col-span-7 lg:mt-9">
            <span className="up block [animation-delay:90ms]">
              The session comes with you.
            </span>
          </span>
        </h1>
        <p className="text-ink hidden font-mono text-[12px] leading-[1.35] tracking-[0.02em] uppercase lg:col-span-7 lg:col-start-1 lg:row-start-3 lg:mt-5 lg:block">
          For macOS. Phone in any browser.
        </p>
        <div className="relative mt-6 lg:col-span-5 lg:col-start-8 lg:row-span-2 lg:row-start-2 lg:mt-12 lg:self-start">
          <p className="text-body max-w-[34rem] text-[17px] leading-relaxed">
            Sombre runs Claude Code and Codex on your Mac. Open the same
            conversation on your phone to answer questions, approve changes, and
            keep work moving while you&apos;re out.
          </p>
          <div className="mt-7 flex flex-wrap items-center gap-x-6 gap-y-4">
            <Access />
            <a
              className="text-ink group flex items-center gap-2 text-[15px] font-medium"
              href="#how"
            >
              <span className="draw at-rest">See how it works</span>
              <span
                aria-hidden="true"
                className="disc bg-ink text-sheet flex size-5 items-center justify-center overflow-clip rounded-full"
              >
                <svg
                  className="nudge-down size-3"
                  fill="none"
                  viewBox="0 0 12 12"
                >
                  <path
                    d="M6 2.5v7M3 6.5l3 3 3-3"
                    stroke="currentColor"
                    strokeLinecap="round"
                    strokeLinejoin="round"
                    strokeWidth="1.5"
                  />
                </svg>
              </span>
            </a>
          </div>
        </div>
      </div>
    </div>
    <div className="bg-ember relative overflow-hidden px-4 pt-12 pb-28 sm:px-8 sm:pt-14 sm:pb-36 lg:pt-8 lg:pb-44">
      <Horizon tone="ember" />
      <div className="relative mx-auto max-w-[1040px]">
        <Demo />
      </div>
    </div>
  </section>
);
