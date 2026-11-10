import { describe, expect, test } from "bun:test";

import { linked, moment, schedule, thread } from "@/lib/timeline";
import type { Entry } from "@/lib/timeline";

const sample: Entry[] = [
  { id: "a", kind: "user", pause: 100, text: "a" },
  { icon: "run", id: "b", kind: "step", pause: 250, text: "b" },
  { icon: "edit", id: "c", kind: "step", pause: 50, text: "c" },
  { id: "d", kind: "agent", pause: 300, text: "d" },
];

describe("schedule", () => {
  test("accumulates pauses into delays", () => {
    expect(schedule(sample).map((entry) => entry.delay)).toEqual([
      100, 350, 400, 700,
    ]);
  });

  test("measures how long each entry stays the latest", () => {
    expect(schedule(sample).map((entry) => entry.span)).toEqual([
      250, 50, 300, 0,
    ]);
  });

  test("returns an empty list for an empty thread", () => {
    expect(schedule([])).toEqual([]);
  });

  test("keeps every entry field", () => {
    const [entry] = schedule([
      { detail: "x", icon: "run", id: "a", kind: "step", pause: 10, text: "a" },
    ]);
    expect(entry).toEqual({
      delay: 10,
      detail: "x",
      icon: "run",
      id: "a",
      kind: "step",
      pause: 10,
      span: 0,
      text: "a",
    });
  });

  test("does not mutate its input", () => {
    const input: Entry[] = [{ id: "a", kind: "user", pause: 10, text: "a" }];
    schedule(input);
    expect(input).toEqual([{ id: "a", kind: "user", pause: 10, text: "a" }]);
  });
});

describe("moment", () => {
  test("finds the delay of the first entry of a kind", () => {
    const entries = schedule(thread);
    const handoff = entries.find((entry) => entry.kind === "handoff");
    expect(moment(entries, "handoff")).toBe(handoff?.delay ?? -1);
  });

  test("falls back to zero when the kind is missing", () => {
    expect(moment(schedule(sample), "handoff")).toBe(0);
  });
});

describe("linked", () => {
  const entries = schedule(sample);

  test("joins a step to the step after it", () => {
    expect(linked(entries, 1)).toBe(true);
  });

  test("does not join a step to a message", () => {
    expect(linked(entries, 2)).toBe(false);
  });

  test("does not join messages or run past the end", () => {
    expect(linked(entries, 0)).toBe(false);
    expect(linked(entries, 3)).toBe(false);
    expect(linked(entries, 9)).toBe(false);
  });
});

describe("thread", () => {
  const entries = schedule(thread);

  test("has unique ids", () => {
    expect(new Set(thread.map((entry) => entry.id)).size).toBe(thread.length);
  });

  test("has exactly one handoff with messages on both sides", () => {
    const index = thread.findIndex((entry) => entry.kind === "handoff");
    expect(thread.filter((entry) => entry.kind === "handoff")).toHaveLength(1);
    expect(thread.slice(0, index).some((entry) => entry.kind === "user")).toBe(
      true
    );
    expect(thread.slice(index + 1).some((entry) => entry.kind === "user")).toBe(
      true
    );
  });

  test("gives every step an icon", () => {
    const steps = thread.filter((entry) => entry.kind === "step");
    expect(steps.length).toBeGreaterThan(0);
    expect(steps.every((entry) => entry.icon !== undefined)).toBe(true);
  });

  test("leaves room for the handoff to travel", () => {
    const handoff = thread.find((entry) => entry.kind === "handoff");
    expect(handoff?.pause ?? 0).toBeGreaterThanOrEqual(1500);
  });

  test("starts with the user and ends with the agent", () => {
    expect(thread.at(0)?.kind).toBe("user");
    expect(thread.at(-1)?.kind).toBe("agent");
  });

  test("plays in strictly increasing order", () => {
    for (const [index, entry] of entries.entries()) {
      expect(entry.delay).toBeGreaterThan(
        index === 0 ? 0 : (entries[index - 1]?.delay ?? 0)
      );
    }
  });

  test("finishes within fourteen seconds", () => {
    expect(entries.at(-1)?.delay ?? 0).toBeLessThanOrEqual(14_000);
  });

  test("has no empty text", () => {
    expect(thread.every((entry) => entry.text.trim().length > 0)).toBe(true);
  });
});
