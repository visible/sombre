import { Access } from "@/components/access";
import { Demo } from "@/components/demo";
import { Sunset } from "@/components/sunset";

export const Hero = () => (
  <section aria-labelledby="title">
    <div className="grid lg:grid-cols-12">
      <div className="border-line px-5 pt-12 sm:px-8 lg:col-span-5 lg:border-r lg:py-16">
        <p className="text-ink font-mono text-[12px] leading-[1.35] tracking-[0.02em] uppercase">
          For macOS.
          <br />
          Phone in any browser.
        </p>
      </div>
      <div className="px-5 pt-8 pb-14 sm:px-8 lg:col-span-7 lg:py-16 lg:pl-12">
        <h1
          className="text-ink text-[clamp(2.6rem,5.4vw,4.6rem)] leading-[1.02] font-medium tracking-[-0.035em] text-balance"
          id="title"
        >
          Step away. The session comes with you.
        </h1>
        <p className="text-body mt-6 max-w-[34rem] text-[17px] leading-relaxed">
          Sombre runs Claude Code and Codex on your Mac. Open the same
          conversation on your phone to answer questions, approve changes, and
          keep work moving while you&apos;re out.
        </p>
        <div className="mt-8 flex flex-wrap items-center gap-x-6 gap-y-4">
          <Access />
          <a
            className="text-ink group flex items-center gap-2 text-[15px] font-medium"
            href="#how"
          >
            <span className="decoration-ink/30 group-hover:decoration-ink underline underline-offset-4 transition-colors">
              See how it works
            </span>
            <span
              aria-hidden="true"
              className="bg-ink text-sheet flex size-5 items-center justify-center rounded-full transition-transform group-hover:translate-y-0.5"
            >
              <svg className="size-3" fill="none" viewBox="0 0 12 12">
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
    <div className="bg-ember relative overflow-hidden px-4 pt-12 pb-28 sm:px-8 sm:pt-16 sm:pb-36 lg:pt-20 lg:pb-44">
      <Sunset className="text-ink/30 pointer-events-none absolute bottom-0 left-1/2 w-[1500px] max-w-none -translate-x-1/2 sm:w-[1700px]" />
      <div className="relative mx-auto max-w-[1040px]">
        <Demo />
      </div>
    </div>
  </section>
);
