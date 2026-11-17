import { gsap } from "gsap";

export const ease = {
  in: "power3.in",
  out: "expo.out",
  soft: "power3.out",
} as const;

export const still = () =>
  window.matchMedia("(prefers-reduced-motion: reduce)").matches;

export const scroll = (target: number | string, instant = false) => {
  const behavior = instant || still() ? "instant" : "smooth";
  if (typeof target === "number") {
    window.scrollTo({ behavior, top: target });
    return;
  }
  document.querySelector(target)?.scrollIntoView({ behavior });
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

type Mode = "idle" | "pop" | "push";

let mode: Mode = "idle";
let rescue = 0;

export const depart = () => {
  if (mode === "push") {
    return false;
  }
  mode = "push";
  window.clearTimeout(rescue);
  return true;
};

export const popped = () => {
  mode = "pop";
};

export const stall = () => {
  rescue = window.setTimeout(() => {
    if (mode === "push") {
      mode = "idle";
      enter();
    }
  }, 4000);
};

export const arrive = () => {
  window.clearTimeout(rescue);
  const pushed = mode === "push";
  mode = "idle";
  if (pushed) {
    const { hash } = window.location;
    scroll(hash && document.querySelector(hash) ? hash : 0, true);
    enter();
  }
};
