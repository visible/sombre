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

const curve = {
  in: "cubic-bezier(0.5, 0, 0.75, 0)",
  out: "cubic-bezier(0.16, 1, 0.3, 1)",
} as const;

const settle = (node: HTMLElement) => {
  for (const running of node.getAnimations()) {
    running.cancel();
  }
};

export const leave = async () => {
  const node = stage();
  if (!node) {
    return;
  }
  const quick = still();
  settle(node);
  const motion = node.animate(
    [
      { opacity: 1, transform: "translateX(0)" },
      { opacity: 0, transform: `translateX(${quick ? 0 : -36}px)` },
    ],
    { duration: quick ? 120 : 320, easing: curve.in, fill: "forwards" }
  );
  await motion.finished.catch(() => null);
  node.style.visibility = "hidden";
};

export const enter = () => {
  const node = stage();
  if (!node) {
    return;
  }
  const quick = still();
  settle(node);
  node.style.visibility = "";
  node.animate(
    [
      { opacity: 0, transform: `translateX(${quick ? 0 : 36}px)` },
      { opacity: 1, transform: "translateX(0)" },
    ],
    { duration: quick ? 120 : 800, easing: curve.out }
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
