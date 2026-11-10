import { describe, expect, test } from "bun:test";

import { devices, history } from "@/lib/history";

const minutes = (time: string) => {
  const [hours, rest] = time.split(":").map(Number);
  return (hours ?? 0) * 60 + (rest ?? 0);
};

describe("history", () => {
  test("has unique ids", () => {
    expect(new Set(history.map((event) => event.id)).size).toBe(history.length);
  });

  test("is in chronological order", () => {
    for (const [index, event] of history.entries()) {
      const previous = history[index - 1];
      if (previous) {
        expect(minutes(event.time)).toBeGreaterThanOrEqual(
          minutes(previous.time)
        );
      }
    }
  });

  test("uses valid clock times", () => {
    expect(history.every((event) => /^\d{1,2}:\d{2}$/u.test(event.time))).toBe(
      true
    );
  });

  test("includes events from both devices", () => {
    const used = new Set<string>(history.map((event) => event.device));
    expect([...used].toSorted()).toEqual(Object.keys(devices).toSorted());
  });

  test("starts on the mac and moves to the phone", () => {
    expect(history.at(0)?.device).toBe("mac");
    const handoff = history.findIndex((event) => event.id === "handoff");
    expect(history[handoff]?.device).toBe("phone");
    expect(
      history.slice(0, handoff).every((event) => event.device === "mac")
    ).toBe(true);
  });

  test("has no empty copy", () => {
    expect(
      history.every(
        (event) =>
          event.title.trim().length > 0 && event.detail.trim().length > 0
      )
    ).toBe(true);
  });

  test("gives phone events a phone screen and mac events none", () => {
    for (const event of history) {
      expect(event.phone !== undefined).toBe(event.device === "phone");
    }
  });

  test("adds every event to the shared transcript", () => {
    for (const event of history) {
      expect(event.rows.length).toBeGreaterThan(0);
      expect(event.rows.every((row) => row.text.trim().length > 0)).toBe(true);
    }
  });
});
