import { Replay } from "@/components/replay";
import { Tag } from "@/components/tag";

export const History = () => (
  <section
    aria-labelledby="log"
    className="border-line scroll-mt-4 border-t"
    data-session="reply"
    id="history"
  >
    <div className="grid gap-6 px-5 py-16 sm:px-8 lg:grid-cols-12 lg:py-24">
      <div className="lg:col-span-5">
        <Tag>Session history</Tag>
      </div>
      <div className="lg:col-span-7 lg:pl-4">
        <h2
          className="text-ink max-w-[18ch] text-[clamp(2rem,3.6vw,3rem)] leading-[1.05] font-medium tracking-[-0.03em] text-balance"
          data-split=""
          id="log"
        >
          Every step, from either screen
        </h2>
        <p className="text-body mt-5 max-w-[46ch] text-[16px] leading-relaxed">
          One history per session. Scrub through it to replay what the agent did
          on your Mac and what you did from your phone.
        </p>
      </div>
    </div>
    <Replay />
  </section>
);
