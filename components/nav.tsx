"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";
import { Fragment, useEffect, useRef, useState } from "react";

import { Access } from "@/components/access";
import { pages } from "@/lib/site";

export const Nav = () => {
  const pathname = usePathname();
  const [open, setOpen] = useState(false);
  const root = useRef<HTMLElement>(null);
  const toggle = useRef<HTMLButtonElement>(null);
  const found = pages.findIndex((page) => pathname.startsWith(page.href));
  const active = found === -1 ? null : found;

  useEffect(() => {
    if (!open) {
      return;
    }
    const away = (event: PointerEvent) => {
      if (
        event.target instanceof Node &&
        !root.current?.contains(event.target)
      ) {
        setOpen(false);
      }
    };
    const escape = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        setOpen(false);
        toggle.current?.focus();
      }
    };
    document.addEventListener("pointerdown", away);
    document.addEventListener("keydown", escape);
    return () => {
      document.removeEventListener("pointerdown", away);
      document.removeEventListener("keydown", escape);
    };
  }, [open]);

  return (
    <nav aria-label="Pages" className="flex items-center gap-4" ref={root}>
      <ul className="hidden items-center md:flex">
        {pages.map((page, index) => (
          <Fragment key={page.href}>
            {index > 0 ? (
              <li aria-hidden="true" className="bg-line h-3.5 w-px" />
            ) : null}
            <li>
              <Link
                aria-current={active === index ? "page" : undefined}
                aria-label={page.label}
                className={`pill tab flex h-9 items-center px-4 text-[13px] ${active === index ? "text-ink" : "text-body"}`}
                href={page.href}
              >
                <span className="roll">
                  <span>{page.label}</span>
                  <span aria-hidden="true" className="text-ink">
                    {page.label}
                  </span>
                </span>
              </Link>
            </li>
          </Fragment>
        ))}
      </ul>
      <button
        aria-controls="menu"
        aria-expanded={open}
        aria-label={open ? "Close menu" : "Open menu"}
        className="text-ink hover:bg-ink/[0.06] flex size-11 items-center justify-center rounded-full transition-colors md:hidden"
        onClick={() => setOpen((value) => !value)}
        ref={toggle}
        type="button"
      >
        <span aria-hidden="true" className="relative block h-[8px] w-[16px]">
          <span
            className={`bg-ink absolute left-0 h-[1.5px] w-full transition-transform duration-200 motion-reduce:transition-none ${open ? "top-[3.25px] rotate-45" : "top-0"}`}
          />
          <span
            className={`bg-ink absolute left-0 h-[1.5px] w-full transition-transform duration-200 motion-reduce:transition-none ${open ? "top-[3.25px] -rotate-45" : "top-[6.5px]"}`}
          />
        </span>
      </button>
      <Access size="small" />
      <div
        className={`border-line bg-paper absolute inset-x-0 top-full z-30 grid border-b transition-[grid-template-rows,visibility] duration-300 ease-[cubic-bezier(0.3,0.7,0.2,1)] motion-reduce:transition-none md:hidden ${open ? "grid-rows-[1fr]" : "invisible grid-rows-[0fr] border-b-0"}`}
        id="menu"
      >
        <ul className="divide-line min-h-0 divide-y overflow-hidden">
          {pages.map((page, index) => (
            <li key={page.href}>
              <Link
                aria-current={active === index ? "page" : undefined}
                className="text-ink active:bg-sheet relative flex h-14 items-center px-5 text-[20px] font-medium tracking-[-0.02em] focus-visible:outline-offset-[-4px]"
                href={page.href}
                onClick={() => setOpen(false)}
                tabIndex={open ? undefined : -1}
              >
                {active === index ? (
                  <span
                    aria-hidden="true"
                    className="bg-ember absolute top-3 bottom-3 left-0 w-[2px]"
                  />
                ) : null}
                {page.label}
              </Link>
            </li>
          ))}
        </ul>
      </div>
    </nav>
  );
};
