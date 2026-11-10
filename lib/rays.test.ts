import { describe, expect, test } from "bun:test";

import { angle, heading, lean, noise, rays } from "@/lib/rays";

const options = {
  count: 48,
  height: 300,
  inner: 40,
  jitter: 0.4,
  outer: 280,
  width: 600,
};

const distance = (x: number, y: number) =>
  Math.hypot(x - options.width / 2, y - options.height);

describe("noise", () => {
  test("stays between zero and one", () => {
    for (let index = 0; index < 500; index += 1) {
      expect(noise(index)).toBeGreaterThanOrEqual(0);
      expect(noise(index)).toBeLessThan(1);
    }
  });

  test("is deterministic", () => {
    expect(noise(7)).toBe(noise(7));
  });
});

describe("rays", () => {
  const lines = rays(options);

  test("draws the requested number of rays", () => {
    expect(lines).toHaveLength(options.count);
  });

  test("returns nothing for zero rays", () => {
    expect(rays({ ...options, count: 0 })).toEqual([]);
  });

  test("renders the same lines every time", () => {
    expect(rays(options)).toEqual(lines);
  });

  test("starts every ray on the inner circle", () => {
    for (const line of lines) {
      expect(distance(line.x1, line.y1)).toBeCloseTo(options.inner, 1);
    }
  });

  test("keeps every ray between the jittered and full length", () => {
    for (const line of lines) {
      const length = distance(line.x2, line.y2);
      expect(length).toBeLessThanOrEqual(options.outer + 0.01);
      expect(length).toBeGreaterThanOrEqual(
        options.outer * (1 - options.jitter) - 0.01
      );
    }
  });

  test("fans only above the horizon", () => {
    for (const line of lines) {
      expect(line.y1).toBeLessThan(options.height);
      expect(line.y2).toBeLessThan(options.height);
    }
  });

  test("is symmetric when jitter is off", () => {
    const even = rays({ ...options, jitter: 0 });
    for (const [index, line] of even.entries()) {
      const mirror = even[even.length - 1 - index];
      expect(line.x2 + (mirror?.x2 ?? 0)).toBeCloseTo(options.width, 1);
      expect(line.y2).toBeCloseTo(mirror?.y2 ?? 0, 1);
    }
  });

  test("stays inside the canvas", () => {
    for (const line of lines) {
      expect(line.x2).toBeGreaterThanOrEqual(0);
      expect(line.x2).toBeLessThanOrEqual(options.width);
      expect(line.y2).toBeGreaterThanOrEqual(0);
    }
  });
});

describe("angle", () => {
  test("spreads rays across the upper half", () => {
    expect(angle(0, 4)).toBeCloseTo(Math.PI * 1.125, 5);
    expect(angle(3, 4)).toBeCloseTo(Math.PI * 1.875, 5);
  });

  test("puts the middle ray straight up", () => {
    expect(angle(1.5, 4)).toBeCloseTo(Math.PI * 1.5, 5);
  });
});

describe("heading", () => {
  const center = { x: 0, y: 0 };

  test("points straight up above the sun", () => {
    expect(heading(center, { x: 0, y: -10 })).toBeCloseTo(Math.PI * 1.5, 5);
  });

  test("points left and right along the horizon", () => {
    expect(heading(center, { x: -10, y: 0 })).toBeCloseTo(Math.PI, 5);
    expect(heading(center, { x: 10, y: -0.0001 })).toBeCloseTo(Math.PI * 2, 3);
  });

  test("always returns a positive angle", () => {
    expect(heading(center, { x: 5, y: -5 })).toBeGreaterThan(0);
  });
});

describe("lean", () => {
  test("is strongest for the ray facing the pointer", () => {
    expect(lean(1, 1, 0.2)).toBe(1);
  });

  test("falls off for rays facing away", () => {
    expect(lean(1, 1.2, 0.2)).toBeCloseTo(Math.exp(-0.5), 5);
    expect(lean(1, 2, 0.2)).toBeLessThan(0.001);
  });

  test("is symmetric", () => {
    expect(lean(1, 1.3, 0.2)).toBeCloseTo(lean(1.3, 1, 0.2), 10);
  });
});
