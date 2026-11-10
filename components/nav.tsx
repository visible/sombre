"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";
import { Fragment, useEffect, useRef, useState } from "react";

import { Access } from "@/components/access";
import { pages } from "@/lib/site";

interface Box {
  left: number;
  width: number;
}

const inset = 16;

export const Nav = () => {
  const pathname = usePathname();
  const [hover, setHover] = useState<number | null>(null);
  const [box, setBox] = useState<Box | null>(null);
  const [open, setOpen] = useState(false);
  const items = useRef<(HTMLAnchorElement | null)[]>([]);
  const root = useRef<HTMLElement>(null);
  const toggle = useRef<HTMLButtonElement>(null);
  const found = pages.findIndex((page) => pathname.startsWith(page.href));
  const active = found === -1 ? null : found;
  const target = hover ?? active;

  useEffect(() => {
    const node = target === null ? null : items.current[target];
    setBox(
      node
        ? { left: node.offsetLeft + inset, width: node.offsetWidth - inset * 2 }
        : null
    );
  }, [target]);

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
      <div
        className="relative hidden items-center md:flex"
        onMouseLeave={() => setHover(null)}
      >
        <ul className="flex items-center">
          {pages.map((page, index) => (
            <Fragment key={page.href}>
              {index > 0 ? (
                <li aria-hidden="true" className="bg-line h-3.5 w-px" />
              ) : null}
              <li>
                <Link
                  aria-current={active === index ? "page" : undefined}
                  className={`flex h-9 items-center px-4 text-[13px] transition-colors duration-150 ${active === index ? "text-ink" : "text-body hover:text-ink"}`}
                  href={page.href}
                  onBlur={() => setHover(null)}
                  onFocus={() => setHover(index)}
                  onMouseEnter={() => setHover(index)}
                  ref={(node) => {
                    items.current[index] = node;
                  }}
                >
                  {page.label}
                </Link>
              </li>
            </Fragment>
          ))}
        </ul>
        <span
          aria-hidden="true"
          className="bg-ember pointer-events-none absolute bottom-1 h-[1.5px] rounded-full transition-[left,width,opacity] duration-300 ease-[cubic-bezier(0.3,0.7,0.2,1)]"
          data-indicator=""
          style={{
            left: box?.left ?? 0,
            opacity: box ? 1 : 0,
            width: box?.width ?? 0,
          }}
        />
      </div>
      <button
        aria-controls="menu"
        aria-expanded={open}
        aria-label={open ? "Close menu" : "Open menu"}
        className="text-ink hover:bg-ink/[0.06] flex size-9 items-center justify-center rounded-full transition-colors md:hidden"
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
