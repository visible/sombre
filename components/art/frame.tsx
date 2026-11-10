import type { ReactNode } from "react";

interface Props {
  children: ReactNode;
  className?: string;
  title?: ReactNode;
  name?: string;
}

export const Window = ({ children, className = "", title, name }: Props) => (
  <div
    className={`bg-sheet overflow-hidden rounded-2xl shadow-[0_30px_60px_-34px_rgb(15_16_17/0.35),0_8px_20px_-14px_rgb(15_16_17/0.18)] ring-1 ring-black/[0.08] ${className}`}
  >
    <div className="border-line relative flex h-10 items-center gap-1.5 border-b px-4">
      <span className="bg-line size-2 rounded-full" />
      <span className="bg-line size-2 rounded-full" />
      <span className="bg-line size-2 rounded-full" />
      {name ? (
        <span className="text-meta absolute left-1/2 -translate-x-1/2 font-mono text-[10px]">
          {name}
        </span>
      ) : null}
      <div className="ml-auto">{title}</div>
    </div>
    {children}
  </div>
);

export const Phone = ({ children, className = "" }: Props) => (
  <div
    className={`rounded-[34px] bg-white p-[5px] shadow-[inset_0_1px_0_white,0_30px_60px_-30px_rgb(15_16_17/0.35),0_8px_20px_-12px_rgb(15_16_17/0.18)] ring-1 ring-black/[0.08] ${className}`}
    data-phone=""
  >
    <span className="bg-line absolute top-24 -right-[3px] h-10 w-[3px] rounded-r-sm" />
    <div className="bg-sheet flex h-full flex-col overflow-hidden rounded-[29px] px-3.5 pt-2.5 ring-1 ring-black/[0.05]">
      <span className="bg-line mx-auto mb-3.5 block h-1 w-10 shrink-0 rounded-full" />
      {children}
    </div>
  </div>
);

export const Live = ({ children }: { children: ReactNode }) => (
  <span className="bg-paper text-ink flex items-center gap-1.5 rounded-full px-2 py-0.5 text-[10px] font-medium">
    <span className="bg-ember size-1.5 rounded-full" />
    {children}
  </span>
);
