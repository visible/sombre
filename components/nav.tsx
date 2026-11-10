"use client";

import { useEffect, useRef, useState } from "react";

const links = [
  { id: "how", label: "How it works" },
  { id: "history", label: "History" },
  { id: "why", label: "Why Sombre" },
  { id: "questions", label: "Questions" },
] as const;

interface Box {
  left: number;
  width: number;
}

export const Nav = () => {
  const [hover, setHover] = useState<number | null>(null);
  const [active, setActive] = useState<number | null>(null);
  const [box, setBox] = useState<Box | null>(null);
  const items = useRef<(HTMLAnchorElement | null)[]>([]);
  const target = hover ?? active;

  useEffect(() => {
    const observer = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          if (entry.isIntersecting) {
            setActive(links.findIndex((link) => link.id === entry.target.id));
          }
        }
      },
      { rootMargin: "-40% 0px -55% 0px" }
    );
    for (const link of links) {
      const node = document.querySelector(`#${link.id}`);
      if (node) {
        observer.observe(node);
      }
    }
    const top = () => {
      if (window.scrollY < 200) {
        setActive(null);
      }
    };
    window.addEventListener("scroll", top, { passive: true });
    return () => {
      observer.disconnect();
      window.removeEventListener("scroll", top);
    };
  }, []);

  useEffect(() => {
    const node = target === null ? null : items.current[target];
    setBox(node ? { left: node.offsetLeft, width: node.offsetWidth } : null);
  }, [target]);

  return (
    <nav aria-label="Sections" className="hidden md:block">
      <div
        className="bg-ink/[0.06] relative rounded-full p-1"
        onMouseLeave={() => setHover(null)}
      >
        <span
          aria-hidden="true"
          className="absolute top-1 bottom-1 rounded-full bg-white shadow-[0_1px_2px_rgb(15_16_17/0.12),0_4px_12px_-4px_rgb(15_16_17/0.18)] transition-[left,width,opacity] duration-300 ease-[cubic-bezier(0.3,0.7,0.2,1)]"
          data-indicator=""
          style={{
            left: box?.left ?? 0,
            opacity: box ? 1 : 0,
            width: box?.width ?? 0,
          }}
        />
        <ul className="flex items-center">
          {links.map((link, index) => (
            <li key={link.id}>
              <a
                aria-current={active === index ? "location" : undefined}
                className="text-ink relative flex h-8 items-center rounded-full px-3.5 text-[13px]"
                href={`#${link.id}`}
                onBlur={() => setHover(null)}
                onFocus={() => setHover(index)}
                onMouseEnter={() => setHover(index)}
                ref={(node) => {
                  items.current[index] = node;
                }}
              >
                {link.label}
              </a>
            </li>
          ))}
        </ul>
      </div>
    </nav>
  );
};
