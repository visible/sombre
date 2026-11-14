"use client";

import { ScrollTrigger } from "gsap/ScrollTrigger";
import { usePathname, useRouter } from "next/navigation";
import { useEffect, useRef } from "react";

import { boot, enter, leave, reveal, scroll } from "@/lib/motion";

type Mode = "idle" | "pop" | "push";

const plain = (event: MouseEvent) =>
  event.button === 0 &&
  !event.defaultPrevented &&
  !event.metaKey &&
  !event.ctrlKey &&
  !event.shiftKey &&
  !event.altKey;

const internal = (event: MouseEvent) => {
  const link =
    event.target instanceof Element ? event.target.closest("a") : null;
  if (!link || link.hasAttribute("download")) {
    return null;
  }
  if (link.target && link.target !== "_self") {
    return null;
  }
  const url = new URL(link.href, window.location.href);
  const same =
    url.origin === window.location.origin &&
    url.pathname !== window.location.pathname;
  return same ? `${url.pathname}${url.search}${url.hash}` : null;
};

export const Motion = () => {
  const router = useRouter();
  const pathname = usePathname();
  const mode = useRef<Mode>("idle");
  const first = useRef(true);
  const rescue = useRef(0);

  useEffect(() => {
    const click = async (event: MouseEvent) => {
      const href = plain(event) ? internal(event) : null;
      if (!href) {
        return;
      }
      event.preventDefault();
      if (mode.current === "push") {
        return;
      }
      mode.current = "push";
      window.clearTimeout(rescue.current);
      router.prefetch(href);
      await leave();
      router.push(href, { scroll: false });
      rescue.current = window.setTimeout(() => {
        if (mode.current === "push") {
          mode.current = "idle";
          enter();
        }
      }, 4000);
    };
    const pop = () => {
      mode.current = "pop";
    };
    document.addEventListener("click", click, true);
    window.addEventListener("popstate", pop);
    return () => {
      document.removeEventListener("click", click, true);
      window.removeEventListener("popstate", pop);
    };
  }, [router]);

  useEffect(() => {
    boot();
    window.clearTimeout(rescue.current);
    const pushed = mode.current === "push";
    mode.current = "idle";
    if (first.current) {
      first.current = false;
    } else if (pushed) {
      const { hash } = window.location;
      scroll(hash && document.querySelector(hash) ? hash : 0, true);
      enter();
    }
    let undo: (() => void) | null = null;
    let live = true;
    const ready = async () => {
      await document.fonts.ready;
      if (live) {
        undo = reveal();
        ScrollTrigger.refresh();
      }
    };
    void ready();
    return () => {
      live = false;
      undo?.();
    };
    // oxlint-disable-next-line react/exhaustive-effect-dependencies -- runs again for every page
  }, [pathname]);

  return null;
};
