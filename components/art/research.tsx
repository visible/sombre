import type { CSSProperties, ReactNode } from "react";

import { Live, Phone, Window } from "@/components/art/frame";
import { Chip } from "@/components/art/terminal";

interface Props {
  small?: boolean;
}

const sizes = {
  regular: "text-[11.5px] leading-[22px]",
  small: "text-[10px] leading-[18px]",
} as const;

const Step = ({
  at,
  children,
  icon,
  last = false,
}: {
  at: number;
  children: ReactNode;
  icon: ReactNode;
  last?: boolean;
}) => (
  <div
    className="relative grid grid-cols-[14px_1fr] gap-x-2"
    data-line=""
    style={{ "--i": at } as CSSProperties}
  >
    {last ? null : (
      <span className="bg-line absolute top-[18px] -bottom-[4px] left-[6.5px] w-px" />
    )}
    <span className="text-meta flex h-[1lh] items-center justify-center">
      {icon}
    </span>
    <div className="min-w-0 pb-1">{children}</div>
  </div>
);

const search = (
  <svg
    className="size-3"
    fill="none"
    stroke="currentColor"
    strokeLinecap="round"
    strokeWidth="1.3"
    viewBox="0 0 12 12"
  >
    <path d="M5.5 9a3.5 3.5 0 1 0 0-7 3.5 3.5 0 0 0 0 7ZM10 10 8 8" />
  </svg>
);

const dot = <span className="bg-meta/60 size-[5px] rounded-full" />;

const spin = (
  <span className="border-meta/40 border-t-ink size-[9px] rounded-full border-[1.5px]" />
);

const Trace = ({ small = false }: Props) => (
  <div className={`font-sans ${small ? sizes.small : sizes.regular}`}>
    <p
      className="text-ink mb-1.5 flex items-center gap-2 font-medium"
      data-line=""
      style={{ "--i": 0 } as CSSProperties}
    >
      Thinking
      <span className="text-meta font-mono font-normal">4s</span>
    </p>
    <Step at={1} icon={search}>
      <p className="text-body truncate">
        {small ? "Searched code" : "Searched the codebase"}
      </p>
      <p className="mt-1 flex items-center gap-1">
        <Chip>
          <span className="text-meta font-mono text-[8px]">TS</span>
          checkout.spec.ts
        </Chip>
        {small ? null : (
          <Chip>
            <span className="text-meta font-mono text-[8px]">TS</span>
            payment.ts
          </Chip>
        )}
        <span className="text-meta px-0.5">{small ? "+4" : "+3 more"}</span>
      </p>
    </Step>
    <Step at={2} icon={dot}>
      <p className="text-body truncate">
        {small ? "Read 7 files" : "Read 7 files about checkout"}
      </p>
    </Step>
    <Step at={3} icon={spin} last>
      <p className="text-meta truncate" data-shimmer="">
        {small ? "Checking the frame" : "Checking how the payment frame loads"}
      </p>
    </Step>
  </div>
);

export const Research = () => (
  <>
    <Window
      className="absolute top-10 left-[calc(50%-245px)] w-[310px] max-sm:left-4 max-sm:w-[250px]"
      name="atlas"
      title={<Live>Mac</Live>}
    >
      <div className="px-4 pt-3.5 pb-10">
        <Trace />
      </div>
    </Window>
    <Phone className="absolute top-5 left-[calc(50%+78px)] h-[310px] w-[156px] max-sm:right-3 max-sm:left-auto">
      <div className="mb-3 flex items-center justify-between">
        <p className="text-ink text-[11px] font-medium">atlas</p>
        <span className="bg-ember size-1.5 rounded-full" />
      </div>
      <Trace small />
    </Phone>
  </>
);
