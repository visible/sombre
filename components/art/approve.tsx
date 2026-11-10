import { Live, Phone } from "@/components/art/frame";
import { Lines } from "@/components/art/terminal";
import type { Line } from "@/components/art/terminal";

const before: Line[] = [
  { aside: "+2 -1", text: "Edited checkout.spec.ts", tone: "done" },
  { text: "Ready to run the suite", tone: "output" },
];

export const Approve = () => (
  <Phone className="absolute top-6 left-1/2 h-[330px] w-[236px] -translate-x-1/2">
    <div className="mb-4 flex items-center justify-between">
      <p className="text-ink text-[12px] font-medium">atlas</p>
      <Live>Live on your Mac</Live>
    </div>
    <Lines lines={before} size="small" />
    <div className="border-line mt-3 rounded-xl border bg-white p-3 shadow-[0_10px_24px_-18px_rgb(15_16_17/0.4)]">
      <p className="text-meta text-[10px]">Wants to run a command</p>
      <p className="bg-paper text-ink mt-1.5 rounded-md px-2 py-1.5 font-mono text-[11px]">
        $ bun test
      </p>
      <div className="mt-3 grid grid-cols-2 gap-1.5">
        <span className="bg-paper text-body flex h-8 items-center justify-center rounded-full text-[11px] font-medium">
          Deny
        </span>
        <span className="relative flex h-8 items-center justify-center">
          <span
            className="bg-ink text-sheet absolute inset-0 flex items-center justify-center rounded-full text-[11px] font-medium opacity-0"
            data-before=""
          >
            Approve
          </span>
          <span
            className="bg-ember text-ink absolute inset-0 flex items-center justify-center gap-1 rounded-full text-[11px] font-medium"
            data-after=""
          >
            <svg className="size-3" fill="none" viewBox="0 0 12 12">
              <path
                d="m3 6.5 2 2 4-5"
                stroke="currentColor"
                strokeLinecap="round"
                strokeLinejoin="round"
                strokeWidth="1.5"
              />
            </svg>
            Approved
          </span>
          <span
            className="bg-ink/25 absolute size-10 rounded-full opacity-0"
            data-tap=""
          />
        </span>
      </div>
    </div>
  </Phone>
);
