import Link from "next/link";

import { Mark } from "@/components/mark";
import { Nav } from "@/components/nav";

export const Header = () => (
  <header
    className="border-line relative z-30 flex h-16 items-center justify-between gap-6 border-b px-5 sm:px-8"
    id="top"
  >
    <Link
      aria-label="Sombre home"
      className="brand text-ink flex w-fit items-end gap-2.5"
      href="/"
    >
      <Mark className="mb-[6px] h-[17px] w-auto" />
      <span className="text-[20px] font-medium tracking-[-0.035em]">
        sombre
      </span>
    </Link>
    <Nav />
  </header>
);
