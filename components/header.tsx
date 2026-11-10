import Link from "next/link";

import { Access } from "@/components/access";
import { Mark } from "@/components/mark";
import { Nav } from "@/components/nav";

export const Header = () => (
  <header
    className="border-line grid h-16 grid-cols-[1fr_auto] items-center gap-6 border-b px-5 sm:px-8 md:grid-cols-[1fr_auto_1fr]"
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
    <div className="justify-self-end">
      <Access size="small" />
    </div>
  </header>
);
