import type { ReactNode } from "react";

import { Live, Phone } from "@/components/art/frame";
import type { Screen } from "@/lib/history";

const check = (
  <svg className="size-3" fill="none" viewBox="0 0 12 12">
    <path
      d="m3 6.5 2 2 4-5"
      stroke="currentColor"
      strokeLinecap="round"
      strokeLinejoin="round"
      strokeWidth="1.5"
    />
  </svg>
);

const card =
  "border-line rounded-xl border bg-white p-3 shadow-[0_10px_24px_-18px_rgb(15_16_17/0.4)]";

const Body = ({ screen }: { screen: Screen }): ReactNode => {
  if (screen.kind === "status") {
    return (
      <div className={`${card} text-center`}>
        <span className="bg-ember text-ink mx-auto flex size-8 items-center justify-center rounded-full">
          {check}
        </span>
        <p className="text-ink mt-2.5 text-[12px] font-medium">
          Session continued
        </p>
        <p className="text-body mt-1 font-mono text-[10px]">{screen.text}</p>
      </div>
    );
  }
  if (screen.kind === "message") {
    return (
      <>
        <p className="text-body text-[11px] leading-snug">
          Waits a fixed 500ms for the payment frame. Wait for its ready event
          instead?
        </p>
        <p className="bg-paper text-ink mt-3 ml-auto w-fit max-w-[85%] rounded-2xl rounded-br-md px-3 py-2 text-[12px] leading-snug">
          {screen.text}
        </p>
      </>
    );
  }
  return (
    <div className={card}>
      <p className="text-body text-[10px]">Wants to run a command</p>
      <p className="bg-paper text-ink mt-1.5 rounded-md px-2 py-1.5 font-mono text-[11px]">
        $ {screen.command}
      </p>
      <div className="mt-3 grid grid-cols-2 gap-1.5">
        <span className="bg-paper text-body flex h-8 items-center justify-center rounded-full text-[11px] font-medium">
          Deny
        </span>
        <span className="bg-ember text-ink flex h-8 items-center justify-center gap-1 rounded-full text-[11px] font-medium">
          {check}
          Approved
        </span>
      </div>
    </div>
  );
};

export const Mobile = ({
  screen,
  className = "",
}: {
  screen: Screen;
  className?: string;
}) => (
  <Phone className={className}>
    <div className="mb-5 flex items-center justify-between">
      <p className="text-ink text-[12px] font-medium">atlas</p>
      <Live>Live on your Mac</Live>
    </div>
    <Body screen={screen} />
  </Phone>
);
