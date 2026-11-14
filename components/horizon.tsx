"use client";

import { gsap } from "gsap";
import { useEffect, useRef } from "react";

import { boot, still as reduced } from "@/lib/motion";
import { angle, heading, lean, noise } from "@/lib/rays";

type Tone = "ember" | "paper";

interface Props {
  tone?: Tone;
}

interface Scene {
  width: number;
  height: number;
  count: number;
  lengths: Float32Array;
}

const tones = {
  ember: { base: 0.22, disc: "--color-ink", fade: 0.3, reach: 0.58 },
  paper: { base: 0.5, disc: "--color-ember", fade: 1, reach: 0.48 },
} as const;

const progress = (risen: boolean, started: number, now: number) => {
  if (risen) {
    return 1;
  }
  if (started < 0) {
    return 0;
  }
  return Math.min(1, Math.max(0, (now - started) / 1.4));
};

const color = (name: string) =>
  getComputedStyle(document.documentElement).getPropertyValue(name).trim();

export const Horizon = ({ tone = "paper" }: Props) => {
  const box = useRef<HTMLDivElement>(null);
  const canvas = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    boot();
    const frame = box.current;
    const node = canvas.current;
    const context = node?.getContext("2d");
    if (!frame || !node || !context) {
      return;
    }
    const still = reduced();
    const look = tones[tone];
    const ink = color("--color-ink");
    const fill = color(look.disc);
    const scene: Scene = {
      count: 0,
      height: 0,
      lengths: new Float32Array(0),
      width: 0,
    };
    let pointer: { x: number; y: number } | null = null;
    let risen = still;
    let started = -1;
    let running = false;
    let visible = false;

    const shape = () => {
      const { width, height } = scene;
      const disc = Math.min(64, Math.max(34, width * 0.05));
      const outer = Math.min(height * 0.94, width * look.reach);
      return { disc, inner: disc + 12, outer, x: width / 2, y: height };
    };

    const resize = () => {
      const width = frame.clientWidth;
      const height = frame.clientHeight;
      const ratio = Math.min(2, window.devicePixelRatio || 1);
      node.width = Math.round(width * ratio);
      node.height = Math.round(height * ratio);
      context.setTransform(ratio, 0, 0, ratio, 0, 0);
      scene.width = width;
      scene.height = height;
      scene.count = width < 640 ? 54 : 96;
      scene.lengths = new Float32Array(scene.count);
    };

    const draw = (now: number) => {
      const { width, height, count, lengths } = scene;
      const { disc, inner, outer, x, y } = shape();
      const grow = progress(risen, started, now);
      const rise = 1 - (1 - grow) ** 3;
      const target =
        pointer && pointer.y < y ? heading({ x, y }, pointer) : null;
      const near = pointer
        ? Math.max(
            0,
            1 - Math.hypot(pointer.x - x, pointer.y - y) / (outer * 1.6)
          )
        : 0;
      context.clearRect(0, 0, width, height);
      context.strokeStyle = ink;
      context.lineWidth = 1;
      for (let index = 0; index < count; index += 1) {
        const turn = angle(index, count);
        const pull = target === null ? 0 : lean(turn, target, 0.2) * near;
        const drift = still ? 0 : 0.06 * Math.sin(now * 0.6 + index * 0.42);
        const base = outer * (1 - 0.45 * noise(index));
        const goal =
          Math.min(outer * 1.08, base * (1 + 0.42 * pull + drift)) * rise;
        const current = lengths[index] ?? 0;
        const next = still ? goal : current + (goal - current) * 0.14;
        lengths[index] = next;
        const reach = Math.max(inner, next);
        context.globalAlpha = look.base + (1 - look.base) * 0.6 * pull;
        context.beginPath();
        context.moveTo(x + inner * Math.cos(turn), y + inner * Math.sin(turn));
        context.lineTo(x + reach * Math.cos(turn), y + reach * Math.sin(turn));
        context.stroke();
      }
      context.globalAlpha = look.fade;
      context.fillStyle = fill;
      context.beginPath();
      context.arc(x, y + disc * (1 - rise) * 0.8, disc, Math.PI, Math.PI * 2);
      context.fill();
      context.globalAlpha = 1;
      if (grow >= 1) {
        risen = true;
      }
    };

    const tick = (time: number) => draw(time);

    const wake = () => {
      const on = visible && !still;
      if (on === running) {
        return;
      }
      running = on;
      if (on) {
        gsap.ticker.add(tick);
      } else {
        gsap.ticker.remove(tick);
      }
    };

    const move = (event: PointerEvent) => {
      const rect = node.getBoundingClientRect();
      const inside =
        event.clientX >= rect.left &&
        event.clientX <= rect.right &&
        event.clientY >= rect.top - 160 &&
        event.clientY <= rect.bottom;
      pointer = inside
        ? { x: event.clientX - rect.left, y: event.clientY - rect.top }
        : null;
    };

    const leave = () => {
      pointer = null;
    };

    resize();
    draw(gsap.ticker.time);

    const sizes = new ResizeObserver(() => {
      resize();
      draw(gsap.ticker.time);
    });
    sizes.observe(frame);

    const view = new IntersectionObserver(
      ([entry]) => {
        visible = entry?.isIntersecting ?? false;
        if (visible && started < 0 && !still) {
          started = gsap.ticker.time;
        }
        wake();
      },
      { threshold: 0.15 }
    );
    view.observe(frame);

    if (!still) {
      window.addEventListener("pointermove", move, { passive: true });
      document.documentElement.addEventListener("pointerleave", leave);
    }

    return () => {
      gsap.ticker.remove(tick);
      sizes.disconnect();
      view.disconnect();
      window.removeEventListener("pointermove", move);
      document.documentElement.removeEventListener("pointerleave", leave);
    };
  }, [tone]);

  return (
    <div aria-hidden="true" className="absolute inset-0" ref={box}>
      <canvas className="size-full" data-horizon="" ref={canvas} />
    </div>
  );
};
