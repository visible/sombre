import type { ReactNode } from "react";

interface Props {
  children: ReactNode;
}

export const Tag = ({ children }: Props) => (
  <p className="text-ink flex items-center gap-2 text-[13px]">
    <span
      aria-hidden="true"
      className="border-ember size-2.5 rounded-full border-2"
    />
    {children}
  </p>
);
