import { gsap } from "gsap";
import { ScrollTrigger } from "gsap/ScrollTrigger";
import { SplitText } from "gsap/SplitText";

import { ease, still } from "@/lib/motion";

gsap.registerPlugin(ScrollTrigger, SplitText);

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
  const context = still()
    ? null
    : gsap.context(() => {
        for (const node of document.querySelectorAll<HTMLElement>(
          "[data-split]"
        )) {
          split(node);
        }
        for (const node of document.querySelectorAll<HTMLElement>(
          "[data-rule]"
        )) {
          gsap.from(node, {
            duration: 1.4,
            ease: ease.out,
            scaleX: 0,
            scrollTrigger: { once: true, start: "top 94%", trigger: node },
            transformOrigin: "left center",
          });
        }
        for (const node of document.querySelectorAll<HTMLElement>(
          "[data-rise]"
        )) {
          gsap.from(node, {
            autoAlpha: 0,
            duration: 1,
            ease: ease.out,
            scrollTrigger: { once: true, start: "top 92%", trigger: node },
            y: 28,
          });
        }
      });
  ScrollTrigger.refresh();
  return () => context?.revert();
};
