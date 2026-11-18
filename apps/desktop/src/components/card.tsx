import type { ReactNode } from "react";

interface RowProps {
  title: ReactNode;
  about?: ReactNode;
  children?: ReactNode;
  below?: ReactNode;
}

export const Row = ({ title, about, children, below }: RowProps) => (
  <div className="border-rim border-b last:border-0">
    <div className="flex items-center gap-6 px-4 py-3.5">
      <div className="flex min-w-0 flex-1 flex-col gap-0.5">
        <p className="text-paper font-sans text-[13.5px]">{title}</p>
        {about ? (
          <p className="text-fog font-sans text-[12.5px] [overflow-wrap:anywhere]">
            {about}
          </p>
        ) : null}
      </div>
      {children}
    </div>
    {below ? <div className="px-4 pb-4">{below}</div> : null}
  </div>
);

export const Card = ({ children }: { children: ReactNode }) => (
  <div className="border-rim bg-raise/50 rounded-xl border">{children}</div>
);
