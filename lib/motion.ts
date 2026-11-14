import { gsap } from "gsap";
import { ScrollTrigger } from "gsap/ScrollTrigger";
import { SplitText } from "gsap/SplitText";
import Lenis from "lenis";

export const ease = {
  in: "power3.in",
  out: "expo.out",
  soft: "power3.out",
} as const;

export const still = () =>
  window.matchMedia("(prefers-reduced-motion: reduce)").matches;

let lenis: Lenis | null = null;
let booted = false;

export const boot = () => {
  if (booted) {
    return;
  }
  booted = true;
  gsap.registerPlugin(ScrollTrigger, SplitText);
  if (still()) {
    return;
  }
  lenis = new Lenis({ anchors: true, lerp: 0.11 });
  lenis.on("scroll", ScrollTrigger.update);
  gsap.ticker.add((time) => lenis?.raf(time * 1000));
  gsap.ticker.lagSmoothing(0);
};

export const scroll = (target: number | string, instant = false) => {
  if (lenis) {
    lenis.scrollTo(target, { force: true, immediate: instant });
    return;
  }
  if (typeof target === "number") {
    window.scrollTo({ top: target });
    return;
  }
  document.querySelector(target)?.scrollIntoView();
};

const split = (node: HTMLElement) => {
  SplitText.create(node, {
    autoSplit: true,
    mask: "lines",
    onSplit: (self) =>
      gsap.from(self.lines, {
        duration: 1.1,
        ease: ease.out,
        scrollTrigger: { once: true, start: "top 90%", trigger: node },
        stagger: 0.08,
        yPercent: 110,
      }),
    type: "lines",
  });
};

export const reveal = () => {
  if (still()) {
    return () => {
      /* nothing was set up */
    };
  }
  const context = gsap.context(() => {
    for (const node of document.querySelectorAll<HTMLElement>("[data-split]")) {
      split(node);
    }
    for (const node of document.querySelectorAll<HTMLElement>("[data-rule]")) {
      gsap.from(node, {
        duration: 1.4,
        ease: ease.out,
        scaleX: 0,
        scrollTrigger: { once: true, start: "top 94%", trigger: node },
        transformOrigin: "left center",
      });
    }
    for (const node of document.querySelectorAll<HTMLElement>("[data-rise]")) {
      gsap.from(node, {
        autoAlpha: 0,
        duration: 1,
        ease: ease.out,
        scrollTrigger: { once: true, start: "top 92%", trigger: node },
        y: 28,
      });
    }
  });
  return () => context.revert();
};

const stage = () => document.querySelector<HTMLElement>("#stage");

export const leave = async () => {
  const node = stage();
  if (!node) {
    return;
  }
  await gsap.to(node, {
    autoAlpha: 0,
    duration: still() ? 0.12 : 0.32,
    ease: ease.in,
    overwrite: true,
    x: still() ? 0 : -36,
  });
};

export const enter = () => {
  const node = stage();
  if (!node) {
    return;
  }
  gsap.fromTo(
    node,
    { autoAlpha: 0, x: still() ? 0 : 36 },
    {
      autoAlpha: 1,
      clearProps: "all",
      duration: still() ? 0.12 : 0.8,
      ease: ease.out,
      overwrite: true,
      x: 0,
    }
  );
};
